//! Source modules are resolved before type checking; no build scripts or code run
//! during loading. Each file is a module rooted at the entry file's directory.
use crate::ast::*;
use crate::diag::{self, Diags, Span};
use crate::{lexer, parser};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const PREFIX: &str = "__yvm_";

pub fn file_id(files: &[SourceFile], span: Span) -> usize {
    files.partition_point(|f| f.start <= span.start).saturating_sub(1)
}

pub struct Project {
    pub program: Program,
    pub source: String,
    pub modular: bool,
    pub relative_paths: Vec<PathBuf>,
}
struct Unit {
    canonical: PathBuf,
    display: PathBuf,
    source: String,
}
struct Loader {
    root: PathBuf,
    display_root: PathBuf,
    units: Vec<Unit>,
    seen: HashMap<PathBuf, usize>,
    links: HashMap<(usize, String), usize>,
    errors: Vec<(usize, Span, &'static str, String)>,
    syntax_errors: Vec<(usize, diag::Diagnostic)>,
    bytes: usize,
    modular: bool,
    root_program: Option<Program>,
}

impl Loader {
    fn discover(&mut self, index: usize, depth: usize) {
        let mut ignored = Diags::default();
        let source = &self.units[index].source;
        let program = parser::parse(lexer::lex_file(source, &mut ignored), source, &mut ignored);
        self.modular |= program.uses.iter().any(|u| !u.path.starts_with("std/"));
        if index == 0 && !self.modular {
            self.syntax_errors.extend(ignored.items.into_iter().map(|d| (index, d)));
            self.root_program = Some(program);
            return;
        }
        if ignored.has_errors() {
            self.syntax_errors.extend(ignored.items.into_iter().map(|d| (index, d)));
            return;
        }
        for u in program.uses {
            if u.path.starts_with("std/") { continue; }
            self.modular = true;
            if depth >= 128 || self.units.len() >= 1024 || self.bytes > 16 * 1024 * 1024 {
                self.errors.push((index, u.span, "U0001", "module graph exceeds the implementation limit (128 levels, 1024 files, 16 MiB)".into()));
                continue;
            }
            let relative = PathBuf::from(format!("{}.yv", u.path));
            let candidate = self.root.join(&relative);
            let canonical = match fs::canonicalize(&candidate) {
                Ok(path) if path.starts_with(&self.root) => path,
                Ok(_) => {
                    self.errors.push((index, u.span, "E0010", format!("module `{}` resolves outside the project directory", u.path)));
                    continue;
                }
                Err(e) => {
                    self.errors.push((index, u.span, "E0010", format!("cannot load module `{}` ({}): {e}", u.path, candidate.display())));
                    continue;
                }
            };
            if canonical != candidate {
                self.errors.push((index, u.span, "U0001", format!("module `{}` uses a symbolic-link alias; import its real project-relative path", u.path)));
                continue;
            }
            if let Some(&id) = self.seen.get(&canonical) {
                self.links.insert((index, u.path), id);
                continue;
            }
            let bytes = match fs::read(&canonical) {
                Ok(bytes) => bytes,
                Err(e) => {
                    self.errors.push((index, u.span, "E0010", format!("cannot read module `{}`: {e}", u.path)));
                    continue;
                }
            };
            let text = match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(_) => {
                    self.errors.push((index, u.span, "E0020", format!("module `{}` is not valid UTF-8", u.path)));
                    continue;
                }
            };
            self.bytes += text.len();
            let id = self.units.len();
            self.seen.insert(canonical.clone(), id);
            self.links.insert((index, u.path), id);
            self.units.push(Unit { canonical, display: self.display_root.join(relative), source: text });
            self.discover(id, depth + 1);
        }
    }
}

