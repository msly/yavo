//! Read-only selfhost capability inventory over the complete stage0 AST and typed IR.
//!
//! This is a planning profile, not a second checker. It walks past unsupported
//! constructs without editing the source or continuing from a poisoned parser.
//! Flow-sensitive selfhost borrow restrictions remain explicitly unknown; a
//! feature-set forecast is never evidence that a proposed implementation works.
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use serde_json::{json, Value};

use crate::ast::*;
use crate::check::*;
use crate::diag::{Diags, Span};
use crate::project::Project;
use crate::types::{self, IntTy, Ty};

pub fn run(input: &Path, profile: &str) -> i32 {
    let result = (|| -> Result<Value, String> {
        let source = std::fs::read_to_string(input).map_err(|e| e.to_string())?;
        let mut diags = Diags::default();
        let project = Project::load(input, &source, &mut diags).map_err(|e| e.to_string())?;
        let typed = if diags.has_errors() { None } else { crate::check::check(&project.program, &mut diags) };
        let Some(typed) = typed else {
            return Ok(json!({"schema":"yavo-subset-features/1", "stage0_accepted":false,
                "ast_complete":false, "semantic_complete":false,
                "unknown":["stage0_rejected"], "diagnostics":project.json(&diags)}));
        };
        let mut a = Analysis::new(&project, profile);
        a.program(&project.program, &typed);
        Ok(json!({"schema":"yavo-subset-features/1", "profile":format!("selfhost-{profile}-structural-v1"),
            "stage0_accepted":true, "ast_complete":true, "typed_ir_complete":true,
            "semantic_complete":false,
            "unknown":["flow_sensitive_selfhost_restrictions", "unmodeled_structural_capabilities", "implementation_limits"],
            "project_sha256":project.hash(), "sources":project.manifest(),
            "blockers":a.blockers, "ast_nodes":a.ast_nodes, "typed_nodes":a.typed_nodes,
            "scope":"All AST declarations/bodies and instantiated typed IR in the loaded module graph are visited. This is an incomplete capability model, not an exhaustive U0001 checker. Unmodeled structure, ownership/liveness guards and implementation limits remain unknown."}))
    })();
    match result {
        Ok(value) => {
            let status = if value["stage0_accepted"] == true { 0 } else { 1 };
            println!("{value}");
            status
        }
        Err(message) => {
            println!("{}", json!({"schema":"yavo-subset-features/1", "stage0_accepted":false,
                "ast_complete":false, "semantic_complete":false, "unknown":["load_failed"],
                "error":message}));
            2
        }
    }
}

struct Analysis<'a> {
    project: &'a Project,
    blockers: BTreeMap<String, Vec<Value>>,
    seen: BTreeSet<(String, usize, usize, String)>,
    seen_types: HashSet<(Ty, usize)>,
    ast_nodes: usize,
    typed_nodes: usize,
    legacy_radix: bool,
    general_patterns: bool,
}

fn primitive(t: Ty) -> bool { matches!(t, Ty::Int(IntTy::I64 | IntTy::U8) | Ty::Bool) }
fn standard_error(t: Ty) -> bool {
    matches!(t, Ty::Enum(_)) && matches!(t.to_string().as_str(),
        "fs.Error" | "io.Error" | "os.Error" | "process.Error" | "Utf8Error")
}
fn standard_error_type(t: Ty) -> bool { standard_error(t) || t.to_string()=="Utf8Error" }
fn flat_tuple(t: Ty) -> bool {
    if let Ty::Tuple(id) = t {
        types::tuple_elems(id).iter().all(|e| primitive(*e) || *e == Ty::String
            || (matches!(e, Ty::Struct(_) | Ty::Enum(_)) && e.is_copy() && !standard_error_type(*e)))
    } else { false }
}
fn heap_pair(t: Ty) -> bool {
    matches!(t, Ty::Tuple(id) if types::tuple_elems(id) == [Ty::String, Ty::Int(IntTy::I64)])
}
fn pure_heap_expression(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Int(_)|ExprKind::Byte(_)|ExprKind::Bool(_)|ExprKind::Ident(_)=>true,
        ExprKind::Str(parts)=>parts.iter().all(|p|matches!(p,StrSeg::Text(_))),
        ExprKind::Paren(x)|ExprKind::Unary{expr:x,..}|ExprKind::Field{base:x,..}=>pure_heap_expression(x),
        ExprKind::Binary{lhs,rhs,..}=>pure_heap_expression(lhs)&&pure_heap_expression(rhs),
        _=>false,
    }
}

