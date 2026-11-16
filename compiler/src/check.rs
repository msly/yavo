//! Name resolution and type checking. Produces a typed IR (`H*`) for code generation.

use std::collections::HashMap;

use crate::ast::*;
use crate::diag::{Diags, Span};
use crate::types::{register_struct, vec_elem, vec_of, IntTy, Ty};

pub type LocalId = usize;

pub struct HProgram {
    pub fns: Vec<HFn>,
    /// Struct definitions, ordered so that every struct comes after the structs it contains.
    pub structs: Vec<HStruct>,
}

pub struct HStruct {
    pub ty: Ty,
    pub fields: Vec<(String, Ty)>,
}

pub struct HFn {
    pub name: String,
    pub params: Vec<LocalId>,
    /// `sinks[i]`: parameter `i` takes ownership of its argument (`x: sink T`).
    pub sinks: Vec<bool>,
    pub ret: Ty,
    pub locals: Vec<HLocal>,
    pub body: Vec<HStmt>,
}

pub struct HLocal {
    pub name: String,
    pub ty: Ty,
    /// The function owns this slot's value and must drop it when the slot goes out of scope.
    /// False for copyable values and for borrowed (non-`sink`) parameters.
    pub owned: bool,
}

pub struct HStmt {
    pub kind: HStmtKind,
    /// Statement position used for output-failure diagnostics.
    pub span: Span,
}

pub enum HStmtKind {
    Let { local: LocalId, init: HExpr },
    /// Evaluate and immediately discard without introducing a name or local slot.
    Discard(HExpr),
    /// `local.path[0].path[1]... = value`; `path` holds field indexes and is empty for a plain variable.
    Assign { local: LocalId, path: Vec<usize>, value: HExpr },
    Expr(HExpr),
    If { cond: HExpr, then: Vec<HStmt>, els: Vec<HStmt> },
    While { cond: HExpr, body: Vec<HStmt> },
    Loop { body: Vec<HStmt> },
    For { local: LocalId, start: HExpr, end: HExpr, body: Vec<HStmt> },
    Break,
    Continue,
    Return(Option<HExpr>),
    Print { fd: u8, newline: bool, parts: Vec<PrintPart> },
    Block(Vec<HStmt>),
}

pub enum PrintPart {
    Text(String),
    Value(HExpr),
}

pub struct HExpr {
    pub kind: HExprKind,
    pub ty: Ty,
    /// Full source range, used for diagnostics.
    pub span: Span,
    /// Position reported by a runtime panic (the operator or the conversion call).
    pub at: Span,
}

pub enum HExprKind {
    Int(i128),
    Bool(bool),
    Str(String),
    Local(LocalId),
    Call { func: usize, args: Vec<HExpr> },
    Neg(Box<HExpr>),
    Not(Box<HExpr>),
    Binary { op: BinOp, lhs: Box<HExpr>, rhs: Box<HExpr> },
    Convert { to: IntTy, checked: bool, expr: Box<HExpr> },
    /// Field values in the order they were written (`usize` is the field index).
    StructLit { fields: Vec<(usize, HExpr)> },
    Field { base: Box<HExpr>, field: usize },
    /// Moves a non-copyable local out; the slot is left empty.
    Move(LocalId),
    VecLit(Vec<HExpr>),
    /// `base[index]` for a copyable element, or a byte of a string.
    Index { base: Box<HExpr>, index: Box<HExpr> },
    Method { method: Builtin, recv: Box<HExpr> },
    /// `local.push(value)`; `value` is moved into a vector or appended to a `String`.
    Push { local: LocalId, value: Box<HExpr> },
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Builtin {
    StrToString,
    StrLen,
    StringLen,
    StringClone,
    VecLen,
    VecClone,
}

const OUTPUT_FNS: [&str; 3] = ["println", "print", "eprintln"];
const UNSUPPORTED_TYPES: &[&str] = &[
    "f32", "f64", "Map", "Set", "Box", "Rc", "Arc", "Arena", "Id", "Mutex", "Atomic", "Channel",
];
const UNSUPPORTED_FNS: &[&str] = &[
    "panic", "assert", "format", "len", "min", "max", "abs", "some", "take", "replace", "swap", "Box", "Rc", "Arc",
    "Mutex",
];

struct Sig {
    params: Vec<Ty>,
    sinks: Vec<bool>,
    ret: Ty,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LocalKind {
    Param,
    SinkParam,
    Let,
    Var,
    LoopVar,
}

struct LocalState {
    name: String,
    ty: Ty,
    kind: LocalKind,
    used: bool,
    span: Span,
}

struct FnCx {
    locals: Vec<LocalState>,
    scopes: Vec<Vec<LocalId>>,
    ret: Ty,
    loop_depth: u32,
    /// `moved[id]`: where the value of local `id` may have been moved out on some path here.
    moved: Moved,
    /// Move states that leave each enclosing loop through `break` or reach its next iteration
    /// through `continue`.
    loops: Vec<LoopFlow>,
}

type Moved = Vec<Option<Span>>;

#[derive(Default)]
struct LoopFlow {
    breaks: Option<Moved>,
    continues: Option<Moved>,
}

/// Locals that may be moved on either path.
fn union(a: &Moved, b: &Moved) -> Moved {
    (0..a.len().max(b.len())).map(|i| a.get(i).copied().flatten().or(b.get(i).copied().flatten())).collect()
}

fn union_into(slot: &mut Option<Moved>, state: &Moved) {
    *slot = Some(match slot.take() {
        Some(prev) => union(&prev, state),
        None => state.clone(),
    });
}

/// Whether control can never fall off the end of these statements.
fn diverges(stmts: &[Stmt]) -> bool {
    terminates(stmts) || stmts.iter().any(|s| matches!(s.kind, StmtKind::Break | StmtKind::Continue))
}

/// Whether `e` mentions the variable `name` anywhere.
fn mentions(e: &Expr, name: &str) -> bool {
    match &e.kind {
        ExprKind::Ident(n) => n == name,
        ExprKind::Int(_) | ExprKind::Bool(_) => false,
        ExprKind::Str(segs) => segs.iter().any(|s| matches!(s, StrSeg::Expr { expr, .. } if mentions(expr, name))),
        ExprKind::Call { callee, args } => mentions(callee, name) || args.iter().any(|a| mentions(a, name)),
        ExprKind::Field { base, .. } => mentions(base, name),
        ExprKind::StructLit { fields, .. } => fields.iter().any(|f| mentions(&f.value, name)),
        ExprKind::VecLit(items) => items.iter().any(|i| mentions(i, name)),
        ExprKind::Index { base, index } => mentions(base, name) || mentions(index, name),
        ExprKind::Unary { expr, .. } | ExprKind::Paren(expr) => mentions(expr, name),
        ExprKind::Binary { lhs, rhs, .. } => mentions(lhs, name) || mentions(rhs, name),
    }
}

struct Checker<'d> {
    diags: &'d mut Diags,
    sigs: Vec<Sig>,
    index: HashMap<String, usize>,
    structs: Vec<StructDef>,
    struct_index: HashMap<String, usize>,
}

struct StructDef {
    name: String,
    span: Span,
    ty: Ty,
    fields: Vec<(String, Ty, Span)>,
}

pub fn check(prog: &Program, diags: &mut Diags) -> Option<HProgram> {
    let mut ck =
        Checker { diags, sigs: Vec::new(), index: HashMap::new(), structs: Vec::new(), struct_index: HashMap::new() };
    let order = ck.collect_structs(prog);
    ck.collect(prog);
    let fns = prog.fns.iter().enumerate().map(|(i, f)| ck.function(f, i)).collect();
    if ck.diags.has_errors() {
        None
    } else {
        let structs = order
            .into_iter()
            .map(|i| {
                let s = &ck.structs[i];
                HStruct { ty: s.ty, fields: s.fields.iter().map(|(n, t, _)| (n.clone(), *t)).collect() }
            })
            .collect();
        Some(HProgram { fns, structs })
    }
}

fn err_expr(span: Span) -> HExpr {
    HExpr { kind: HExprKind::Error, ty: Ty::Error, span, at: span }
}

fn literal_magnitude(e: &Expr) -> Option<u128> {
    match &e.kind {
        ExprKind::Int(v) => Some(*v),
        ExprKind::Paren(inner) => literal_magnitude(inner),
        _ => None,
    }
}

/// Expressions built only from integer literals take their type from context.
fn is_untyped_literal(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Int(_) => true,
        ExprKind::Paren(inner) => is_untyped_literal(inner),
        ExprKind::Unary { op: UnOp::Neg, expr, .. } => is_untyped_literal(expr),
        ExprKind::Binary { op, lhs, rhs, .. } => {
            matches!(
                op,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem | BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap
            ) && is_untyped_literal(lhs)
                && is_untyped_literal(rhs)
        }
        _ => false,
    }
}