impl Project {
    /// The root text is supplied by the caller so it is read and hashed once.
    pub fn load(input: &Path, root_source: &str, diags: &mut Diags) -> io::Result<Self> {
        let canonical = fs::canonicalize(input)?;
        let root = canonical.parent().unwrap().to_path_buf();
        let display_root = input.parent().unwrap_or_else(|| Path::new("")).to_path_buf();
        let mut loader = Loader {
            root: root.clone(), display_root,
            units: vec![Unit { canonical: canonical.clone(), display: input.to_path_buf(), source: root_source.into() }],
            seen: HashMap::from([(canonical, 0)]), links: HashMap::new(), errors: Vec::new(), syntax_errors: Vec::new(),
            bytes: root_source.len(), modular: false, root_program: None,
        };
        loader.discover(0, 0);
        let mut source = String::new();
        let mut files = Vec::new();
        let mut relative_paths = Vec::new();
        let mut line = 1u32;
        for unit in &loader.units {
            if !files.is_empty() {
                source.push_str("\n\n");
                line += 2;
            }
            let start = source.len();
            let start_line = line;
            source.push_str(&unit.source);
            line += unit.source.bytes().filter(|b| *b == b'\n').count() as u32;
            files.push(SourceFile {
                path: unit.display.to_string_lossy().replace('\\', "/"),
                start, end: source.len(), line: start_line,
            });
            relative_paths.push(unit.canonical.strip_prefix(&root).unwrap().to_path_buf());
        }
        for (id, mut span, code, message) in loader.errors {
            span.start += files[id].start;
            span.end += files[id].start;
            span.line += files[id].line - 1;
            diags.error(code, span, message);
        }
        let bad_syntax = !loader.syntax_errors.is_empty();
        for (id, mut diagnostic) in loader.syntax_errors {
            diagnostic.span.start += files[id].start;
            diagnostic.span.end += files[id].start;
            diagnostic.span.line += files[id].line - 1;
            diags.items.push(diagnostic);
        }
        if let Some(mut program) = loader.root_program {
            program.files = files;
            return Ok(Self { program, source, modular: false, relative_paths });
        }
        if bad_syntax {
            return Ok(Self { program: Program { files, fns: Vec::new(), structs: Vec::new(),
                enums: Vec::new(), interfaces: Vec::new(), uses: Vec::new() },
                source, modular: loader.modular, relative_paths });
        }
        // Parsing the combined immutable snapshot keeps every nested span,
        // including interpolation and generic bodies, in one coordinate system.
        let tokens = lexer::lex_file(&source, diags);
        let mut program = parser::parse(tokens, &source, diags);
        program.files = files;
        if loader.modular {
            resolve(&mut program, &loader.links, diags);
        }
        Ok(Self { program, source, modular: loader.modular, relative_paths })
    }

    pub fn root_source(&self) -> &str {
        let f = &self.program.files[0];
        &self.source[f.start..f.end]
    }

    pub fn manifest(&self) -> Value {
        json!(self.program.files.iter().zip(&self.relative_paths).map(|(f, relative)| {
            json!({"path":f.path, "relative_path":relative.to_string_lossy().replace('\\', "/"),
                "sha256":diag::source_hash(&self.source[f.start..f.end]), "bytes":f.end-f.start})
        }).collect::<Vec<_>>())
    }

    pub fn hash(&self) -> String {
        let mut hash = Sha256::new();
        for (f, relative) in self.program.files.iter().zip(&self.relative_paths) {
            let path = relative.to_string_lossy().replace('\\', "/");
            let text = &self.source[f.start..f.end];
            hash.update((path.len() as u64).to_le_bytes()); hash.update(path.as_bytes());
            hash.update((text.len() as u64).to_le_bytes()); hash.update(text.as_bytes());
        }
        format!("{:x}", hash.finalize())
    }

