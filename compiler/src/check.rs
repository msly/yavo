//! Name resolution and type checking. Produces a typed IR (`H*`) for code generation.

use std::collections::HashMap;

use crate::ast::*;
use crate::diag::{Diags, Span};
use crate::types::{IntTy, Ty};

pub type LocalId = usize;

pub struct HProgram {
    pub fns: Vec<HFn>,
}

pub struct HFn {
    pub name: String,
    pub params: Vec<LocalId>,
    pub ret: Ty,
    pub locals: Vec<HLocal>,
    pub body: Vec<HStmt>,
}

pub struct HLocal {
    pub name: String,
    pub ty: Ty,
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
    Assign { local: LocalId, value: HExpr },
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
    Error,
}

const OUTPUT_FNS: [&str; 3] = ["println", "print", "eprintln"];
const UNSUPPORTED_TYPES: &[&str] = &[
    "f32", "f64", "String", "Vec", "Map", "Set", "Box", "Rc", "Arc", "Arena", "Id", "Mutex", "Atomic", "Channel",
];
const UNSUPPORTED_FNS: &[&str] = &[
    "panic", "assert", "format", "len", "min", "max", "abs", "some", "take", "replace", "swap", "Box", "Rc", "Arc",
    "Mutex",
];

struct Sig {
    params: Vec<Ty>,
    ret: Ty,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LocalKind {
    Param,
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
}

struct Checker<'d> {
    diags: &'d mut Diags,
    sigs: Vec<Sig>,
    index: HashMap<String, usize>,
}

pub fn check(prog: &Program, diags: &mut Diags) -> Option<HProgram> {
    let mut ck = Checker { diags, sigs: Vec::new(), index: HashMap::new() };
    ck.collect(prog);
    let fns = prog.fns.iter().enumerate().map(|(i, f)| ck.function(f, i)).collect();
    if ck.diags.has_errors() {
        None
    } else {
        Some(HProgram { fns })
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
    IntTy::from_name(name).is_some() || name == "bool" || name == "str"
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
    fn collect(&mut self, prog: &Program) {
        for f in &prog.fns {
            let params = f.params.iter().map(|p| self.resolve_ty(&p.ty)).collect();
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
            self.sigs.push(Sig { params, ret });
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
        if let Some(it) = IntTy::from_name(&t.name) {
            return Ty::Int(it);
        }
        match t.name.as_str() {
            "bool" => Ty::Bool,
            "str" => Ty::Str,
            name if UNSUPPORTED_TYPES.contains(&name) => {
                self.diags.error(
                    "U0001",
                    t.span,
                    format!("type `{name}` is not supported in P0 (available: int, i8-i64, u8-u64, byte, bool, str)"),
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
        let mut cx = FnCx { locals: Vec::new(), scopes: vec![Vec::new()], ret, loop_depth: 0 };
        let params = f
            .params
            .iter()
            .zip(param_tys)
            .map(|(p, ty)| self.declare(&mut cx, &p.name, p.span, ty, LocalKind::Param))
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
            ret,
            locals: cx.locals.into_iter().map(|l| HLocal { name: l.name, ty: l.ty }).collect(),
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
                    HStmtKind::Discard(init)
                } else {
                    let kind = if *mutable { LocalKind::Var } else { LocalKind::Let };
                    let local = self.declare(cx, name, *name_span, ty, kind);
                    HStmtKind::Let { local, init }
                }
            }
            StmtKind::Assign { target, op, op_span, value } => {
                let ExprKind::Ident(name) = &target.kind else {
                    self.diags.error("U0001", target.span, "only plain variables can be assigned in P0");
                    return None;
                };
                let Some(id) = self.lookup(cx, name) else {
                    self.diags.error("E0010", target.span, format!("cannot find variable `{name}` in this scope"));
                    self.expr(value, None, cx);
                    return None;
                };
                let lty = cx.locals[id].ty;
                match cx.locals[id].kind {
                    LocalKind::Var => {}
                    LocalKind::Let => {
                        self.diags
                            .error("E0014", target.span, format!("cannot assign to `{name}` because it was declared with `let`"))
                            .note(format!("declare it with `var {name}` to make it mutable"));
                    }
                    LocalKind::Param => {
                        self.diags
                            .error("E0014", target.span, format!("cannot assign to parameter `{name}`; parameters are read-only"))
                            .note(format!("copy it into a local first: `var {name}2 = {name}`"));
                    }
                    LocalKind::LoopVar => {
                        self.diags.error("E0014", target.span, format!("cannot assign to loop variable `{name}`"));
                    }
                }
                let value = match op {
                    None => {
                        let v = self.expr(value, Some(lty), cx);
                        self.expect_ty(&v, lty);
                        v
                    }
                    Some(op) => {
                        cx.locals[id].used = true;
                        let lhs = HExpr { kind: HExprKind::Local(id), ty: lty, span: target.span, at: target.span };
                        let rhs = self.expr(value, Some(lty), cx);
                        self.arith(*op, lhs, rhs, *op_span, s.span)
                    }
                };
                HStmtKind::Assign { local: id, value }
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
                let then = self.block(then, cx);
                let els = match els {
                    None => Vec::new(),
                    Some(Else::Block(b)) => self.block(b, cx),
                    Some(Else::If(s)) => self.stmt(s, cx).into_iter().collect(),
                };
                HStmtKind::If { cond, then, els }
            }
            StmtKind::While { cond, body } => {
                let cond = self.cond(cond, cx);
                cx.loop_depth += 1;
                let body = self.block(body, cx);
                cx.loop_depth -= 1;
                HStmtKind::While { cond, body }
            }
            StmtKind::Loop { body } => {
                cx.loop_depth += 1;
                let body = self.block(body, cx);
                cx.loop_depth -= 1;
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
                cx.loop_depth += 1;
                let body = self.block(body, cx);
                cx.loop_depth -= 1;
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
                        HStmtKind::Return(Some(h))
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
                    return HExpr { kind: HExprKind::Local(id), ty: cx.locals[id].ty, span, at: span };
                }
                if self.index.contains_key(name) {
                    self.diags.error(
                        "U0001",
                        span,
                        format!("functions cannot be used as values in P0; call it with `{name}(...)`"),
                    );
                } else if is_type_name(name) {
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
                self.diags.error(
                    "U0001",
                    span,
                    "fields and methods are not supported in P0 (only `T.min`, `T.max` and `T.wrap(x)` on integer types)",
                );
                err_expr(span)
            }
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
                    } else if (ordering && !matches!(l.ty, Ty::Int(_)))
                        || !matches!(l.ty, Ty::Int(_) | Ty::Bool | Ty::Str)
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
                let hargs = args
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let exp = params.get(i).copied();
                        let h = self.expr(a, exp, cx);
                        if let Some(t) = exp {
                            self.expect_ty(&h, t);
                        }
                        h
                    })
                    .collect();
                HExpr { kind: HExprKind::Call { func: fi, args: hargs }, ty: ret, span, at: callee.span }
            }
            ExprKind::Field { base, name } if name == "wrap" => {
                if let ExprKind::Ident(tn) = &base.kind {
                    if self.lookup(cx, tn).is_none() {
                        if let Some(to) = IntTy::from_name(tn) {
                            return self.conversion(to, false, args, callee.span, span, cx);
                        }
                    }
                }
                self.diags.error("U0001", callee.span, "methods are not supported in P0 (only `T.wrap(x)` on integer types)");
                err_expr(span)
            }
            _ => {
                self.diags.error("U0001", callee.span, "only calls to named functions are supported in P0");
                err_expr(span)
            }
        }
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