impl<'a> Analysis<'a> {
    fn new(project: &'a Project, profile: &str) -> Self {
        Self { project, blockers:BTreeMap::new(), seen:BTreeSet::new(), seen_types:HashSet::new(),
            ast_nodes:0, typed_nodes:0, legacy_radix:profile=="batch51", general_patterns:profile=="batch53" }
    }
    fn add(&mut self, feature: &str, span: Span, detail: impl Into<String>) {
        let detail = detail.into();
        if !self.seen.insert((feature.into(), span.start, span.end, detail.clone())) { return; }
        let file = &self.project.program.files[crate::project::file_id(&self.project.program.files, span)];
        self.blockers.entry(feature.into()).or_default().push(json!({
            "file":file.path, "line":span.line.saturating_sub(file.line)+1,
            "column":span.col, "start":span.start.saturating_sub(file.start),
            "end":span.end.saturating_sub(file.start), "detail":detail}));
    }
    fn ty(&mut self, t: Ty, span: Span) {
        if !self.seen_types.insert((t, span.start)) { return; }
        let text = t.to_string();
        match t {
            Ty::Float(_) => self.add("float", span, text),
            Ty::Int(i) if !matches!(i, IntTy::I64 | IntTy::U8) => self.add("numeric_types", span, text),
            Ty::Vec(id) => {
                let e = types::vec_elem(id);
                if standard_error_type(e) {self.add("vec_standard_error",span,text.clone());}
                match e {
                    Ty::Tuple(_) => self.add(if e.is_copy() {"vec_copy_tuple"} else {"vec_owned_tuple"}, span, text),
                    Ty::Opt(_) => self.add("vec_optional", span, text),
                    Ty::Res(_) => self.add("vec_result", span, text),
                    Ty::Vec(inner) if types::vec_elem(inner) != Ty::Int(IntTy::I64) => self.add("vec_nested", span, text),
                    Ty::Map(_) | Ty::Set(_) | Ty::Heap(_) | Ty::Deque(_) | Ty::Arena(_) | Ty::Box(_) | Ty::Dyn(_) | Ty::Sync(_) | Ty::Slice(_) | Ty::Ref(_) | Ty::RefMut(_) => self.add("vec_general", span, text),
                    _ => {}
                }
                self.ty(e, span);
            }
            Ty::Map(id) => {
                let (k,v) = types::map_parts(id);
                if k != Ty::String { self.add("map_keys", span, k.to_string()); }
                if !primitive(v) { self.add("map_values", span, v.to_string()); }
                self.ty(k,span); self.ty(v,span);
            }
            Ty::Set(id) => {
                let (k,_) = types::map_parts(id);
                if k != Ty::String { self.add("set_general",span,k.to_string()); }
                self.ty(k,span);
            }
            Ty::Heap(id) => {
                let e=types::vec_elem(id);
                if !heap_pair(e) { self.add("heap_general",span,text); }
                self.ty(e,span);
            }
            Ty::Deque(id) => { self.add("deque",span,text); self.ty(types::vec_elem(id),span); }
            Ty::Arena(id) | Ty::Id(id) => { self.add("arena",span,text); self.ty(types::vec_elem(id),span); }
            Ty::Opt(id) => {
                let e=types::opt_inner(id);
                if standard_error_type(e) {self.add("optional_standard_error",span,text.clone());}
                let pattern_owned = self.general_patterns && matches!(e,Ty::String | Ty::Struct(_) | Ty::Enum(_));
                if !(primitive(e) || matches!(e,Ty::Struct(_) | Ty::Enum(_)) && e.is_copy() || flat_tuple(e) || pattern_owned) {
                    self.add(if e.needs_drop() {"optional_owned"} else {"optional_general"},span,text);
                }
                self.ty(e,span);
            }
            Ty::Res(id) => {
                let (o,e)=types::res_parts(id);
                if matches!(o,Ty::Opt(_) | Ty::Res(_) | Ty::Tuple(_) | Ty::Heap(_))
                    || matches!(e,Ty::Opt(_) | Ty::Res(_) | Ty::Tuple(_) | Ty::Heap(_)) {
                    self.add("result_composition",span,text);
                }
                if o.carries_borrow() || e.carries_borrow() { self.add("result_borrow",span,"Result with borrowed payload"); }
                self.ty(o,span); self.ty(e,span);
            }
            Ty::Tuple(id) => {
                if !flat_tuple(t) { self.add("tuple_general",span,text); }
                for e in types::tuple_elems(id) { self.ty(e,span); }
            }
            Ty::Slice(id) => {
                let e=types::slice_elem(id);
                if standard_error_type(e) || !(primitive(e) || matches!(e,Ty::String | Ty::Str | Ty::Struct(_) | Ty::Enum(_))) {
                    self.add("slice_general",span,text);
                }
                self.ty(e,span);
            }
            Ty::Box(id) => { self.add("box",span,text); self.ty(types::box_inner(id),span); }
            Ty::Dyn(_) => self.add("interfaces",span,text),
            Ty::Sync(id) => { self.add("concurrency",span,text); self.ty(types::sync_parts(id).1,span); }
            // Implicit references in typed IR are normal iteration/argument lowering.
            // Explicit reference syntax is recorded by the AST walker.
            Ty::Ref(id) => self.ty(types::ref_inner(id),span),
            Ty::RefMut(id) => self.ty(types::ref_mut_inner(id),span),
            Ty::Error => self.add("unknown_typed_error",span,"stage0 produced Error type"),
            _ => {}
        }
    }
    fn syn_type(&mut self,t:&TypeExpr) {
        self.ast_nodes+=1;
        // Uninstantiated generic bodies are absent from typed IR. Preserve
        // their independently known syntax requirements as well.
        if t.name=="Vec" && t.args.len()==1 && t.args[0].name=="(,)" {
            let parts=&t.args[0].args;
            let kind=if parts.iter().all(|p|matches!(p.name.as_str(),"int"|"byte"|"bool")) {
                "vec_copy_tuple"
            } else if parts.iter().any(|p|p.name=="String") {
                "vec_owned_tuple"
            } else { "vec_tuple_unresolved" };
            // Typed occurrences can refine named tuple elements below.
            if kind!="vec_tuple_unresolved" {self.add(kind,t.span,"Vec tuple annotation");}
        }
        if matches!(t.name.as_str(),"&" | "&inout") { self.add("references",t.span,t.name.clone()); }
        if matches!(t.name.as_str(),"i8"|"i16"|"i32"|"i64"|"u8"|"u16"|"u32"|"u64") {
            self.add("numeric_types",t.span,t.name.clone());
        }
        for a in &t.args { self.syn_type(a); }
    }
    fn program(&mut self,p:&Program,h:&HProgram) {
        for u in &p.uses {
            let file=crate::project::file_id(&p.files,u.span);
            let before=p.fns.iter().map(|f|f.name_span).chain(p.structs.iter().map(|s|s.name_span))
                .chain(p.enums.iter().map(|e|e.name_span)).chain(p.interfaces.iter().map(|i|i.name_span))
                .any(|s|s.start<u.span.start && crate::project::file_id(&p.files,s)==file);
            if before {self.add("imports_after_items",u.span,u.path.clone());}
            if u.path.starts_with("std/") && !matches!(u.module.as_str(),"fs"|"io"|"os"|"process"|"collections") {
                self.add("std_modules",u.span,u.path.clone());
            }
        }
        for s in &p.structs {
            if !s.type_params.is_empty() { self.add("generics",s.name_span,s.name.clone()); }
            if !s.impls.is_empty() { self.add("interfaces",s.name_span,s.name.clone()); }
            for field in &s.fields {
                self.syn_type(&field.ty);
                if let Some(default)=&field.default { self.expr(default,false); }
            }
            for f in &s.methods { self.function(f); }
        }
        for e in &p.enums {
            if !self.general_patterns && e.variants.iter().any(|v| !v.fields.is_empty()) { self.add("enum_payload",e.name_span,e.name.clone()); }
            if !e.impls.is_empty() { self.add("interfaces",e.name_span,e.name.clone()); }
            for v in &e.variants { for f in &v.fields {
                self.syn_type(&f.ty); if let Some(x)=&f.default { self.expr(x,false); }
            }}
            for f in &e.methods { self.function(f); }
        }
        for i in &p.interfaces {
            self.add("interfaces",i.name_span,i.name.clone());
            for f in &i.methods { self.function(f); }
        }
        for f in &p.fns { self.function(f); }
        for s in &h.structs {
            let span=p.structs.iter().find(|x| x.name==s.ty.to_string()).map(|x| x.name_span)
                .or_else(||p.enums.iter().find(|x| x.name==s.ty.to_string()).map(|x|x.name_span))
                .unwrap_or_default();
            for (name,t) in &s.fields {
                if matches!(t,Ty::Opt(_) | Ty::Tuple(_) | Ty::Heap(_) | Ty::Res(_) | Ty::Unit) {
                    self.add("aggregate_composition",span,format!("{}.{name}: {t}",s.ty));
                }
                self.ty(*t,span);
            }
            if !self.general_patterns && !standard_error(s.ty) && s.variants.iter().any(|(_,fs)|!fs.is_empty()) {
                self.add("enum_payload",span,s.ty.to_string());
            }
            for (_,fs) in &s.variants { for (_,t) in fs {
                if self.general_patterns && matches!(t,Ty::Opt(_) | Ty::Tuple(_) | Ty::Res(_) | Ty::Heap(_)) {
                    self.add("enum_payload_composition",span,format!("{} payload: {t}",s.ty));
                }
                self.ty(*t,span);
            }}
        }
        for f in &h.fns {
            let span=p.fns.iter().find(|x| x.name==f.name).map(|x|x.name_span).unwrap_or_default();
            self.ty(f.ret,span);
            for &param in &f.params {
                self.ty(f.locals[param].ty,span);
                if f.locals[param].ty==Ty::Unit { self.add("unit_parameters",span,f.name.clone()); }
            }
            self.hbody(&f.body,f);
        }
    }
    fn function(&mut self,f:&FnDecl) {
        self.ast_nodes+=1;
        if f.owner.is_some() { self.add("methods",f.name_span,f.name.clone()); }
        if !f.type_params.is_empty() { self.add("generics",f.name_span,f.name.clone()); }
        for p in &f.params { self.syn_type(&p.ty); }
        if let Some(t)=&f.ret { self.syn_type(t); }
        self.block(&f.body);
    }
    fn block(&mut self,b:&Block) { for s in &b.stmts { self.stmt(s); } }
    fn els(&mut self,e:&Option<Else>) {
        if let Some(e)=e { match e { Else::Block(b)=>self.block(b), Else::If(s)=>self.stmt(s) }}
    }
    fn stmt(&mut self,s:&Stmt) {
        self.ast_nodes+=1;
        match &s.kind {
            StmtKind::Let{ty,init,..}=> { if let Some(t)=ty { self.syn_type(t); } self.expr(init,false); }
            StmtKind::LetTuple{init,..}=> { self.add("tuple_destructure",s.span,"let tuple"); self.expr(init,false); }
            StmtKind::Assign{target,value,..}=> { self.expr(target,false); self.expr(value,false); }
            StmtKind::Expr(x)=>self.expr(x,false),
            StmtKind::If{cond,then,els}=> {self.expr(cond,false);self.block(then);self.els(els);}
            StmtKind::IfLet{value,then,els,..}=>{self.expr(value,false);self.block(then);self.els(els);}
            StmtKind::Match{value,arms}=>{self.expr(value,false);for a in arms {self.pattern(&a.pat,a.span,0);self.block(&a.body);}}
            StmtKind::While{cond,body}=>{self.expr(cond,false);self.block(body);}
            StmtKind::Loop{body}=>self.block(body),
            StmtKind::For{start,end,body,..}=>{self.expr(start,false);self.expr(end,false);self.block(body);}
            StmtKind::ForIn{iter,body,..}=>{
                if matches!(&iter.kind,ExprKind::Borrow(_)){self.add("mutable_iteration",s.span,"for in &collection");}
                self.expr(iter,false);self.block(body);
            }
            StmtKind::Return(x)=>{if let Some(x)=x {self.expr(x,false);}}
            StmtKind::Block(b)=>{self.add("block_statements",s.span,"nested block statement");self.block(b);}
            StmtKind::Scope{body}=>{self.add("concurrency",s.span,"scope");self.block(body);}
            StmtKind::With{init,body,..}=>{self.add("concurrency",s.span,"with");self.expr(init,false);self.block(body);}
            StmtKind::Break|StmtKind::Continue=>{}
        }
    }
    fn pattern(&mut self,p:&Pattern,span:Span,depth:usize) {
        self.ast_nodes+=1;
        match p {
            Pattern::Some(x)|Pattern::Ok(x)|Pattern::Err(x)=> {
                if !self.general_patterns { self.add("patterns_optional_result",span,"some/ok/err pattern"); }
                if !self.general_patterns && depth>0 { self.add("patterns_nested",span,"nested pattern"); }
                self.pattern(x,span,depth+1);
            }
            Pattern::None=>if !self.general_patterns { self.add("patterns_optional_result",span,"none pattern"); },
            Pattern::Case{args,..}=>{
                if !self.general_patterns && depth>0 {self.add("patterns_nested",span,"nested enum pattern");}
                for p in args {self.pattern(p,span,depth+1);}
            }
            Pattern::Bind{..}|Pattern::Wild=>{}
        }
    }
    fn expr(&mut self,e:&Expr,heap_closure:bool) {
        self.ast_nodes+=1;
        match &e.kind {
            ExprKind::Float(_)=>self.add("float",e.span,"floating-point literal"),
            ExprKind::Int(_)=>{
                let raw=self.project.source.get(e.span.start..e.span.end).unwrap_or("");
                if self.legacy_radix && (raw.starts_with("0x")||raw.starts_with("0X")||raw.starts_with("0b")||raw.starts_with("0B")) {
                    self.add("radix_literals",e.span,raw.to_string());
                }
            }
            ExprKind::Byte(_)|ExprKind::Bool(_)|ExprKind::Ident(_)|ExprKind::None|ExprKind::Unit|ExprKind::Break|ExprKind::Continue=>{}
            ExprKind::Str(parts)=>for p in parts {if let StrSeg::Expr{expr,spec}=p {
                if spec.is_some() {self.add("format_specifiers",expr.span,"format specifier");}
                self.expr(expr,false);
            }},
            ExprKind::Call{callee,args}=>{
                if let ExprKind::Ident(name)=&callee.kind {
                    if name=="assert" && args.len()==2 {self.add("assert_message",e.span,name.clone());}
                    if matches!(name.as_str(),"println"|"eprintln") && args.is_empty() {self.add("empty_output",e.span,name.clone());}
                    if name=="bool" {self.add("numeric_builtins",e.span,name.clone());}
                    if matches!(name.as_str(),"i8"|"i16"|"i32"|"i64"|"u8"|"u16"|"u32"|"u64"|"f32"|"f64") {
                        self.add("numeric_conversions",e.span,name.clone());
                    }
                }
                let is_heap=matches!(&callee.kind,ExprKind::Field{name,base} if name=="by" && matches!(&base.kind,ExprKind::Generic{name,..} if name=="Heap"));
                self.expr(callee,false);
                for a in args {self.expr(a,is_heap);}
            }
            ExprKind::Field{base,name}=>{
                if matches!(&base.kind,ExprKind::Ident(n) if matches!(n.as_str(),"int"|"byte"|"i8"|"i16"|"i32"|"i64"|"u8"|"u16"|"u32"|"u64"|"f32"|"f64")) {
                    self.add("numeric_constants",e.span,name.clone());
                }
                self.expr(base,false);
            }
            ExprKind::StructLit{name,targs,fields,..}=>{
                if !targs.is_empty() && !matches!(name.as_str(),"Set"|"Vec") {self.add("generic_construction",e.span,"generic struct construction");}
                for t in targs {self.syn_type(t);}
                for f in fields {self.expr(&f.value,false);}
            }
            ExprKind::Generic{name,targs}=>{
                if name!="Heap" {self.add("generic_expressions",e.span,name.clone());}
                for t in targs {self.syn_type(t);}
            }
            ExprKind::VecLit(items)|ExprKind::Tuple(items)=>for x in items {self.expr(x,false);},
            ExprKind::MapLit(items)=>{
                if !items.is_empty(){self.add("map_literals",e.span,"nonempty map literal");}
                for (k,v) in items {self.expr(k,false);self.expr(v,false);}
            }
            ExprKind::Index{base,index}=>{self.expr(base,false);self.expr(index,false);}
            ExprKind::SliceRange{base,start,end}=>{self.expr(base,false);self.expr(start,false);self.expr(end,false);}
            ExprKind::Borrow(x)|ExprKind::Paren(x)|ExprKind::Err(x)|ExprKind::Try(x)=>self.expr(x,false),
            ExprKind::Unary{op,expr,..}=>{
                if matches!(op,UnOp::Deref){self.add("references",e.span,"dereference");}
                self.expr(expr,false);
            }
            ExprKind::Binary{lhs,rhs,..}|ExprKind::Coalesce{lhs,rhs,..}=>{self.expr(lhs,false);self.expr(rhs,false);}
            ExprKind::Return(x)=>{if let Some(x)=x {self.expr(x,false);}},
            ExprKind::Closure{params,body}=>{
                if !heap_closure {self.add("closures",e.span,"non-Heap closure");}
                if heap_closure && !pure_heap_expression(body) {self.add("heap_comparator_expression",e.span,"Heap comparator requires a pure scalar expression without calls");}
                if params.iter().any(|(_,_,t)|t.is_some()){self.add("closure_typed",e.span,"typed closure");}
                if matches!(&body.kind,ExprKind::Block(_)){self.add("closure_block",e.span,"block closure");}
                for (_,_,t) in params {if let Some(t)=t{self.syn_type(t);}}
                self.expr(body,false);
            }
            ExprKind::Case{args,..}=>if let Some(xs)=args{for x in xs{self.expr(x,false);}},
            ExprKind::Named{value,..}=>self.expr(value,false),
            ExprKind::Spawn(x)=>{self.add("concurrency",e.span,"spawn");self.expr(x,false);}
            ExprKind::Catch{value,handler,block,..}=>{
                if !block {self.add("catch_expression",e.span,"expression catch");}
                self.expr(value,false);self.expr(handler,false);
            }
            ExprKind::Match{value,arms}=>{
                self.expr(value,false);
                for a in arms{self.pattern(&a.pat,a.span,0);self.expr(&a.body,false);}
            }
            ExprKind::Block(b)=>self.block(b),
        }
    }
    fn hbody(&mut self,b:&[HStmt],f:&HFn) {for s in b{self.hstmt(s,f);}}
    fn parts(&mut self,ps:&[PrintPart],f:&HFn) {for p in ps{match p {
        PrintPart::Text(_)=>{},
        PrintPart::Value(x)|PrintPart::Fixed(x,_)=>{
            if matches!(x.ty,Ty::Opt(_)){self.add("optional_print",x.span,x.ty.to_string());}
            self.hexpr(x,f);
        }
    }}}
    fn arms(&mut self,scrut:&HExpr,arms:&[HArm],f:&HFn) {
        if !self.general_patterns && !standard_error(scrut.ty) {
            self.add(if matches!(scrut.ty,Ty::Opt(_) | Ty::Res(_)) {"patterns_optional_result"} else {"match_user_enum"},
                scrut.span,scrut.ty.to_string());
        }
        self.hexpr(scrut,f);
        for a in arms {self.hbody(&a.body,f);if let Some(x)=&a.value{self.hexpr(x,f);}}
    }
    fn hstmt(&mut self,s:&HStmt,f:&HFn) {
        self.typed_nodes+=1;
        match &s.kind {
            HStmtKind::Let{local,init}=>{
                self.ty(f.locals[*local].ty,s.span);
                if f.locals[*local].ty==Ty::Unit{self.add("unit_bindings",s.span,f.locals[*local].name.clone());}
                self.hexpr(init,f);
            }
            HStmtKind::LetTuple{init,..}|HStmtKind::Discard(init)|HStmtKind::Expr(init)=>self.hexpr(init,f),
            HStmtKind::Assign{local,value,..}=>{self.ty(f.locals[*local].ty,s.span);self.hexpr(value,f);}
            HStmtKind::If{cond,then,els}=>{self.hexpr(cond,f);self.hbody(then,f);self.hbody(els,f);}
            HStmtKind::While{cond,body}=>{self.hexpr(cond,f);self.hbody(body,f);}
            HStmtKind::Loop{body}|HStmtKind::Block(body)=>self.hbody(body,f),
            HStmtKind::For{start,end,body,..}=>{self.hexpr(start,f);self.hexpr(end,f);self.hbody(body,f);}
            HStmtKind::ForEach{vec,body,..}=>{
                self.ty(f.locals[*vec].ty,s.span); self.hbody(body,f);
                if matches!(f.locals[*vec].ty,Ty::Set(_)){self.add("set_iteration",s.span,"Set iteration");}
            }
            HStmtKind::ForMap{map,body,..}=>{
                if matches!(f.locals[*map].ty,Ty::Set(_)){self.add("set_iteration",s.span,"Set iteration");}
                self.ty(f.locals[*map].ty,s.span);self.hbody(body,f);
            }
            HStmtKind::IfLet{scrut,then,els,..}=>{self.hexpr(scrut,f);self.hbody(then,f);self.hbody(els,f);}
            HStmtKind::Return(x)=>if let Some(x)=x{self.hexpr(x,f);},
            HStmtKind::Print{parts,..}=>self.parts(parts,f),
            HStmtKind::Match{scrut,arms,..}=>self.arms(scrut,arms,f),
            HStmtKind::Break|HStmtKind::Continue=>{}
        }
    }
    fn hexpr(&mut self,e:&HExpr,f:&HFn) {
        self.typed_nodes+=1;
        self.ty(e.ty,e.span);
        match &e.kind {
            HExprKind::Int(_)|HExprKind::Float(_)|HExprKind::Bool(_)|HExprKind::Str(_)|HExprKind::Local(_)|
            HExprKind::Move(_)|HExprKind::DequeNew|HExprKind::ArenaNew|HExprKind::ScopeNew|
            HExprKind::None|HExprKind::Break|HExprKind::Continue|HExprKind::OsArgs|HExprKind::EmptyString|
            HExprKind::InoutPlace(_)|HExprKind::InoutArg(_)|HExprKind::Error=>{},
            HExprKind::LetTemp{init,value,..}=>{self.hexpr(init,f);self.hexpr(value,f);}
            HExprKind::Call{args,..}|HExprKind::Math{args,..}|HExprKind::Spawn{args,..}|HExprKind::DynCall{args,..}=>for a in args{self.hexpr(a,f);},
            HExprKind::Neg(x)|HExprKind::Not(x)|HExprKind::StrView(x)|HExprKind::Deref(x)|
            HExprKind::SyncClone(x)|HExprKind::ChanNew(x)|HExprKind::ChanRecv(x)|HExprKind::ChanClose(x)|
            HExprKind::JoinCall(x)|HExprKind::FromUtf8(x)|HExprKind::AddrOf(x)|HExprKind::Some(x)|
            HExprKind::Exit(x)|HExprKind::ErrVal(x)|HExprKind::Try(x)|HExprKind::BoxNew(x)|HExprKind::Unbox(x)=>self.hexpr(x,f),
            HExprKind::Convert{expr,..}=>{
                if !primitive(e.ty)||!primitive(expr.ty){self.add("numeric_conversions",e.span,e.ty.to_string());}
                self.hexpr(expr,f);
            }
            HExprKind::ToFloat(expr)|HExprKind::FloatToInt{expr,..}=>{
                self.add("numeric_conversions",e.span,e.ty.to_string());self.hexpr(expr,f);
            }
            HExprKind::Binary{lhs,rhs,..}|HExprKind::Coalesce{lhs,rhs}|HExprKind::SameTag{lhs,rhs,..}=>{
                if matches!(lhs.ty,Ty::Opt(_)) && matches!(&e.kind,HExprKind::Binary{..}){self.add("optional_comparison",e.span,"optional comparison");}
                self.hexpr(lhs,f);self.hexpr(rhs,f);
            }
            HExprKind::StructLit{fields}|HExprKind::EnumLit{fields,..}=>for (_,x) in fields{self.hexpr(x,f);},
            HExprKind::Field{base,..}=>self.hexpr(base,f),
            HExprKind::VecLit(items)|HExprKind::TupleLit(items)=>for x in items{self.hexpr(x,f);},
            HExprKind::ArenaRead{recv,key,..}|HExprKind::MapRead{recv,key,..}=>{
                if matches!(&e.kind,HExprKind::MapRead{op:MapRead::Get,..}){self.add("map_borrow",e.span,"Map.get");}
                self.hexpr(recv,f);self.hexpr(key,f);
            }
            HExprKind::ArenaWrite{value,..}|HExprKind::DequePush{value,..}|HExprKind::Push{value,..}=>self.hexpr(value,f),
            HExprKind::HeapNew{caps,..}=>{
                if !caps.is_empty(){self.add("heap_captures",e.span,"capturing Heap comparator");}
                for x in caps{self.hexpr(x,f);}
            }
            HExprKind::DequePop{..}|HExprKind::Pop(_)=>{},
            HExprKind::Index{base,index}=>{self.hexpr(base,f);self.hexpr(index,f);}
            HExprKind::SliceRange{base,start,end}=>{
                if !matches!(base.ty,Ty::Str|Ty::String){self.add("sequence_slices",e.span,base.ty.to_string());}
                self.hexpr(base,f);self.hexpr(start,f);self.hexpr(end,f);
            }
            HExprKind::Method{method,recv}=>{
                if matches!(method,Builtin::SliceToVec){self.add("slice_to_vec",e.span,"slice.toVec");}
                self.hexpr(recv,f);
            }
            HExprKind::Reserve{n,..}=>self.hexpr(n,f),
            HExprKind::SyncNew{value,..}=>self.hexpr(value,f),
            HExprKind::Lock{mutex,..}=>self.hexpr(mutex,f),
            HExprKind::ChanSend{chan,value}=>{self.hexpr(chan,f);self.hexpr(value,f);}
            HExprKind::Return(x)|HExprKind::Ok(x)=>if let Some(x)=x{self.hexpr(x,f);},
            HExprKind::Take(p)=>{
                if !matches!(p.ty,Ty::String|Ty::Vec(_)){self.add("take_general",e.span,p.ty.to_string());}
            }
            HExprKind::Replace(_,x)=>{self.add("replace_swap",e.span,"replace");self.hexpr(x,f);}
            HExprKind::Swap(_,_)=>self.add("replace_swap",e.span,"swap"),
            HExprKind::MapLit(items)=>for(k,v)in items{self.hexpr(k,f);self.hexpr(v,f);},
            HExprKind::MapWrite{op,args,..}=>{
                if matches!(op,MapWrite::GetMut){self.add("map_borrow",e.span,"Map.getMut");}
                for x in args{self.hexpr(x,f);}
            }
            HExprKind::Catch{value,handler,..}=>{
                self.hexpr(value,f);
                match handler{CatchHandler::Value(x)=>self.hexpr(x,f),CatchHandler::Block(b)=>self.hbody(b,f)}
            }
            HExprKind::Match{scrut,arms,..}=>self.arms(scrut,arms,f),
            HExprKind::Panic{cond,parts}=>{
                if cond.is_none(){self.add("panic_builtin",e.span,"panic");}
                if let Some(x)=cond{self.hexpr(x,f);}self.parts(parts,f);
            }
            HExprKind::Format(parts)=>self.parts(parts,f),
            HExprKind::MinMax{a,b,..}=>{self.hexpr(a,f);self.hexpr(b,f);}
            HExprKind::Abs(x)=>self.hexpr(x,f),
            HExprKind::Call2{method,recv,args}=>{
                if matches!(method,Method2::ToLower|Method2::ToUpper|Method2::ParseFloat|Method2::TotalCmp|Method2::SeqContains|Method2::Join) {
                    self.add("extra_methods",e.span,format!("{method:?}"));
                }
                self.hexpr(recv,f);for x in args{self.hexpr(x,f);}
            }
            HExprKind::Sort(p)=>{
                let t=p.ty;
                if !matches!(t,Ty::Vec(id) if types::vec_elem(id)==Ty::Int(IntTy::I64)||types::vec_elem(id)==Ty::String){
                    self.add("sort_general",e.span,t.to_string());
                }
            }
            HExprKind::TagTest{value,..}=>self.hexpr(value,f),
            HExprKind::SortBy{caps,..}=>{
                self.add("sort_by",e.span,"sortBy");for x in caps{self.hexpr(x,f);}
            }
            HExprKind::MapUpdate{key,init,caps,..}=>{
                self.add("map_update",e.span,"Map.update");self.hexpr(key,f);self.hexpr(init,f);for x in caps{self.hexpr(x,f);}
            }
            HExprKind::Io{args,..}=>{
                for x in args{self.hexpr(x,f);}
            }
            HExprKind::SliceOf(x)=>self.hexpr(x,f),
            HExprKind::ToDyn{value,..}=>self.hexpr(value,f),
        }
    }
}