    pub fn snapshot(&self, directory: &Path) -> io::Result<PathBuf> {
        for (f, relative) in self.program.files.iter().zip(&self.relative_paths) {
            let path = directory.join(relative);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, &self.source[f.start..f.end])?;
        }
        Ok(directory.join(&self.relative_paths[0]))
    }

    pub fn json(&self, diags: &Diags) -> Value {
        if !self.modular {
            return diag::render_json_value(&self.program.files[0].path, self.root_source(), diags);
        }
        let mut items = Vec::new();
        for (id, f) in self.program.files.iter().enumerate() {
            let local = self.local_diags(diags, id);
            let rendered = diag::render_json_value(&f.path, &self.source[f.start..f.end], &local);
            items.extend(rendered["diagnostics"].as_array().unwrap().iter().cloned());
        }
        json!({"schema":"yavo-diagnostics/p0", "file":self.program.files[0].path,
            "source_sha256":diag::source_hash(self.root_source()), "project_sha256":self.hash(),
            "sources":self.manifest(), "error_count":diags.count(diag::Severity::Error),
            "warning_count":diags.count(diag::Severity::Warning), "diagnostics":items})
    }

    pub fn human(&self, diags: &Diags) -> String {
        let mut out = String::new();
        for (id, f) in self.program.files.iter().enumerate() {
            let local = self.local_diags(diags, id);
            if !local.items.is_empty() {
                out.push_str(&diag::render_human(&f.path, &self.source[f.start..f.end], &local));
            }
        }
        out
    }

    fn display_names(&self, text: &str) -> String {
        if !self.modular { return text.to_string(); }
        let mut out = text.to_string();
        for (id, relative) in self.relative_paths.iter().enumerate() {
            let module = relative.with_extension("").to_string_lossy().replace('\\', "/");
            let display = if id == 0 { String::new() } else { format!("{module}.") };
            out = out.replace(&format!("{PREFIX}{id}_"), &display);
        }
        out
    }

    fn local_diags(&self, diags: &Diags, id: usize) -> Diags {
        let f = &self.program.files[id];
        let mut local = Diags::default();
        for d in &diags.items {
            if file_id(&self.program.files, d.span) != id { continue; }
            let mut d = d.clone();
            d.message = self.display_names(&d.message);
            if self.modular {
                for note in &mut d.notes {
                    *note = self.display_names(note);
                    if let Some(at) = note.find("on line ") {
                        let first = at + "on line ".len();
                        let digits = note[first..].bytes().take_while(u8::is_ascii_digit).count();
                        if let Ok(line) = note[first..first + digits].parse::<u32>() {
                            if let Some(source) = self.program.files.iter().rev().find(|f| f.line <= line) {
                                note.replace_range(at..first + digits, &format!("at {}:{}", source.path, line - source.line + 1));
                            }
                        }
                    }
                }
            }
            d.span.start = d.span.start.saturating_sub(f.start);
            d.span.end = d.span.end.saturating_sub(f.start);
            d.span.line = d.span.line.saturating_sub(f.line - 1);
            // Multi-file repairs need a project-wide recheck and are not issued
            // by the single-file repair planner.
            if self.modular { d.fixes.clear(); }
            local.items.push(d);
        }
        local
    }

    pub fn query_name(&self, name: &str) -> String {
        if let Some((module, symbol)) = name.split_once('.') {
            let mut matches = Vec::new();
            for (id, path) in self.relative_paths.iter().enumerate().skip(1) {
                let module_path = path.with_extension("").to_string_lossy().replace('\\', "/");
                if module_path == module { return format!("{PREFIX}{id}_{symbol}"); }
                if path.file_stem().is_some_and(|n| n == module) { matches.push(id); }
            }
            if matches.len() == 1 { return format!("{PREFIX}{}_{symbol}", matches[0]); }
        }
        if self.modular && name != "main" {
            let internal = format!("{PREFIX}0_{name}");
            if self.program.fns.iter().any(|f| f.name == internal) { return internal; }
        }
        name.to_string()
    }

    pub fn map_operation_locations(&self, value: &mut Value) {
        match value {
            Value::Object(object) => {
                if let Some(line) = object.get("line").and_then(Value::as_u64) {
                    if object.contains_key("file") {
                        if let Some(f) = self.program.files.iter().rev().find(|f| f.line as u64 <= line) {
                            object.insert("file".into(), json!(f.path));
                            object.insert("line".into(), json!(line - f.line as u64 + 1));
                            for key in ["start", "end"] {
                                if let Some(offset) = object.get(key).and_then(Value::as_u64) {
                                    object.insert(key.into(), json!(offset.saturating_sub(f.start as u64)));
                                }
                            }
                        }
                    }
                }
                for child in object.values_mut() { self.map_operation_locations(child); }
            }
            Value::Array(items) => for item in items { self.map_operation_locations(item); },
            _ => {}
        }
    }
}

#[derive(Clone)]
struct Symbol { name: String, public: bool, ty: bool }
#[derive(Clone)]
struct Import { target: usize, span: Span, used: bool }
struct Resolver<'a> {
    symbols: &'a [HashMap<String, Symbol>],
    imports: &'a mut [HashMap<String, Import>],
    module: usize,
    types: HashSet<String>,
    locals: Vec<HashSet<String>>,
    diags: &'a mut Diags,
}

