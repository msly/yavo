//! Conservative, syntax-preserving repair proposals. The caller must validate
//! the complete automatic edit set and each candidate with a fresh type check.
use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;
use crate::diag::{Diags, Edit, Fix, Span};
use crate::lexer::{self, Tok, Token};

pub struct Proposal {
    pub code: &'static str,
    pub diagnostic_start: usize,
    pub fix: Fix,
}

struct Binding<'a> {
    name: &'a str,
    span: Span,
    references: Vec<Span>,
    ty: Option<&'a TypeExpr>,
    init: Option<&'a Expr>,
    statement: Option<&'a Stmt>,
    mutable: bool,
}

/// Resolve source identifiers to bindings before proposing renames. Merely
/// replacing identifier tokens would also rename fields, shadowing closure
/// parameters, or names in another match arm.
struct Function<'a> {
    declaration: &'a FnDecl,
    bindings: Vec<Binding<'a>>,
    scopes: Vec<BTreeMap<String, usize>>,
    uses: BTreeMap<usize, usize>,
    names: BTreeSet<String>,
    statements: Vec<&'a Stmt>,
    expressions: Vec<&'a Expr>,
}

impl<'a> Function<'a> {
    fn new(f: &'a FnDecl) -> Self {
        let mut this = Self {
            declaration: f,
            bindings: Vec::new(),
            scopes: vec![BTreeMap::new()],
            uses: BTreeMap::new(),
            names: BTreeSet::new(),
            statements: Vec::new(),
            expressions: Vec::new(),
        };
        for p in &f.params {
            this.bind(&p.name, p.span, Some(&p.ty), None, None, p.inout);
        }
        if let Some(sources) = &f.ret_of {
            for (name, span) in sources {
                // In a source clause, static is the lifetime marker even when
                // a parameter happens to have that spelling.
                if name != "static" {
                    this.reference(name, *span);
                }
            }
        }
        this.block(&f.body);
        this
    }

    fn bind(
        &mut self,
        name: &'a str,
        span: Span,
        ty: Option<&'a TypeExpr>,
        init: Option<&'a Expr>,
        statement: Option<&'a Stmt>,
        mutable: bool,
    ) {
        self.names.insert(name.to_string());
        if name == "_" {
            return;
        }
        let id = self.bindings.len();
        self.bindings.push(Binding { name, span, references: Vec::new(), ty, init, statement, mutable });
        self.scopes.last_mut().unwrap().insert(name.to_string(), id);
    }

    fn reference(&mut self, name: &str, span: Span) {
        self.names.insert(name.to_string());
        if let Some(id) = self.scopes.iter().rev().find_map(|scope| scope.get(name)).copied() {
            self.bindings[id].references.push(span);
            self.uses.insert(span.start, id);
        }
    }

    fn block(&mut self, block: &'a Block) {
        self.scopes.push(BTreeMap::new());
        for statement in &block.stmts {
            self.statement(statement);
        }
        self.scopes.pop();
    }

    fn pattern(&mut self, pattern: &'a Pattern) {
        match pattern {
            Pattern::Bind { name, span } => self.bind(name, *span, None, None, None, false),
            Pattern::Some(p) | Pattern::Ok(p) | Pattern::Err(p) => self.pattern(p),
            Pattern::Case { args, .. } => {
                for p in args {
                    self.pattern(p);
                }
            }
            Pattern::None | Pattern::Wild => {}
        }
    }

    fn otherwise(&mut self, otherwise: &'a Option<Else>) {
        match otherwise {
            Some(Else::Block(block)) => self.block(block),
            Some(Else::If(statement)) => self.statement(statement),
            None => {}
        }
    }