fn is_type_name(name: &str) -> bool {
    IntTy::from_name(name).is_some() || matches!(name, "bool" | "str" | "String" | "Vec")
}

fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == *cb { 0 } else { 1 };
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn terminates(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_terminates)
}

fn stmt_terminates(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Return(_) => true,
        StmtKind::If { then, els: Some(els), .. } => {
            terminates(&then.stmts)
                && match els {
                    Else::Block(b) => terminates(&b.stmts),
                    Else::If(s) => stmt_terminates(s),
                }
        }
        StmtKind::Loop { body } => !contains_break(&body.stmts),
        StmtKind::Block(b) => terminates(&b.stmts),
        _ => false,
    }
}

/// Whether a `break` in these statements targets the enclosing loop.
fn contains_break(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Break => true,
        StmtKind::If { then, els, .. } => {
            contains_break(&then.stmts)
                || match els {
                    Some(Else::Block(b)) => contains_break(&b.stmts),
                    Some(Else::If(s)) => contains_break(std::slice::from_ref(s.as_ref())),
                    None => false,
                }
        }
        StmtKind::Block(b) => contains_break(&b.stmts),
        _ => false,
    })
}

impl Checker<'_> {
    /// Registers every struct, resolves field types and returns the structs in dependency order.
    fn collect_structs(&mut self, prog: &Program) -> Vec<usize> {
        for s in &prog.structs {
            if is_type_name(&s.name) || UNSUPPORTED_TYPES.contains(&s.name.as_str()) {
                self.diags.error("E0011", s.name_span, format!("`{}` is a built-in type name and cannot be redefined", s.name));
            } else if let Some(&prev) = self.struct_index.get(&s.name) {
                let line = self.structs[prev].span.line;
                self.diags
                    .error("E0011", s.name_span, format!("struct `{}` is defined more than once", s.name))
                    .note(format!("first defined on line {line}"));
            } else {
                self.struct_index.insert(s.name.clone(), self.structs.len());
            }
            // Every declaration gets an id, even a duplicate, so ids and indexes stay equal.
            let ty = Ty::Struct(register_struct(&s.name));
            self.structs.push(StructDef { name: s.name.clone(), span: s.name_span, ty, fields: Vec::new() });
        }
        for (i, s) in prog.structs.iter().enumerate() {
            let mut fields: Vec<(String, Ty, Span)> = Vec::new();
            for f in &s.fields {
                let ty = self.resolve_ty(&f.ty);
                if ty == Ty::Str {
                    self.diags
                        .error("E0301", f.ty.span, format!("field `{}` cannot hold a `str`: a view cannot be stored in a struct field", f.name))
                        .note("store the text elsewhere; `String` fields are not supported in P0 yet");
                } else if !ty.is_copy() {
                    self.diags.error(
                        "U0001",
                        f.ty.span,
                        format!("struct fields of type `{ty}` (values that move) are not supported in P0 yet"),
                    );
                }
                if let Some((_, _, first)) = fields.iter().find(|(n, _, _)| *n == f.name) {
                    let line = first.line;
                    self.diags
                        .error("E0011", f.span, format!("field `{}` is declared more than once in `{}`", f.name, s.name))
                        .note(format!("first declared on line {line}"));
                }
                fields.push((f.name.clone(), ty, f.span));
            }
            self.structs[i].fields = fields;
        }
        let mut state = vec![0u8; self.structs.len()];
        let mut order = Vec::new();
        for i in 0..self.structs.len() {
            self.visit_struct(i, &mut state, &mut order, &mut Vec::new());
        }
        order
    }

    /// Depth-first walk over by-value containment; a back edge means infinite size.
    fn visit_struct(&mut self, i: usize, state: &mut [u8], order: &mut Vec<usize>, stack: &mut Vec<usize>) {
        match state[i] {
            2 => return,
            1 => {
                let start = stack.iter().position(|&s| s == i).unwrap_or(0);
                let mut path: Vec<&str> = stack[start..].iter().map(|&s| self.structs[s].name.as_str()).collect();
                path.push(&self.structs[i].name);
                let msg = format!(
                    "struct `{}` contains itself by value ({}), so it would have infinite size",
                    self.structs[i].name,
                    path.join(" -> ")
                );
                let span = self.structs[i].span;
                self.diags.error("E0017", span, msg).note("break the cycle with `?Box[T]` (not available in P0 yet)");
                return;
            }
            _ => {}
        }
        state[i] = 1;
        stack.push(i);
        let deps: Vec<usize> = self.structs[i]
            .fields
            .iter()
            .filter_map(|(_, t, _)| if let Ty::Struct(id) = t { Some(*id as usize) } else { None })
            .collect();
        for d in deps {
            self.visit_struct(d, state, order, stack);
        }
        stack.pop();
        state[i] = 2;
        order.push(i);
    }

    fn struct_field(&self, ty: Ty, name: &str) -> Option<(usize, Ty)> {
        let Ty::Struct(id) = ty else { return None };
        let fields = &self.structs[id as usize].fields;
        fields.iter().position(|(n, _, _)| n == name).map(|i| (i, fields[i].1))
    }

    fn no_field(&mut self, ty: Ty, name: &str, span: Span) {
        let Ty::Struct(id) = ty else { return };
        let candidates: Vec<String> = self.structs[id as usize].fields.iter().map(|(n, _, _)| n.clone()).collect();
        let d = self.diags.error("E0010", span, format!("struct `{ty}` has no field `{name}`"));
        if !candidates.is_empty() {
            d.note(format!("its fields are: {}", candidates.join(", ")));
        }
    }

    fn struct_lit(&mut self, name: &str, name_span: Span, inits: &[FieldInit], span: Span, cx: &mut FnCx) -> HExpr {
        let Some(&si) = self.struct_index.get(name) else {
            if UNSUPPORTED_TYPES.contains(&name) {
                self.diags.error("U0001", name_span, format!("constructing `{name}` is not supported in P0"));
            } else {
                self.diags.error("E0010", name_span, format!("cannot find struct `{name}`"));
            }
            for init in inits {
                self.expr(&init.value, None, cx);
            }
            return err_expr(span);
        };
        let decl: Vec<(String, Ty)> = self.structs[si].fields.iter().map(|(n, t, _)| (n.clone(), *t)).collect();
        let sty = self.structs[si].ty;
        let mut seen = vec![false; decl.len()];
        let mut out = Vec::new();
        let mut ok = true;
        for init in inits {
            match decl.iter().position(|(n, _)| *n == init.name) {
                Some(fi) if !seen[fi] => {
                    seen[fi] = true;
                    let fty = decl[fi].1;
                    let h = self.expr(&init.value, Some(fty), cx);
                    self.expect_ty(&h, fty);
                    out.push((fi, h));
                }
                Some(_) => {
                    self.diags.error("E0011", init.span, format!("field `{}` is given more than once", init.name));
                    self.expr(&init.value, None, cx);
                    ok = false;
                }
                None => {
                    self.no_field(sty, &init.name, init.span);
                    self.expr(&init.value, None, cx);
                    ok = false;
                }
            }
        }
        let missing: Vec<String> =
            decl.iter().zip(&seen).filter(|(_, s)| !**s).map(|((n, _), _)| format!("`{n}`")).collect();
        if !missing.is_empty() {
            self.diags
                .error("E0018", name_span, format!("missing {} in `{name}`: {}", plural(missing.len(), "field"), missing.join(", ")))
                .note("every field must be given; Yavo has no implicit default values");
            ok = false;
        }
        if !ok {
            return err_expr(span);
        }
        HExpr { kind: HExprKind::StructLit { fields: out }, ty: sty, span, at: span }
    }

    fn collect(&mut self, prog: &Program) {
        for f in &prog.fns {
            let params = f.params.iter().map(|p| self.resolve_ty(&p.ty)).collect();
            let sinks = f.params.iter().map(|p| p.sink).collect();
            let ret = f.ret.as_ref().map(|t| self.resolve_ty(t)).unwrap_or(Ty::Unit);
            if OUTPUT_FNS.contains(&f.name.as_str()) || is_type_name(&f.name) {
                self.diags.error("E0011", f.name_span, format!("`{}` is a built-in name and cannot be redefined", f.name));
            } else if let Some(&prev) = self.index.get(&f.name) {
                let line = prog.fns[prev].name_span.line;
                self.diags
                    .error("E0011", f.name_span, format!("function `{}` is defined more than once", f.name))
                    .note(format!("first defined on line {line}"));
            } else {
                self.index.insert(f.name.clone(), self.sigs.len());
            }
            self.sigs.push(Sig { params, sinks, ret });
        }
        match self.index.get("main") {
            None => {
                let span = Span { start: 0, end: 0, line: 1, col: 1 };
                self.diags
                    .error("E0016", span, "no `fn main()` found; every program needs an entry point")
                    .note("add `fn main() { ... }`");
            }
            Some(&i) => {
                let f = &prog.fns[i];
                if !f.params.is_empty() || self.sigs[i].ret != Ty::Unit {
                    self.diags.error(
                        "E0016",
                        f.name_span,
                        "`main` must be declared as `fn main()` with no parameters and no return type",
                    );
                }
            }
        }
    }

    fn resolve_ty(&mut self, t: &TypeExpr) -> Ty {
        if t.name == "Vec" {
            let [elem] = t.args.as_slice() else {
                self.diags.error("E0001", t.span, "`Vec` takes exactly one type argument, e.g. `Vec[int]`");
                return Ty::Error;
            };
            return match self.resolve_ty(elem) {
                Ty::Error => Ty::Error,
                e => Ty::Vec(vec_of(e)),
            };
        }
        if !t.args.is_empty() {
            if UNSUPPORTED_TYPES.contains(&t.name.as_str()) {
                self.diags.error("U0001", t.span, format!("type `{}` is not supported in P0", t.name));
            } else if is_type_name(&t.name) || self.struct_index.contains_key(&t.name) {
                self.diags.error("E0001", t.span, format!("type `{}` takes no type arguments", t.name));
            } else {
                self.diags.error("E0010", t.span, format!("unknown type `{}`", t.name));
            }
            return Ty::Error;
        }
        if let Some(it) = IntTy::from_name(&t.name) {
            return Ty::Int(it);
        }
        if let Some(&i) = self.struct_index.get(&t.name) {
            return self.structs[i].ty;
        }
        match t.name.as_str() {
            "bool" => Ty::Bool,
            "str" => Ty::Str,
            "String" => Ty::String,
            name if UNSUPPORTED_TYPES.contains(&name) => {
                self.diags.error(
                    "U0001",
                    t.span,
                    format!("type `{name}` is not supported in P0 (available: integers, bool, str, String, Vec[T], structs)"),
                );
                Ty::Error
            }
            name => {
                self.diags.error("E0010", t.span, format!("unknown type `{name}`"));
                Ty::Error
            }
        }
    }

    fn function(&mut self, f: &FnDecl, idx: usize) -> HFn {
        let ret = self.sigs[idx].ret;
        let param_tys = self.sigs[idx].params.clone();
        let sinks = self.sigs[idx].sinks.clone();
        let mut cx =
            FnCx { locals: Vec::new(), scopes: vec![Vec::new()], ret, loop_depth: 0, moved: Vec::new(), loops: Vec::new() };
        let params = f
            .params
            .iter()
            .zip(param_tys)
            .map(|(p, ty)| {
                let kind = if p.sink { LocalKind::SinkParam } else { LocalKind::Param };
                self.declare(&mut cx, &p.name, p.span, ty, kind)
            })
            .collect();
        let body = self.stmts(&f.body.stmts, &mut cx);
        if ret != Ty::Unit && ret != Ty::Error && !terminates(&f.body.stmts) {
            self.diags
                .error(
                    "E0013",
                    f.body.close,
                    format!("function `{}` can reach its end without returning a value of type `{ret}`", f.name),
                )
                .note("add a `return` statement before the closing `}`");
        }
        self.pop_scope(&mut cx);
        HFn {
            name: f.name.clone(),
            params,
            sinks,
            ret,
            locals: cx
                .locals
                .into_iter()
                .map(|l| HLocal { owned: !l.ty.is_copy() && l.kind != LocalKind::Param, name: l.name, ty: l.ty })
                .collect(),
            body,
        }
    }

    fn lookup(&self, cx: &FnCx, name: &str) -> Option<LocalId> {
        cx.scopes.iter().rev().flat_map(|s| s.iter().rev()).copied().find(|&id| cx.locals[id].name == name)
    }

    fn declare(&mut self, cx: &mut FnCx, name: &str, span: Span, ty: Ty, kind: LocalKind) -> LocalId {
        if name != "_" {
            if let Some(prev) = self.lookup(cx, name) {
                let line = cx.locals[prev].span.line;
                self.diags
                    .error(
                        "E0011",
                        span,
                        format!("`{name}` is already defined in this function (line {line}); Yavo does not allow shadowing"),
                    )
                    .note("choose a different name");
            }
        }
        let id = cx.locals.len();
        cx.locals.push(LocalState { name: name.to_string(), ty, kind, used: false, span });
        cx.moved.resize(cx.locals.len(), None);
        cx.moved[id] = None;
        // Parameters and range iterators still need an IR slot, but `_` never
        // participates in name lookup (nor does it conflict with another `_`).
        if name != "_" {
            cx.scopes.last_mut().unwrap().push(id);
        }
        id
    }

    fn pop_scope(&mut self, cx: &mut FnCx) {
        let scope = cx.scopes.pop().unwrap_or_default();
        for id in scope {
            let l = &cx.locals[id];
            if !l.used && !l.name.starts_with('_') && l.ty != Ty::Error {
                self.diags
                    .warn("W0001", l.span, format!("unused variable `{}`", l.name))
                    .note(format!("rename it to `_{}` if this is intentional", l.name));
            }
        }
    }

    fn similar_name(&self, cx: &FnCx, name: &str, want_fn: bool) -> Option<String> {
        let candidates: Vec<&str> = if want_fn {
            self.index.keys().map(|s| s.as_str()).collect()
        } else {
            cx.scopes.iter().flatten().map(|&id| cx.locals[id].name.as_str()).collect()
        };
        candidates
            .into_iter()
            .filter(|cand| *cand != name && edit_distance(cand, name) <= 2.min(name.len() / 2 + 1))
            .min_by_key(|cand| (edit_distance(cand, name), *cand))
            .map(str::to_string)
    }

    fn expect_ty(&mut self, e: &HExpr, expected: Ty) {
        if e.ty == expected || e.ty == Ty::Error || expected == Ty::Error {
            return;
        }
        let d = self.diags.error("E0001", e.span, format!("mismatched types: expected `{expected}`, found `{}`", e.ty));
        if let (Ty::Int(_), Ty::Int(_)) = (e.ty, expected) {
            d.note(format!("Yavo has no implicit conversions; convert explicitly with `{expected}(...)`"));
        }
    }

    fn block(&mut self, b: &Block, cx: &mut FnCx) -> Vec<HStmt> {
        cx.scopes.push(Vec::new());
        let out = self.stmts(&b.stmts, cx);
        self.pop_scope(cx);
        out
    }

    fn check_moved(&mut self, cx: &FnCx, id: LocalId, span: Span) {
        if let Some(at) = cx.moved.get(id).copied().flatten() {
            let name = cx.locals[id].name.clone();
            self.diags
                .error("E0402", span, format!("use of moved value `{name}`"))
                .note(format!("it was moved on line {} (on at least one path to here)", at.line))
                .note(format!("keep a copy with `{name}.clone()` there, or use it before moving it"));
        }
    }

    /// The value of `h` is moved: stored, returned, or passed to a `sink` parameter. A local of a
    /// non-copyable type is moved out and must not be used again.
    fn consume(&mut self, mut h: HExpr, cx: &mut FnCx) -> HExpr {
        let HExprKind::Local(id) = h.kind else { return h };
        if h.ty.is_copy() || h.ty == Ty::Error {
            return h;
        }
        if cx.locals[id].kind == LocalKind::Param {
            let name = cx.locals[id].name.clone();
            self.diags
                .error("E0401", h.span, format!("cannot move out of `{name}`: it is a borrowed parameter"))
                .note(format!("declare it as `{name}: sink {}` to take ownership, or pass `{name}.clone()`", h.ty));
            return h;
        }
        if cx.moved.len() <= id {
            cx.moved.resize(id + 1, None);
        }
        cx.moved[id] = Some(h.span);
        h.kind = HExprKind::Move(id);
        h
    }

    /// Checks a loop body. A value moved in one iteration must not be used by the next, so when
    /// the body moves something, it is checked a second time (diagnostics only) from the state
    /// at the end of the first iteration.
    fn loop_body(&mut self, body: &Block, cx: &mut FnCx, infinite: bool) -> Vec<HStmt> {
        let before = cx.moved.clone();
        cx.loops.push(LoopFlow::default());
        cx.loop_depth += 1;
        let out = self.block(body, cx);
        cx.loop_depth -= 1;
        let mut flow = cx.loops.pop().unwrap_or_default();
        let mut end = cx.moved.clone();
        if let Some(c) = &flow.continues {
            end = union(&end, c);
        }
        let next = union(&before, &end);
        let grew = next.iter().enumerate().any(|(i, m)| m.is_some() && before.get(i).copied().flatten().is_none());
        if grew {
            let saved = std::mem::take(&mut *self.diags);
            cx.moved = next.clone();
            cx.loops.push(LoopFlow::default());
            cx.loop_depth += 1;
            let _ = self.block(body, cx);
            cx.loop_depth -= 1;
            if let Some(b) = cx.loops.pop().and_then(|f| f.breaks) {
                union_into(&mut flow.breaks, &b);
            }
            let recheck = std::mem::replace(&mut *self.diags, saved);
            for d in recheck.items {
                if d.code == "E0402" && !self.diags.items.iter().any(|x| x.code == d.code && x.span == d.span) {
                    self.diags.items.push(d);
                }
            }
        }
        cx.moved = match (infinite, flow.breaks) {
            (true, Some(b)) => b,
            (false, Some(b)) => union(&next, &b),
            (_, None) => next,
        };
        out
    }

    fn stmts(&mut self, stmts: &[Stmt], cx: &mut FnCx) -> Vec<HStmt> {
        stmts.iter().filter_map(|s| self.stmt(s, cx)).collect()
    }

    fn cond(&mut self, e: &Expr, cx: &mut FnCx) -> HExpr {
        let h = self.expr(e, Some(Ty::Bool), cx);
        self.expect_ty(&h, Ty::Bool);
        h
    }

    fn stmt(&mut self, s: &Stmt, cx: &mut FnCx) -> Option<HStmt> {
        let kind = match &s.kind {
            StmtKind::Let { mutable, name, name_span, ty, init } => {
                let declared = ty.as_ref().map(|t| self.resolve_ty(t));
                let init = self.expr(init, declared, cx);
                if init.ty == Ty::Unit && name != "_" {
                    self.diags.error("E0001", init.span, "this call returns nothing, so it cannot initialize a variable");
                }
                let ty = match declared {
                    Some(d) => {
                        self.expect_ty(&init, d);
                        d
                    }
                    None => init.ty,
                };
                if name == "_" {
                    // `let _ = x` neither moves nor drops a variable; any other value is dropped now.
                    HStmtKind::Discard(init)
                } else {
                    let init = self.consume(init, cx);
                    let kind = if *mutable { LocalKind::Var } else { LocalKind::Let };
                    let local = self.declare(cx, name, *name_span, ty, kind);
                    HStmtKind::Let { local, init }
                }
            }
            StmtKind::Assign { target, op, op_span, value } => {
                // The target is a variable, or a chain of struct fields rooted at a variable.
                let mut fields_rev: Vec<(&str, Span)> = Vec::new();
                let mut root = target;
                loop {
                    match &root.kind {
                        ExprKind::Field { base, name } => {
                            fields_rev.push((name.as_str(), root.span));
                            root = base;
                        }
                        ExprKind::Paren(inner) => root = inner,
                        _ => break,
                    }
                }
                let ExprKind::Ident(name) = &root.kind else {
                    self.diags.error("U0001", target.span, "only variables and their struct fields can be assigned in P0");
                    return None;
                };
                let Some(id) = self.lookup(cx, name) else {
                    self.diags.error("E0010", target.span, format!("cannot find variable `{name}` in this scope"));
                    self.expr(value, None, cx);
                    return None;
                };
                match cx.locals[id].kind {
                    LocalKind::Var => {}
                    LocalKind::Let => {
                        self.diags
                            .error("E0014", target.span, format!("cannot assign to `{name}` because it was declared with `let`"))
                            .note(format!("declare it with `var {name}` to make it mutable"));
                    }
                    LocalKind::Param | LocalKind::SinkParam => {
                        self.diags
                            .error("E0014", target.span, format!("cannot assign to parameter `{name}`; parameters are read-only"))
                            .note(format!("copy it into a local first: `var {name}2 = {name}`"));
                    }
                    LocalKind::LoopVar => {
                        self.diags.error("E0014", target.span, format!("cannot assign to loop variable `{name}`"));
                    }
                }
                let mut lty = cx.locals[id].ty;
                let mut path = Vec::new();
                for (fname, fspan) in fields_rev.iter().rev() {
                    match self.struct_field(lty, fname) {
                        Some((fi, fty)) => {
                            path.push(fi);
                            lty = fty;
                        }
                        None => {
                            match lty {
                                Ty::Struct(_) => self.no_field(lty, fname, *fspan),
                                Ty::Error => {}
                                other => {
                                    self.diags.error("E0001", *fspan, format!("a value of type `{other}` has no fields"));
                                }
                            }
                            self.expr(value, None, cx);
                            return None;
                        }
                    }
                }
                let value = match op {
                    None => {
                        let v = self.expr(value, Some(lty), cx);
                        self.expect_ty(&v, lty);
                        let v = self.consume(v, cx);
                        if path.is_empty() && id < cx.moved.len() {
                            // Assigning a new value makes a moved-from variable usable again.
                            cx.moved[id] = None;
                        }
                        v
                    }
                    Some(op) => {
                        cx.locals[id].used = true;
                        let mut lhs =
                            HExpr { kind: HExprKind::Local(id), ty: cx.locals[id].ty, span: root.span, at: root.span };
                        let mut ty = lhs.ty;
                        for &fi in &path {
                            let Ty::Struct(sid) = ty else { unreachable!() };
                            ty = self.structs[sid as usize].fields[fi].1;
                            lhs = HExpr { kind: HExprKind::Field { base: Box::new(lhs), field: fi }, ty, span: target.span, at: target.span };
                        }
                        let rhs = self.expr(value, Some(lty), cx);
                        self.arith(*op, lhs, rhs, *op_span, s.span)
                    }
                };
                HStmtKind::Assign { local: id, path, value }
            }
            StmtKind::Expr(e) => {
                let ExprKind::Call { callee, args } = &e.kind else {
                    self.diags.error(
                        "E0001",
                        e.span,
                        "this expression's value is not used; only function calls can stand alone as statements",
                    );
                    self.expr(e, None, cx);
                    return None;
                };
                if let ExprKind::Ident(n) = &callee.kind {
                    if OUTPUT_FNS.contains(&n.as_str()) && self.lookup(cx, n).is_none() {
                        return self.print(n, args, e.span, cx).map(|kind| HStmt { kind, span: s.span });
                    }
                }
                HStmtKind::Expr(self.expr(e, None, cx))
            }
            StmtKind::If { cond, then, els } => {
                let cond = self.cond(cond, cx);
                let before = cx.moved.clone();
                let then_h = self.block(then, cx);
                let after_then = std::mem::replace(&mut cx.moved, before);
                let els_h = match els {
                    None => Vec::new(),
                    Some(Else::Block(b)) => self.block(b, cx),
                    Some(Else::If(s)) => self.stmt(s, cx).into_iter().collect(),
                };
                let after_else = std::mem::take(&mut cx.moved);
                // A branch that cannot fall through does not affect the code after the `if`.
                let then_div = diverges(&then.stmts);
                let else_div = match els {
                    Some(Else::Block(b)) => diverges(&b.stmts),
                    Some(Else::If(s)) => diverges(std::slice::from_ref(s.as_ref())),
                    None => false,
                };
                cx.moved = match (then_div, else_div) {
                    (true, false) => after_else,
                    (false, true) => after_then,
                    _ => union(&after_then, &after_else),
                };
                HStmtKind::If { cond, then: then_h, els: els_h }
            }
            StmtKind::While { cond, body } => {
                let cond = self.cond(cond, cx);
                let body = self.loop_body(body, cx, false);
                HStmtKind::While { cond, body }
            }
            StmtKind::Loop { body } => {
                let body = self.loop_body(body, cx, true);
                HStmtKind::Loop { body }
            }
            StmtKind::For { var, var_span, start, end, body } => {
                let (start, end) = self.pair(start, end, None, cx);
                let mut ty = start.ty;
                if start.ty != Ty::Error && end.ty != Ty::Error {
                    if !matches!(start.ty, Ty::Int(_)) {
                        self.diags.error("E0001", start.span, format!("range bounds must be integers, found `{}`", start.ty));
                        ty = Ty::Error;
                    } else {
                        self.expect_ty(&end, start.ty);
                    }
                }
                cx.scopes.push(Vec::new());
                let local = self.declare(cx, var, *var_span, ty, LocalKind::LoopVar);
                let body = self.loop_body(body, cx, false);
                self.pop_scope(cx);
                HStmtKind::For { local, start, end, body }
            }
            StmtKind::Break | StmtKind::Continue => {
                let is_break = matches!(s.kind, StmtKind::Break);
                if cx.loop_depth == 0 {
                    let word = if is_break { "break" } else { "continue" };
                    self.diags.error("E0015", s.span, format!("`{word}` can only be used inside a loop"));
                    return None;
                }
                let state = cx.moved.clone();
                if let Some(flow) = cx.loops.last_mut() {
                    union_into(if is_break { &mut flow.breaks } else { &mut flow.continues }, &state);
                }
                if is_break {
                    HStmtKind::Break
                } else {
                    HStmtKind::Continue
                }
            }
            StmtKind::Return(value) => {
                let ret = cx.ret;
                match value {
                    None => {
                        if ret != Ty::Unit && ret != Ty::Error {
                            self.diags.error("E0001", s.span, format!("this function must return a value of type `{ret}`"));
                        }
                        HStmtKind::Return(None)
                    }
                    Some(e) if ret == Ty::Unit => {
                        self.diags
                            .error("E0001", e.span, "this function has no return type, so `return` cannot carry a value")
                            .note("add `-> T` to the function signature");
                        self.expr(e, None, cx);
                        HStmtKind::Return(None)
                    }
                    Some(e) => {
                        let h = self.expr(e, Some(ret), cx);
                        self.expect_ty(&h, ret);
                        HStmtKind::Return(Some(self.consume(h, cx)))
                    }
                }
            }
            StmtKind::Block(b) => HStmtKind::Block(self.block(b, cx)),
        };
        Some(HStmt { kind, span: s.span })
    }

    fn print(&mut self, name: &str, args: &[Expr], span: Span, cx: &mut FnCx) -> Option<HStmtKind> {
        let fd = if name == "eprintln" { 2 } else { 1 };
        let newline = name != "print";
        let mut parts = Vec::new();
        match args {
            [] if newline => {}
            [arg] => match &arg.kind {
                ExprKind::Str(segs) => {
                    for seg in segs {
                        match seg {
                            StrSeg::Text(t) => parts.push(PrintPart::Text(t.clone())),
                            StrSeg::Expr { expr, spec } => {
                                if spec.is_some() {
                                    self.diags.error(
                                        "U0001",
                                        expr.span,
                                        "format specifiers such as `${x:.2}` are not supported in P0",
                                    );
                                }
                                let h = self.expr(expr, None, cx);
                                self.printable(&h);
                                parts.push(PrintPart::Value(h));
                            }
                        }
                    }
                }
                _ => {
                    let h = self.expr(arg, None, cx);
                    self.printable(&h);
                    parts.push(PrintPart::Value(h));
                }
            },
            _ => {
                self.diags
                    .error("E0012", span, format!("`{name}` takes exactly one argument"))
                    .note("use interpolation to print several values: `println(\"${a} ${b}\")`");
                return None;
            }
        }
        Some(HStmtKind::Print { fd, newline, parts })
    }

    fn printable(&mut self, h: &HExpr) {
        if h.ty == Ty::Unit {
            self.diags.error("E0001", h.span, "this call returns nothing, so there is nothing to print");
        }
        if let Ty::Struct(_) | Ty::Vec(_) = h.ty {
            self.diags
                .error("E0001", h.span, format!("a value of type `{}` cannot be printed directly", h.ty))
                .note("print its parts instead, e.g. `println(\"${p.x} ${p.y}\")`");
        }
    }

    fn int_lit(&mut self, mag: u128, negative: bool, expected: Option<Ty>, span: Span) -> HExpr {
        let it = match expected {
            Some(Ty::Int(t)) => t,
            _ => IntTy::I64,
        };
        let value = (mag <= u64::MAX as u128).then(|| if negative { -(mag as i128) } else { mag as i128 });
        match value {
            Some(v) if v >= it.min() && v <= it.max() => {
                HExpr { kind: HExprKind::Int(v), ty: Ty::Int(it), span, at: span }
            }
            _ => {
                let text = format!("{}{mag}", if negative { "-" } else { "" });
                let d = self.diags.error(
                    "E0003",
                    span,
                    format!("literal `{text}` does not fit in `{it}` (range {}..={})", it.min(), it.max()),
                );
                if expected.is_none() && !negative && mag <= u64::MAX as u128 {
                    d.note("give it an explicit type, e.g. `let x: u64 = ...`");
                }
                HExpr { kind: HExprKind::Error, ty: Ty::Int(it), span, at: span }
            }
        }
    }

    /// Types two operands so that an untyped literal adopts the other side's type.
    fn pair(&mut self, a: &Expr, b: &Expr, expected: Option<Ty>, cx: &mut FnCx) -> (HExpr, HExpr) {
        let hint = |t: Ty, fallback: Option<Ty>| if matches!(t, Ty::Int(_)) { Some(t) } else { fallback };
        if is_untyped_literal(a) && !is_untyped_literal(b) {
            let hb = self.expr(b, expected, cx);
            let ha = self.expr(a, hint(hb.ty, expected), cx);
            (ha, hb)
        } else {
            let ha = self.expr(a, expected, cx);
            let hb = self.expr(b, hint(ha.ty, expected), cx);
            (ha, hb)
        }
    }

    fn arith(&mut self, op: BinOp, l: HExpr, r: HExpr, at: Span, span: Span) -> HExpr {
        let ty = match (l.ty, r.ty) {
            (Ty::Error, _) | (_, Ty::Error) => Ty::Error,
            (Ty::Int(a), Ty::Int(b)) if a == b => l.ty,
            (Ty::Int(a), Ty::Int(b)) => {
                self.diags
                    .error("E0001", span, format!("mismatched operand types for `{}`: `{a}` and `{b}`", op.symbol()))
                    .note(format!("Yavo has no implicit conversions; convert one side explicitly, e.g. `{a}(...)`"));
                Ty::Error
            }
            (a, b) => {
                self.diags.error(
                    "E0001",
                    span,
                    format!("operator `{}` needs integer operands, found `{a}` and `{b}`", op.symbol()),
                );
                Ty::Error
            }
        };
        HExpr { kind: HExprKind::Binary { op, lhs: Box::new(l), rhs: Box::new(r) }, ty, span, at }
    }

    fn expr(&mut self, e: &Expr, expected: Option<Ty>, cx: &mut FnCx) -> HExpr {
        let span = e.span;
        match &e.kind {
            ExprKind::Int(v) => self.int_lit(*v, false, expected, span),
            ExprKind::Bool(b) => HExpr { kind: HExprKind::Bool(*b), ty: Ty::Bool, span, at: span },
            ExprKind::Paren(inner) => {
                let mut h = self.expr(inner, expected, cx);
                h.span = span;
                h
            }
            ExprKind::Str(segs) => {
                if segs.iter().any(|s| matches!(s, StrSeg::Expr { .. })) {
                    self.diags.error(
                        "E0001",
                        span,
                        "string interpolation can only be passed directly to `println`, `print` or `eprintln` in P0",
                    );
                    return err_expr(span);
                }
                let text: String = segs.iter().map(|s| if let StrSeg::Text(t) = s { t.as_str() } else { "" }).collect();
                HExpr { kind: HExprKind::Str(text), ty: Ty::Str, span, at: span }
            }
            ExprKind::Unary { op: UnOp::Neg, op_span, expr: inner } => {
                if let Some(mag) = literal_magnitude(inner) {
                    return self.int_lit(mag, true, expected, span);
                }
                let h = self.expr(inner, expected, cx);
                match h.ty {
                    Ty::Int(t) if t.signed() => {
                        HExpr { kind: HExprKind::Neg(Box::new(h)), ty: Ty::Int(t), span, at: *op_span }
                    }
                    Ty::Int(t) => {
                        self.diags
                            .error("E0001", span, format!("cannot negate a value of unsigned type `{t}`"))
                            .note("use `0 -% x` for wrapping negation");
                        err_expr(span)
                    }
                    Ty::Error => err_expr(span),
                    other => {
                        let d = self.diags.error("E0001", span, format!("cannot negate a value of type `{other}`"));
                        if other == Ty::Bool {
                            d.note("use `!` to negate a bool");
                        }
                        err_expr(span)
                    }
                }
            }
            ExprKind::Unary { op: UnOp::Not, expr: inner, .. } => {
                let h = self.expr(inner, Some(Ty::Bool), cx);
                self.expect_ty(&h, Ty::Bool);
                HExpr { kind: HExprKind::Not(Box::new(h)), ty: Ty::Bool, span, at: span }
            }
            ExprKind::Binary { op, op_span, lhs, rhs } => self.binary(*op, *op_span, lhs, rhs, expected, span, cx),
            ExprKind::Ident(name) => {
                if let Some(id) = self.lookup(cx, name) {
                    cx.locals[id].used = true;
                    self.check_moved(cx, id, span);
                    return HExpr { kind: HExprKind::Local(id), ty: cx.locals[id].ty, span, at: span };
                }
                if self.index.contains_key(name) {
                    self.diags.error(
                        "U0001",
                        span,
                        format!("functions cannot be used as values in P0; call it with `{name}(...)`"),
                    );
                } else if is_type_name(name) || self.struct_index.contains_key(name) {
                    self.diags.error("E0001", span, format!("`{name}` is a type, not a value"));
                } else if OUTPUT_FNS.contains(&name.as_str()) {
                    self.diags.error("E0001", span, format!("`{name}` must be called"));
                } else {
                    let suggestion = self.similar_name(cx, name, false);
                    let d = self.diags.error("E0010", span, format!("cannot find `{name}` in this scope"));
                    if let Some(s) = suggestion {
                        d.note(format!("did you mean `{s}`?"));
                    }
                }
                err_expr(span)
            }
            ExprKind::Field { base, name } => {
                if let ExprKind::Ident(tn) = &base.kind {
                    if self.lookup(cx, tn).is_none() {
                        if let Some(it) = IntTy::from_name(tn) {
                            let v = match name.as_str() {
                                "min" => Some(it.min()),
                                "max" => Some(it.max()),
                                _ => None,
                            };
                            if let Some(v) = v {
                                return HExpr { kind: HExprKind::Int(v), ty: Ty::Int(it), span, at: span };
                            }
                        }
                    }
                }
                // A field of a struct value. A bare name that is not a variable (a type or module)
                // keeps the "not supported" diagnostic below.
                let named_non_local = matches!(&base.kind, ExprKind::Ident(n) if self.lookup(cx, n).is_none());
                if !named_non_local {
                    let b = self.expr(base, None, cx);
                    match b.ty {
                        Ty::Struct(_) => {
                            return match self.struct_field(b.ty, name) {
                                Some((field, ty)) => {
                                    HExpr { kind: HExprKind::Field { base: Box::new(b), field }, ty, span, at: span }
                                }
                                None => {
                                    self.no_field(b.ty, name, span);
                                    err_expr(span)
                                }
                            };
                        }
                        Ty::Error => return err_expr(span),
                        _ => {}
                    }
                }
                self.diags.error(
                    "U0001",
                    span,
                    "fields and methods are not supported in P0 (only `T.min`, `T.max` and `T.wrap(x)` on integer types)",
                );
                err_expr(span)
            }
            ExprKind::StructLit { name, name_span, fields } => self.struct_lit(name, *name_span, fields, span, cx),
            ExprKind::VecLit(items) => self.vec_lit(items, expected, span, cx),
            ExprKind::Index { base, index } => self.index(base, index, span, cx),
            ExprKind::Call { callee, args } => self.call(callee, args, span, cx),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn binary(
        &mut self,
        op: BinOp,
        at: Span,
        lhs: &Expr,
        rhs: &Expr,
        expected: Option<Ty>,
        span: Span,
        cx: &mut FnCx,
    ) -> HExpr {
        match op {
            BinOp::And | BinOp::Or => {
                let l = self.cond(lhs, cx);
                let r = self.cond(rhs, cx);
                HExpr { kind: HExprKind::Binary { op, lhs: Box::new(l), rhs: Box::new(r) }, ty: Ty::Bool, span, at }
            }
            BinOp::Shl | BinOp::Shr => {
                let l = self.expr(lhs, expected, cx);
                let r = self.expr(rhs, None, cx);
                let ty = match (l.ty, r.ty) {
                    (Ty::Error, _) | (_, Ty::Error) => Ty::Error,
                    (Ty::Int(_), Ty::Int(_)) => l.ty,
                    (Ty::Int(_), other) => {
                        self.diags.error("E0001", r.span, format!("shift amount must be an integer, found `{other}`"));
                        Ty::Error
                    }
                    (other, _) => {
                        self.diags.error("E0001", l.span, format!("cannot shift a value of type `{other}`"));
                        Ty::Error
                    }
                };
                HExpr { kind: HExprKind::Binary { op, lhs: Box::new(l), rhs: Box::new(r) }, ty, span, at }
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let (l, r) = self.pair(lhs, rhs, None, cx);
                if l.ty != Ty::Error && r.ty != Ty::Error {
                    let ordering = !matches!(op, BinOp::Eq | BinOp::Ne);
                    if l.ty != r.ty {
                        let d = self.diags.error("E0001", span, format!("cannot compare `{}` with `{}`", l.ty, r.ty));
                        if let (Ty::Int(a), Ty::Int(_)) = (l.ty, r.ty) {
                            d.note(format!("convert one side explicitly, e.g. `{a}(...)`"));
                        }
                    } else if (ordering && !matches!(l.ty, Ty::Int(_) | Ty::Str | Ty::String))
                        || !matches!(l.ty, Ty::Int(_) | Ty::Bool | Ty::Str | Ty::String)
                    {
                        let what = if ordering { "ordered with `<`, `<=`, `>`, `>=`" } else { "compared" };
                        self.diags.error("E0501", span, format!("values of type `{}` cannot be {what}", l.ty));
                    }
                }
                HExpr { kind: HExprKind::Binary { op, lhs: Box::new(l), rhs: Box::new(r) }, ty: Ty::Bool, span, at }
            }
            _ => {
                let (l, r) = self.pair(lhs, rhs, expected, cx);
                self.arith(op, l, r, at, span)
            }
        }
    }

    fn call(&mut self, callee: &Expr, args: &[Expr], span: Span, cx: &mut FnCx) -> HExpr {
        match &callee.kind {
            ExprKind::Ident(name) => {
                if self.lookup(cx, name).is_some() {
                    self.diags.error("E0001", callee.span, format!("`{name}` is a variable, not a function"));
                    return err_expr(span);
                }
                if let Some(to) = IntTy::from_name(name) {
                    return self.conversion(to, true, args, callee.span, span, cx);
                }
                if OUTPUT_FNS.contains(&name.as_str()) {
                    self.diags.error("E0001", span, format!("`{name}` does not produce a value; call it as a statement"));
                    return err_expr(span);
                }
                let Some(&fi) = self.index.get(name) else {
                    if UNSUPPORTED_FNS.contains(&name.as_str()) {
                        self.diags.error("U0001", callee.span, format!("`{name}` is not available in P0"));
                    } else {
                        let suggestion = self.similar_name(cx, name, true);
                        let d = self.diags.error("E0010", callee.span, format!("cannot find function `{name}`"));
                        if let Some(s) = suggestion {
                            d.note(format!("did you mean `{s}`?"));
                        }
                    }
                    return err_expr(span);
                };
                let params = self.sigs[fi].params.clone();
                let ret = self.sigs[fi].ret;
                if args.len() != params.len() {
                    self.diags.error(
                        "E0012",
                        span,
                        format!(
                            "function `{name}` takes {} but {} {} supplied",
                            plural(params.len(), "argument"),
                            args.len(),
                            if args.len() == 1 { "was" } else { "were" }
                        ),
                    );
                }
                // Arguments are processed left to right; each is borrowed or (for a `sink`
                // parameter) moved before the next one is evaluated.
                let sinks = self.sigs[fi].sinks.clone();
                let mut borrowed: Vec<LocalId> = Vec::new();
                let mut hargs = Vec::new();
                for (i, a) in args.iter().enumerate() {
                    let exp = params.get(i).copied();
                    let h = self.expr(a, exp, cx);
                    if let Some(t) = exp {
                        self.expect_ty(&h, t);
                    }
                    let h = if sinks.get(i).copied().unwrap_or(false) {
                        if let HExprKind::Local(id) = h.kind {
                            if borrowed.contains(&id) {
                                let n = cx.locals[id].name.clone();
                                self.diags
                                    .error("E0102", h.span, format!("`{n}` is borrowed by an earlier argument of this call, so it cannot be moved here"))
                                    .note("pass a copy with `.clone()`, or move it in a separate statement");
                            }
                        }
                        self.consume(h, cx)
                    } else {
                        if let HExprKind::Local(id) = h.kind {
                            if !h.ty.is_copy() {
                                borrowed.push(id);
                            }
                        }
                        h
                    };
                    hargs.push(h);
                }
                HExpr { kind: HExprKind::Call { func: fi, args: hargs }, ty: ret, span, at: callee.span }
            }
            ExprKind::Field { base, name } => self.method_call(base, name, args, span, callee.span, cx),
            _ => {
                self.diags.error("U0001", callee.span, "only calls to named functions are supported in P0");
                err_expr(span)
            }
        }
    }

    fn method_call(&mut self, base: &Expr, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        if let ExprKind::Ident(tn) = &base.kind {
            if self.lookup(cx, tn).is_none() {
                if name == "wrap" {
                    if let Some(to) = IntTy::from_name(tn) {
                        return self.conversion(to, false, args, at, span, cx);
                    }
                }
                self.diags.error("U0001", at, format!("`{tn}.{name}(...)` is not supported in P0"));
                return err_expr(span);
            }
        }
        if name == "push" {
            return self.push_call(base, args, span, at, cx);
        }
        let recv = self.expr(base, None, cx);
        let (method, ty) = match (recv.ty, name) {
            (Ty::Str, "toString") => (Builtin::StrToString, Ty::String),
            (Ty::Str, "len") => (Builtin::StrLen, Ty::Int(IntTy::I64)),
            (Ty::String, "len") => (Builtin::StringLen, Ty::Int(IntTy::I64)),
            (Ty::String, "clone") => (Builtin::StringClone, Ty::String),
            (Ty::Vec(_), "len") => (Builtin::VecLen, Ty::Int(IntTy::I64)),
            (Ty::Vec(_), "clone") => (Builtin::VecClone, recv.ty),
            (Ty::Error, _) => return err_expr(span),
            (t, m) => {
                self.diags.error("U0001", at, format!("method `{m}` on `{t}` is not supported in P0"));
                for a in args {
                    self.expr(a, None, cx);
                }
                return err_expr(span);
            }
        };
        if !args.is_empty() {
            self.diags.error("E0012", span, format!("`{name}` takes no arguments"));
            return err_expr(span);
        }
        HExpr { kind: HExprKind::Method { method, recv: Box::new(recv) }, ty, span, at }
    }

    /// `v.push(x)` moves `x` into the vector; `s.push(t)` appends text to a `String`.
    fn push_call(&mut self, base: &Expr, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let ExprKind::Ident(name) = &base.kind else {
            self.diags.error("U0001", base.span, "`push` is only supported on a variable in P0");
            return err_expr(span);
        };
        let Some(id) = self.lookup(cx, name) else {
            self.diags.error("E0010", base.span, format!("cannot find variable `{name}` in this scope"));
            return err_expr(span);
        };
        cx.locals[id].used = true;
        self.check_moved(cx, id, base.span);
        match cx.locals[id].kind {
            LocalKind::Var => {}
            LocalKind::Let => {
                self.diags
                    .error("E0014", base.span, format!("cannot modify `{name}` because it was declared with `let`"))
                    .note(format!("declare it with `var {name}` to make it mutable"));
            }
            LocalKind::Param | LocalKind::SinkParam | LocalKind::LoopVar => {
                self.diags.error("E0014", base.span, format!("cannot modify `{name}`: parameters and loop variables are read-only"));
            }
        }
        let [arg] = args else {
            self.diags.error("E0012", span, "`push` takes exactly one argument");
            return err_expr(span);
        };
        // v1 has no two-phase borrows: the receiver is borrowed mutably before the argument runs.
        if mentions(arg, name) {
            self.diags
                .error("E0403", arg.span, format!("`{name}` is already borrowed by `{name}.push(...)`, so the argument cannot use it"))
                .note(format!("compute the value first: `let x = ...` then `{name}.push(x)`"));
            return err_expr(span);
        }
        let rty = cx.locals[id].ty;
        let value = match rty {
            Ty::Vec(vid) => {
                let et = vec_elem(vid);
                let h = self.expr(arg, Some(et), cx);
                self.expect_ty(&h, et);
                self.consume(h, cx)
            }
            Ty::String => {
                let h = self.expr(arg, None, cx);
                if !matches!(h.ty, Ty::Str | Ty::String | Ty::Error) {
                    self.diags.error("E0001", h.span, format!("expected `str` or `String`, found `{}`", h.ty));
                }
                h
            }
            Ty::Error => return err_expr(span),
            other => {
                self.diags.error("U0001", at, format!("method `push` on `{other}` is not supported in P0"));
                return err_expr(span);
            }
        };
        HExpr { kind: HExprKind::Push { local: id, value: Box::new(value) }, ty: Ty::Unit, span, at }
    }

    fn vec_lit(&mut self, items: &[Expr], expected: Option<Ty>, span: Span, cx: &mut FnCx) -> HExpr {
        let mut elem = match expected {
            Some(Ty::Vec(id)) => Some(vec_elem(id)),
            _ => None,
        };
        let mut out = Vec::new();
        for it in items {
            let h = self.expr(it, elem, cx);
            match elem {
                Some(e) => self.expect_ty(&h, e),
                None if h.ty == Ty::Unit => {
                    self.diags.error("E0001", h.span, "this call returns nothing, so it cannot be a vector element");
                }
                None if h.ty != Ty::Error => elem = Some(h.ty),
                None => {}
            }
            out.push(self.consume(h, cx));
        }
        let Some(elem) = elem else {
            if items.is_empty() {
                self.diags
                    .error("E0001", span, "cannot infer the element type of `[]`")
                    .note("annotate it, e.g. `let v: Vec[int] = []`");
            }
            return err_expr(span);
        };
        HExpr { kind: HExprKind::VecLit(out), ty: Ty::Vec(vec_of(elem)), span, at: span }
    }

    fn index(&mut self, base: &Expr, index: &Expr, span: Span, cx: &mut FnCx) -> HExpr {
        if let ExprKind::Ident(n) = &base.kind {
            if self.lookup(cx, n).is_none()
                && (is_type_name(n) || UNSUPPORTED_TYPES.contains(&n.as_str()) || self.struct_index.contains_key(n))
            {
                self.diags.error("U0001", span, "type arguments in expressions (`Name[T]`) are not supported in P0");
                return err_expr(span);
            }
        }
        let b = self.expr(base, None, cx);
        let i = self.expr(index, None, cx);
        let elem = match (b.ty, i.ty) {
            (Ty::Error, _) | (_, Ty::Error) => return err_expr(span),
            (Ty::Vec(id), Ty::Int(_)) => vec_elem(id),
            (Ty::Str | Ty::String, Ty::Int(_)) => Ty::Int(IntTy::U8),
            (Ty::Vec(_) | Ty::Str | Ty::String, other) => {
                self.diags.error("E0001", i.span, format!("an index must be an integer, found `{other}`"));
                return err_expr(span);
            }
            (other, _) => {
                self.diags.error("E0001", b.span, format!("a value of type `{other}` cannot be indexed"));
                return err_expr(span);
            }
        };
        if !elem.is_copy() {
            self.diags.error(
                "U0001",
                span,
                format!("reading an element of type `{elem}` with `v[i]` needs borrows, which are not supported in P0 yet"),
            );
            return err_expr(span);
        }
        HExpr { kind: HExprKind::Index { base: Box::new(b), index: Box::new(i) }, ty: elem, span, at: span }
    }

    fn conversion(&mut self, to: IntTy, checked: bool, args: &[Expr], at: Span, span: Span, cx: &mut FnCx) -> HExpr {
        let [arg] = args else {
            let form = if checked { format!("{to}(x)") } else { format!("{to}.wrap(x)") };
            self.diags.error("E0012", span, format!("`{form}` takes exactly one argument"));
            return err_expr(span);
        };
        let h = self.expr(arg, None, cx);
        match h.ty {
            Ty::Int(from) if checked && from == to => h,
            Ty::Int(_) => HExpr { kind: HExprKind::Convert { to, checked, expr: Box::new(h) }, ty: Ty::Int(to), span, at },
            Ty::Error => err_expr(span),
            other => {
                let d = self.diags.error("E0001", arg.span, format!("cannot convert `{other}` to `{to}`"));
                if other == Ty::Bool {
                    d.note("use `if b { 1 } else { 0 }` logic to turn a bool into a number");
                }
                err_expr(span)
            }
        }
    }
}