fn resolve(program: &mut Program, links: &HashMap<(usize, String), usize>, diags: &mut Diags) {
    crate::check::check_declaration_names(program, diags);
    let mut symbols = vec![HashMap::new(); program.files.len()];
    let mut add = |name: &str, span: Span, public: bool, ty: bool| {
        let id = file_id(&program.files, span);
        if name.starts_with(PREFIX) {
            diags.error("E0010", span, "this name uses the compiler-reserved module prefix");
        }
        let target = if id == 0 && name == "main" && !ty { name.to_string() } else { format!("{PREFIX}{id}_{name}") };
        if symbols[id].insert(name.to_string(), Symbol { name: target, public, ty }).is_some() {
            diags.error("E0011", span, format!("declaration `{name}` is defined more than once in this module"));
        }
    };
    for f in &program.fns { if f.owner.is_none() { add(&f.name, f.name_span, f.public, false); } }
    for s in &program.structs { add(&s.name, s.name_span, s.public, true); }
    for e in &program.enums { add(&e.name, e.name_span, e.public, true); }
    for i in &program.interfaces { add(&i.name, i.name_span, i.public, true); }
    let mut imports = vec![HashMap::new(); program.files.len()];
    let mut aliases = HashSet::new();
    for u in &program.uses {
        let id = file_id(&program.files, u.span);
        if !aliases.insert((id, u.module.clone())) {
            // Standard-library duplicates remain the checker's responsibility.
            if !u.path.starts_with("std/") || imports[id].contains_key(&u.module) {
                diags.error("E0011", u.span, format!("module name `{}` is imported more than once", u.module));
            }
        }
        if let Some(&target) = links.get(&(id, u.path.clone())) {
            if symbols[id].contains_key(&u.module) {
                diags.error("E0011", u.span, format!("module name `{}` conflicts with a local declaration", u.module));
            }
            imports[id].insert(u.module.clone(), Import { target, span: u.span, used: false });
        }
    }
    let owner_params: HashMap<_, _> = program.structs.iter().map(|s| {
        ((file_id(&program.files, s.name_span), s.name.clone()), s.type_params.iter().map(|p| p.name.clone()).collect::<HashSet<_>>())
    }).collect();
    for s in &mut program.structs {
        let id = file_id(&program.files, s.name_span);
        let mut r = Resolver::new(&symbols, &mut imports, id, diags);
        r.types.extend(s.type_params.iter().map(|p| p.name.clone()));
        r.params(&mut s.type_params);
        for f in &mut s.fields { r.ty(&mut f.ty); }
        for (name, span) in &mut s.impls { *name = r.name(name, *span, true); }
        s.name = symbols[id][&s.name].name.clone();
    }
    for e in &mut program.enums {
        let id = file_id(&program.files, e.name_span);
        let mut r = Resolver::new(&symbols, &mut imports, id, diags);
        for v in &mut e.variants { for f in &mut v.fields { r.ty(&mut f.ty); } }
        for (name, span) in &mut e.impls { *name = r.name(name, *span, true); }
        e.name = symbols[id][&e.name].name.clone();
    }
    for interface in &mut program.interfaces {
        let id = file_id(&program.files, interface.name_span);
        let mut r = Resolver::new(&symbols, &mut imports, id, diags);
        for method in &mut interface.methods { r.function(method); }
        interface.name = symbols[id][&interface.name].name.clone();
    }
    for f in &mut program.fns {
        let id = file_id(&program.files, f.name_span);
        let mut r = Resolver::new(&symbols, &mut imports, id, diags);
        if let Some(owner) = &f.owner {
            if let Some(params) = owner_params.get(&(id, owner.clone())) { r.types.extend(params.iter().cloned()); }
        }
        r.function(f);
    }
    for map in &imports {
        let mut unused: Vec<_> = map.iter().filter(|(_, i)| !i.used).collect();
        unused.sort_by_key(|(_, i)| i.span.start);
        for (name, i) in unused { diags.warn("W0003", i.span, format!("unused import `{name}`")).note("remove it"); }
    }
    program.uses.retain(|u| u.path.starts_with("std/"));
}