    fn statement(&mut self, statement: &'a Stmt) {
        self.statements.push(statement);
        match &statement.kind {
            StmtKind::Let { mutable, name, name_span, ty, init } => {
                self.expression(init);
                self.bind(name, *name_span, ty.as_ref(), Some(init), Some(statement), *mutable);
            }
            StmtKind::LetTuple { mutable, names, init } => {
                self.expression(init);
                for (name, span) in names {
                    self.bind(name, *span, None, None, Some(statement), *mutable);
                }
            }
            StmtKind::Assign { target, value, .. } => {
                self.expression(target);
                self.expression(value);
            }
            StmtKind::Expr(e) => self.expression(e),
            StmtKind::If { cond, then, els } => {
                self.expression(cond);
                self.block(then);
                self.otherwise(els);
            }
            StmtKind::IfLet { name, name_span, value, then, els } => {
                self.expression(value);
                self.scopes.push(BTreeMap::new());
                self.bind(name, *name_span, None, None, None, false);
                self.block(then);
                self.scopes.pop();
                self.otherwise(els);
            }
            StmtKind::Match { value, arms } => {
                self.expression(value);
                for arm in arms {
                    self.scopes.push(BTreeMap::new());
                    self.pattern(&arm.pat);
                    self.block(&arm.body);
                    self.scopes.pop();
                }
            }
            StmtKind::While { cond, body } => {
                self.expression(cond);
                self.block(body);
            }
            StmtKind::Loop { body } | StmtKind::Scope { body } | StmtKind::Block(body) => self.block(body),
            StmtKind::For { var, var_span, start, end, body } => {
                self.expression(start);
                self.expression(end);
                self.scopes.push(BTreeMap::new());
                self.bind(var, *var_span, None, None, None, false);
                self.block(body);
                self.scopes.pop();
            }
            StmtKind::ForIn { var, var_span, var2, iter, body } => {
                self.expression(iter);
                self.scopes.push(BTreeMap::new());
                self.bind(var, *var_span, None, None, None, false);
                if let Some((name, span)) = var2 {
                    self.bind(name, *span, None, None, None, false);
                }
                self.block(body);
                self.scopes.pop();
            }
            StmtKind::With { name, name_span, init, body } => {
                self.expression(init);
                self.scopes.push(BTreeMap::new());
                self.bind(name, *name_span, None, None, None, false);
                self.block(body);
                self.scopes.pop();
            }
            StmtKind::Return(Some(e)) => self.expression(e),
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
        }
    }

    fn expression(&mut self, expr: &'a Expr) {
        self.expressions.push(expr);
        match &expr.kind {
            ExprKind::Ident(name) => self.reference(name, expr.span),
            ExprKind::Call { callee, args } => {
                self.expression(callee);
                for arg in args {
                    self.expression(arg);
                }
            }
            ExprKind::Field { base, .. } => self.expression(base),
            ExprKind::StructLit { name, fields, .. } => {
                self.names.insert(name.clone());
                for field in fields {
                    self.expression(&field.value);
                }
            }
            ExprKind::Generic { name, .. } => {
                self.names.insert(name.clone());
            }
            ExprKind::VecLit(values) | ExprKind::Tuple(values) => {
                for value in values {
                    self.expression(value);
                }
            }
            ExprKind::MapLit(values) => {
                for (key, value) in values {
                    self.expression(key);
                    self.expression(value);
                }
            }
            ExprKind::Index { base, index } => {
                self.expression(base);
                self.expression(index);
            }
            ExprKind::SliceRange { base, start, end } => {
                self.expression(base);
                self.expression(start);
                self.expression(end);
            }
            ExprKind::Borrow(e) | ExprKind::Paren(e) | ExprKind::Err(e)
            | ExprKind::Try(e) | ExprKind::Spawn(e) | ExprKind::Unary { expr: e, .. }
            | ExprKind::Named { value: e, .. } => self.expression(e),
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::Coalesce { lhs, rhs, .. } => {
                self.expression(lhs);
                self.expression(rhs);
            }
            ExprKind::Return(Some(e)) => self.expression(e),
            ExprKind::Closure { params, body } => {
                self.scopes.push(BTreeMap::new());
                for (name, span, ty) in params {
                    self.bind(name, *span, ty.as_ref(), None, None, false);
                }
                self.expression(body);
                self.scopes.pop();
            }
            ExprKind::Case { args: Some(args), .. } => {
                for arg in args {
                    self.expression(arg);
                }
            }
            ExprKind::Catch { value, name, name_span, handler, .. } => {
                self.expression(value);
                self.scopes.push(BTreeMap::new());
                self.bind(name, *name_span, None, None, None, false);
                self.expression(handler);
                self.scopes.pop();
            }
            ExprKind::Match { value, arms } => {
                self.expression(value);
                for arm in arms {
                    self.scopes.push(BTreeMap::new());
                    self.pattern(&arm.pat);
                    self.expression(&arm.body);
                    self.scopes.pop();
                }
            }
            ExprKind::Block(block) => self.block(block),
            ExprKind::Str(segments) => {
                for segment in segments {
                    if let StrSeg::Expr { expr, .. } = segment {
                        self.expression(expr);
                    }
                }
            }
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Byte(_) | ExprKind::Bool(_)
            | ExprKind::None | ExprKind::Return(None) | ExprKind::Break | ExprKind::Continue
            | ExprKind::Unit | ExprKind::Case { args: None, .. } => {}
        }
    }
}