impl<'a> Resolver<'a> {
    fn new(symbols: &'a [HashMap<String, Symbol>], imports: &'a mut [HashMap<String, Import>], module: usize, diags: &'a mut Diags) -> Self {
        Self { symbols, imports, module, types: HashSet::new(), locals: vec![HashSet::new()], diags }
    }
    fn bound(&self, name: &str) -> bool { self.locals.iter().rev().any(|s| s.contains(name)) }
    fn bind(&mut self, name: &str) { self.locals.last_mut().unwrap().insert(name.into()); }
    fn name(&mut self, name: &str, span: Span, type_only: bool) -> String {
        if self.types.contains(name) { return name.into(); }
        if name.starts_with(PREFIX) {
            self.diags.error("E0010", span, "compiler-generated module names cannot be referenced in source");
            return name.into();
        }
        if let Some((module, member)) = name.split_once('.') {
            if let Some(import) = self.imports[self.module].get_mut(module) {
                import.used = true;
                match self.symbols[import.target].get(member) {
                    Some(symbol) if symbol.public && (!type_only || symbol.ty) => return symbol.name.clone(),
                    Some(_) => { self.diags.error("E0010", span, format!("`{name}` is private or is not an exported type")); }
                    None => { self.diags.error("E0010", span, format!("module `{module}` has no exported declaration `{member}`")); }
                }
                return name.into();
            }
        }
        if let Some(symbol) = self.symbols[self.module].get(name) {
            return symbol.name.clone();
        }
        let mut candidates: Vec<_> = self.imports[self.module].iter().filter_map(|(alias, import)| {
            self.symbols[import.target].get(name).filter(|s| s.ty && s.public).map(|s| (alias.clone(), s.name.clone()))
        }).collect();
        candidates.sort();
        if candidates.len() == 1 {
            self.imports[self.module].get_mut(&candidates[0].0).unwrap().used = true;
            return candidates[0].1.clone();
        }
        if candidates.len() > 1 {
            self.diags.error("E0010", span, format!("type `{name}` is ambiguous; qualify it with its module name"));
        }
        if self.module != 0 && name == "main" {
            self.diags.error("E0010", span, "the entry function is not in this module; use an explicit public import");
        }
        name.into()
    }
    fn ty(&mut self, ty: &mut TypeExpr) {
        ty.name = self.name(&ty.name, ty.span, true);
        for arg in &mut ty.args { self.ty(arg); }
    }
    fn params(&mut self, params: &mut [TypeParam]) {
        for p in params { for (bound, span) in &mut p.bounds { *bound = self.name(bound, *span, true); } }
    }
    fn function(&mut self, f: &mut FnDecl) {
        self.types.extend(f.type_params.iter().map(|p| p.name.clone()));
        self.params(&mut f.type_params);
        if let Some(owner) = &mut f.owner {
            let new_owner = self.name(owner, f.name_span, true);
            let method = f.name.rsplit('.').next().unwrap();
            f.name = format!("{new_owner}.{method}");
            *owner = new_owner;
        } else {
            f.name = self.name(&f.name, f.name_span, false);
        }
        for p in &mut f.params { self.ty(&mut p.ty); self.bind(&p.name); }
        if let Some(ret) = &mut f.ret { self.ty(ret); }
        self.block(&mut f.body);
    }
    fn pattern(&mut self, pat: &mut Pattern) {
        match pat {
            Pattern::Bind { name, .. } => self.bind(name),
            Pattern::Some(p) | Pattern::Ok(p) | Pattern::Err(p) => self.pattern(p),
            Pattern::Case { ty, args, .. } => {
                if let Some((name, span)) = ty { *name = self.name(name, *span, true); }
                for arg in args { self.pattern(arg); }
            }
            Pattern::None | Pattern::Wild => {}
        }
    }
    fn block(&mut self, block: &mut Block) {
        self.locals.push(HashSet::new());
        for stmt in &mut block.stmts { self.stmt(stmt); }
        self.locals.pop();
    }
    fn otherwise(&mut self, els: &mut Option<Else>) {
        match els { Some(Else::Block(b)) => self.block(b), Some(Else::If(s)) => self.stmt(s), None => {} }
    }
    fn stmt(&mut self, stmt: &mut Stmt) {
        match &mut stmt.kind {
            StmtKind::Let { name, ty, init, .. } => {
                if let Some(t) = ty { self.ty(t); }
                self.expr(init); self.bind(name);
            }
            StmtKind::LetTuple { names, init, .. } => { self.expr(init); for (name, _) in names { self.bind(name); } }
            StmtKind::Assign { target, value, .. } => { self.expr(target); self.expr(value); }
            StmtKind::Expr(e) | StmtKind::Return(Some(e)) => self.expr(e),
            StmtKind::If { cond, then, els } => { self.expr(cond); self.block(then); self.otherwise(els); }
            StmtKind::IfLet { name, value, then, els, .. } => {
                self.expr(value); self.locals.push(HashSet::new()); self.bind(name); self.block(then); self.locals.pop(); self.otherwise(els);
            }
            StmtKind::Match { value, arms } => {
                self.expr(value);
                for arm in arms { self.locals.push(HashSet::new()); self.pattern(&mut arm.pat); self.block(&mut arm.body); self.locals.pop(); }
            }
            StmtKind::While { cond, body } => { self.expr(cond); self.block(body); }
            StmtKind::Loop { body } | StmtKind::Block(body) | StmtKind::Scope { body } => self.block(body),
            StmtKind::For { var, start, end, body, .. } => {
                self.expr(start); self.expr(end); self.locals.push(HashSet::new()); self.bind(var); self.block(body); self.locals.pop();
            }
            StmtKind::ForIn { var, var2, iter, body, .. } => {
                self.expr(iter); self.locals.push(HashSet::new()); self.bind(var);
                if let Some((name, _)) = var2 { self.bind(name); }
                self.block(body); self.locals.pop();
            }
            StmtKind::With { name, init, body, .. } => {
                self.expr(init); self.locals.push(HashSet::new()); self.bind(name); self.block(body); self.locals.pop();
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
        }
    }
    fn expr(&mut self, e: &mut Expr) {
        // Qualified module names are resolved before traversing their base.
        if let ExprKind::Field { base, name } = &e.kind {
            if let ExprKind::Ident(module) = &base.kind {
                if !self.bound(module) && self.imports[self.module].contains_key(module) {
                    e.kind = ExprKind::Ident(self.name(&format!("{module}.{name}"), e.span, false));
                    return;
                }
                if !self.bound(module) && !self.types.contains(module)
                    && !self.symbols[self.module].contains_key(module)
                    && !self.imports[self.module].values().any(|import| self.symbols[import.target].get(module).is_some_and(|s| s.public && s.ty))
                    && !crate::check::is_builtin_namespace(module) {
                    self.diags.error("E0010", base.span, format!("module or value `{module}` is not in scope"));
                    return;
                }
            }
        }
        match &mut e.kind {
            ExprKind::Ident(name) => { if !self.bound(name) { *name = self.name(name, e.span, false); } }
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Byte(_) | ExprKind::Bool(_)
            | ExprKind::None | ExprKind::Break | ExprKind::Continue | ExprKind::Unit => {}
            ExprKind::Str(parts) => for part in parts { if let StrSeg::Expr { expr, .. } = part { self.expr(expr); } },
            ExprKind::Call { callee, args } => { self.expr(callee); for arg in args { self.expr(arg); } }
            ExprKind::Field { base, .. } => self.expr(base),
            ExprKind::StructLit { name, name_span, targs, fields } => {
                *name = self.name(name, *name_span, true);
                for t in targs { self.ty(t); }
                for f in fields { self.expr(&mut f.value); }
            }
            ExprKind::Generic { name, targs } => {
                *name = self.name(name, e.span, false);
                for t in targs { self.ty(t); }
            }
            ExprKind::VecLit(items) | ExprKind::Tuple(items) => for item in items { self.expr(item); },
            ExprKind::MapLit(items) => for (key, value) in items { self.expr(key); self.expr(value); },
            ExprKind::Index { base, index } => { self.expr(base); self.expr(index); }
            ExprKind::SliceRange { base, start, end } => { self.expr(base); self.expr(start); self.expr(end); }
            ExprKind::Borrow(v) | ExprKind::Paren(v) | ExprKind::Err(v) | ExprKind::Try(v) | ExprKind::Spawn(v)
            | ExprKind::Unary { expr: v, .. } | ExprKind::Named { value: v, .. } => self.expr(v),
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::Coalesce { lhs, rhs, .. } => { self.expr(lhs); self.expr(rhs); }
            ExprKind::Return(value) => if let Some(v) = value { self.expr(v); },
            ExprKind::Closure { params, body } => {
                self.locals.push(HashSet::new());
                for (name, _, ty) in params { if let Some(t) = ty { self.ty(t); } self.bind(name); }
                self.expr(body); self.locals.pop();
            }
            ExprKind::Case { args, .. } => for arg in args.iter_mut().flatten() { self.expr(arg); },
            ExprKind::Catch { value, name, handler, .. } => {
                self.expr(value); self.locals.push(HashSet::new()); self.bind(name); self.expr(handler); self.locals.pop();
            }
            ExprKind::Match { value, arms } => {
                self.expr(value);
                for arm in arms { self.locals.push(HashSet::new()); self.pattern(&mut arm.pat); self.expr(&mut arm.body); self.locals.pop(); }
            }
            ExprKind::Block(block) => self.block(block),
        }
    }
}