fn edit(span: Span, replacement: impl Into<String>) -> Edit {
    Edit { start: span.start, end: span.end, replacement: replacement.into() }
}

fn fresh(stem: &str, names: &mut BTreeSet<String>) -> String {
    let mut name = stem.to_string();
    let mut suffix = 2;
    while names.contains(&name) {
        name = format!("{stem}{suffix}");
        suffix += 1;
    }
    names.insert(name.clone());
    name
}

fn proposal(
    code: &'static str,
    start: usize,
    id: &str,
    title: String,
    automatic: bool,
    impact: &str,
    preconditions: &[&str],
    edits: Vec<Edit>,
) -> Proposal {
    Proposal {
        code,
        diagnostic_start: start,
        fix: Fix {
            id: id.to_string(),
            title,
            applicability: if automatic { "automatic" } else { "candidate" },
            impact: impact.to_string(),
            preconditions: preconditions.iter().map(|s| s.to_string()).collect(),
            edits,
        },
    }
}

fn text(src: &str, span: Span) -> Option<&str> {
    src.get(span.start..span.end)
}

fn newline(src: &str) -> &str {
    if src.contains("\r\n") { "\r\n" } else { "\n" }
}

fn indentation(src: &str, pos: usize) -> &str {
    let start = src[..pos].rfind('\n').map_or(0, |i| i + 1);
    let line = &src[start..pos];
    &line[..line.bytes().take_while(|c| matches!(c, b' ' | b'\t')).count()]
}

fn is_vec(binding: &Binding<'_>) -> bool {
    if let Some(ty) = binding.ty {
        return ty.name == "Vec";
    }
    matches!(binding.init.map(|e| &e.kind), Some(ExprKind::VecLit(_)))
}

fn split_push(
    src: &str,
    function: &Function<'_>,
    statement: &Stmt,
    diagnostic_start: usize,
    names: &mut BTreeSet<String>,
) -> Option<Proposal> {
    let StmtKind::Expr(Expr { kind: ExprKind::Call { callee, args }, .. }) = &statement.kind else { return None };
    let ExprKind::Field { base, name } = &callee.kind else { return None };
    let ExprKind::Ident(receiver) = &base.kind else { return None };
    if name != "push" || args.len() != 1 {
        return None;
    }
    let arg = &args[0];
    if arg.span.start != diagnostic_start {
        return None;
    }
    let ExprKind::Call { callee: read, args: read_args } = &arg.kind else { return None };
    let ExprKind::Field { base: read_base, name: read_name } = &read.kind else { return None };
    if read_name != "len" || !read_args.is_empty() {
        return None;
    }
    let ExprKind::Ident(read_receiver) = &read_base.kind else { return None };
    if receiver != read_receiver {
        return None;
    }
    let id = *function.uses.get(&base.span.start)?;
    if !is_vec(&function.bindings[id]) || function.uses.get(&read_base.span.start) != Some(&id) {
        return None;
    }
    let name = fresh(&format!("{receiver}Len"), names);
    let inserted = format!(
        "let {name} = {}{}{}",
        text(src, arg.span)?,
        newline(src),
        indentation(src, statement.span.start),
    );
    Some(proposal(
        "E0403",
        diagnostic_start,
        "split-receiver-read",
        format!("Read {receiver}.len() before borrowing {receiver} for push"),
        true,
        "Reads the built-in vector length once in the same statement block; preserves control flow, side-effect order and resource lifetimes.",
        &["The receiver resolves to a local built-in Vec.", "The argument is exactly that vector's side-effect-free len() read.", "The call is a standalone expression statement."],
        vec![
            Edit { start: statement.span.start, end: statement.span.start, replacement: inserted },
            edit(arg.span, name),
        ],
    ))
}

/// Hoisting across a receiver call can change observable order. It is always a
/// reviewable candidate; complete-file rechecking still gates publication.
fn hoist_argument(src: &str, statement: &Stmt, diagnostic_start: usize, names: &mut BTreeSet<String>) -> Option<Proposal> {
    let StmtKind::Expr(Expr { kind: ExprKind::Call { callee, args }, .. }) = &statement.kind else { return None };
    let ExprKind::Field { .. } = &callee.kind else { return None };
    let arg = args.iter().find(|arg| arg.span.start == diagnostic_start)?;
    let name = fresh("argument", names);
    let inserted = format!("let {name} = {}{}{}", text(src, arg.span)?, newline(src), indentation(src, statement.span.start));
    Some(proposal(
        "E0403", diagnostic_start, "hoist-argument", "Evaluate the argument before acquiring the mutable receiver".into(), false,
        "Moves this argument's evaluation before the receiver and any preceding arguments; their side effects, panic order and resource lifetimes can change.",
        &["Choose only when evaluating this argument first matches the intended behavior.", "The call remains in its original statement block; the entire patched file must recheck."],
        vec![Edit { start: statement.span.start, end: statement.span.start, replacement: inserted }, edit(arg.span, name)],
    ))
}

fn import_proposals(src: &str, program: &Program, functions: &[Function<'_>], diags: &Diags, out: &mut Vec<Proposal>) {
    let mut lexical = Diags::default();
    let tokens = lexer::lex_file(src, &mut lexical);
    if lexical.has_errors() {
        return;
    }
    // W0003 only reflects bodies that were type checked. Uninstantiated generic
    // functions and generic-struct methods can still need an import later. Keep
    // any import with a syntactic reference anywhere in the source, even when
    // the spelling is ambiguous. Lexical names cover type annotations; the AST
    // names additionally cover expressions inside string interpolations.
    let mut syntax_names = BTreeSet::new();
    for token in &tokens {
        if let Tok::Ident(name) = &token.tok {
            if !program.uses.iter().any(|u| u.span.start == token.span.start) {
                syntax_names.insert(name.as_str());
            }
        }
    }
    for function in functions {
        syntax_names.extend(function.names.iter().map(String::as_str));
    }
    let referenced = |module: &str| {
        syntax_names.contains(module) || match module {
            "collections" => ["Heap", "Deque"].iter().any(|name| syntax_names.contains(*name)),
            "sync" => ["Rc", "Arc", "Mutex", "Guard", "Channel", "SendError", "Join", "Scope", "Send", "Sync", "scope", "spawn", "with"]
                .iter().any(|name| syntax_names.contains(*name)),
            _ => false,
        }
    };
    // A duplicate import has E0011, not a mechanical unused-import repair.
    // Deleting its first occurrence could expose a new W0003 only on the next
    // run, so leave that invalid declaration set unchanged.
    let unused: BTreeSet<usize> = diags.items.iter()
        .filter(|d| d.code == "W0003")
        .filter(|d| program.uses.iter().find(|u| u.span.start == d.span.start)
            .is_some_and(|u| !referenced(&u.module)
                && program.uses.iter().filter(|other| other.module == u.module).count() == 1))
        .map(|d| d.span.start).collect();
    let mut after_imports = 0;
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(&token.tok, Tok::Ident(name) if name == "use") {
            continue;
        }
        let end = tokens[index..].iter().position(|t| matches!(t.tok, Tok::Newline | Tok::Eof))
            .map(|n| index + n).unwrap_or(tokens.len());
        let group = &tokens[index..end];
        let modules: Vec<&Token> = group.iter().filter(|t| program.uses.iter().any(|u| u.span.start == t.span.start)).collect();
        if modules.is_empty() {
            continue;
        }
        // Add missing imports after existing import lines. This also keeps an
        // insertion disjoint from deleting an unused first-line import.
        after_imports = after_imports.max(tokens.get(end).map_or(group.last().unwrap().span.end, |t| t.span.end));
        if !modules.iter().any(|m| unused.contains(&m.span.start)) {
            continue;
        }
        let first_unused = modules.iter().find(|m| unused.contains(&m.span.start)).unwrap().span.start;
        let mut edits = Vec::new();
        if modules.iter().all(|m| unused.contains(&m.span.start)) {
            edits.push(Edit { start: token.span.start, end: group.last().unwrap().span.end, replacement: String::new() });
        } else {
            let mut i = 0;
            while i < modules.len() {
                if !unused.contains(&modules[i].span.start) {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < modules.len() && unused.contains(&modules[i].span.start) {
                    i += 1;
                }
                let (from, to) = if i < modules.len() {
                    (modules[start].span.start, modules[i].span.start)
                } else {
                    (modules[start - 1].span.end, modules[i - 1].span.end)
                };
                edits.push(Edit { start: from, end: to, replacement: String::new() });
            }
        }
        out.push(proposal(
            "W0003", first_unused, "remove-unused-import",
            "Remove unused standard-library imports".into(), true,
            "Removes only imports reported unused; preserves the remaining grouped imports and trailing comments.",
            &["The import has W0003 and no runtime initialization semantics.", "No syntactic reference remains, including uninstantiated generic bodies and types."], edits,
        ));
    }

    let mut missing = BTreeSet::new();
    let mut first = None;
    for d in &diags.items {
        if d.code != "E0010" {
            continue;
        }
        for module in ["io", "fs", "os", "math", "time", "rand", "sync", "collections"] {
            let module_message = format!("module {mark}{module}{mark} is not imported", mark = '\u{60}');
            let sync_message = format!("comes from {mark}std/sync{mark}, which is not imported", mark = '\u{60}');
            if (d.message == module_message || (module == "sync" && d.message.contains(&sync_message)))
                && !program.uses.iter().any(|u| u.module == module)
            {
                missing.insert(module);
                first = Some(first.map_or(d.span.start, |p: usize| p.min(d.span.start)));
            }
        }
    }
    if let Some(start) = first {
        let mut inserted = String::new();
        for module in missing {
            inserted.push_str(&format!("use std/{module}{}", newline(src)));
        }
        out.push(proposal(
            "E0010", start, "add-import", "Import explicitly named standard-library modules".into(), true,
            "Adds only the standard-library module already identified by the unresolved qualified reference.",
            &["The diagnostic identifies an unimported built-in module; no name guessing is performed."],
            vec![Edit { start: after_imports, end: after_imports, replacement: inserted }],
        ));
    }
}

fn move_candidates(src: &str, functions: &[Function<'_>], diagnostic_start: usize, out: &mut Vec<Proposal>) {
    // A user function can shadow the prelude take operation. Passing the type
    // checker does not prove that such a function replaces the source field.
    let builtin_take = !functions.iter().any(|f| f.declaration.name == "take");
    for function in functions {
        for expr in &function.expressions {
            let ExprKind::Call { callee, args } = &expr.kind else { continue };
            let ExprKind::Ident(callee_name) = &callee.kind else { continue };
            let Some(target) = functions.iter().find(|f| f.declaration.name == *callee_name && f.declaration.owner.is_none()) else { continue };
            if !target.declaration.type_params.is_empty() || function.uses.contains_key(&callee.span.start) {
                continue;
            }
            for (index, arg) in args.iter().enumerate() {
                if arg.span.start != diagnostic_start {
                    continue;
                }
                let ExprKind::Field { base, .. } = &arg.kind else { continue };
                let ExprKind::Ident(_) = &base.kind else { continue };
                let Some(binding) = function.uses.get(&base.span.start).map(|id| &function.bindings[*id]) else { continue };
                let Some(statement) = binding.statement else { continue };
                let StmtKind::Let { .. } = statement.kind else { continue };
                let Some(parameter) = target.declaration.params.get(index) else { continue };
                if !parameter.sink {
                    continue;
                }
                let Some(value) = text(src, arg.span) else { continue };
                if builtin_take && !function.names.contains("take") {
                    let mut take_edits = vec![edit(arg.span, format!("take(&{value})"))];
                    if !binding.mutable {
                        if src.get(statement.span.start..statement.span.start + 3) != Some("let") {
                            continue;
                        }
                        take_edits.push(Edit { start: statement.span.start, end: statement.span.start + 3, replacement: "var".into() });
                    }
                    out.push(proposal(
                        "E0401", diagnostic_start, "take", "Move out by replacing the field with its default value".into(), false,
                        "Consumes the field value and leaves its default value behind; makes the owning binding writable if needed.",
                        &["Choose this only if replacing the original field is intended.", "The field type must support take and the owner must have exclusive access.", "The prelude take operation must not be shadowed."],
                        take_edits,
                    ));
                }
                out.push(proposal(
                    "E0401", diagnostic_start, "clone", "Call the field's clone implementation".into(), false,
                    "Calls the field's clone method; allocation, sharing and side effects follow that implementation.",
                    &["Choose this only if the clone implementation matches the intended ownership.", "The field must provide a valid clone operation."],
                    vec![edit(arg.span, format!("{value}.clone()"))],
                ));

                let Some(prefix) = src.get(parameter.span.end..parameter.ty.span.start) else { continue };
                let mut lexical = Diags::default();
                let prefix_tokens = lexer::lex_file(prefix, &mut lexical);
                let Some(sink) = prefix_tokens.iter().find(|t| matches!(&t.tok, Tok::Ident(n) if n == "sink")) else { continue };
                let mut borrow_edits = vec![Edit {
                    start: parameter.span.end + sink.span.start,
                    end: parameter.ty.span.start,
                    replacement: "&".into(),
                }];
                for caller in functions {
                    for call in &caller.expressions {
                        let ExprKind::Call { callee, args } = &call.kind else { continue };
                        if !matches!(&callee.kind, ExprKind::Ident(n) if n == callee_name)
                            || caller.uses.contains_key(&callee.span.start)
                        {
                            continue;
                        }
                        let Some(actual) = args.get(index) else { continue };
                        let Some(actual_text) = text(src, actual.span) else { continue };
                        borrow_edits.push(edit(actual.span, format!("&{actual_text}")));
                    }
                }
                out.push(proposal(
                    "E0401", diagnostic_start, "borrow", format!("Make {callee_name} borrow this parameter and update its calls"), false,
                    "Changes the function contract from consuming an owned value to borrowing a read-only reference; callers keep ownership.",
                    &["Choose this only if the callee should not consume the value.", "Every known call and the changed body must recheck; external callers may require the same change."],
                    borrow_edits,
                ));
            }
        }
    }
}

fn source_candidates(functions: &[Function<'_>], diagnostic_start: usize, out: &mut Vec<Proposal>) {
    for function in functions {
        let declaration = function.declaration;
        let Some(ret) = &declaration.ret else { continue };
        if ret.span.start != diagnostic_start || declaration.ret_of.is_some() {
            continue;
        }
        let names: Vec<&str> = declaration.params.iter().filter(|p| !p.sink && p.name != "_").map(|p| p.name.as_str()).collect();
        let mut sources: Vec<String> = names.iter().map(|n| n.to_string()).collect();
        if names.len() > 1 {
            sources.push(format!("({})", names.join(", ")));
        }
        sources.push("static".into());
        for source in sources {
            out.push(proposal(
                "E0302", diagnostic_start, "add-of", format!("Declare returned borrow source: {source}"), false,
                "Makes the function's borrowing contract explicit; the caller must preserve every declared source for the returned view's lifetime.",
                &["Choose the source contract that matches the intended API.", "The proposed signature and all returns must independently recheck."],
                vec![Edit { start: ret.span.end, end: ret.span.end, replacement: format!(" of {source}") }],
            ));
        }
    }
}

pub fn suggest(src: &str, program: &Program, diags: &Diags) -> Vec<Proposal> {
    let functions: Vec<Function<'_>> = program.fns.iter().map(Function::new).collect();
    let mut reserved: Vec<BTreeSet<String>> = functions.iter().map(|f| f.names.clone()).collect();
    let mut diagnostics: Vec<_> = diags.items.iter().collect();
    diagnostics.sort_by_key(|d| (d.span.start, d.code));
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for diagnostic in diagnostics {
        if !seen.insert((diagnostic.code, diagnostic.span.start)) {
            continue;
        }
        match diagnostic.code {
            "W0001" => {
                for (index, function) in functions.iter().enumerate() {
                    let Some(binding) = function.bindings.iter().find(|b| b.span.start == diagnostic.span.start) else { continue };
                    if binding.name.starts_with('_') {
                        continue;
                    }
                    let name = fresh(&format!("_{}", binding.name), &mut reserved[index]);
                    let mut spans = binding.references.clone();
                    spans.push(binding.span);
                    spans.sort_by_key(|span| span.start);
                    spans.dedup_by_key(|span| span.start);
                    let edits = spans.into_iter().map(|span| edit(span, &name)).collect();
                    out.push(proposal(
                        "W0001", diagnostic.span.start, "rename-unused", format!("Rename {} to {name}", binding.name), true,
                        "Keeps the same binding, writes and destruction time; only suppresses the unused-binding warning.",
                        &["All edited identifiers resolve to this binding.", "The new name does not capture any existing name in the function."],
                        edits,
                    ));
                }
            }
            "E0403" => {
                for (index, function) in functions.iter().enumerate() {
                    for statement in &function.statements {
                        if let Some(p) = split_push(src, function, statement, diagnostic.span.start, &mut reserved[index]) {
                            out.push(p);
                        } else if let Some(p) = hoist_argument(src, statement, diagnostic.span.start, &mut reserved[index]) {
                            out.push(p);
                        }
                    }
                }
            }
            "E0401" => move_candidates(src, &functions, diagnostic.span.start, &mut out),
            "E0302" => source_candidates(&functions, diagnostic.span.start, &mut out),
            _ => {}
        }
    }
    import_proposals(src, program, &functions, diags, &mut out);
    out.sort_by_key(|p| (p.diagnostic_start, p.fix.id.clone()));
    out
}
