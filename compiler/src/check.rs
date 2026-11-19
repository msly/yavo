//! Name resolution and tye checking. Produces a typed IR (`H*`) for code generation.

use std::collections::HashMap;

use crate::ast::*;
use crate::diag::{Diags, Span};
use crate::types::{
    box_inner, box_of, finish_agg, map_of, map_parts, opt_inner, opt_of, ref_inner, ref_mut_inner, ref_mut_of, ref_of, register_enum,
    register_struct, res_of, res_parts, set_user_drop, has_user_drop, slice_elem, slice_of, tuple_elems, tuple_of, vec_elem,
    vec_of, IntTy, Ty,
};

pub type LocalId = usize;

/// A step of `HPlace::path` (and of `HStmtKind::Assign`'s path) that goes into the content of
/// a `Box` instead of a field (semantic table A7: fields project through `Box`).
pub const BOX_STEP: usize = usize::MAX;

pub struct HProgram {
    pub fns: Vec<HFn>,
    /// Struct and enum definitions, ordered so that every type comes after the types it
    /// contains by value.
    pub structs: Vec<HStruct>,
    /// Comparison adapters for `sortBy`, one per call site.
    pub tramps: Vec<Tramp>,
}

/// A C comparison function for the runtime's sort that calls closure function `func` with
/// the captured values (passed through a `void *` array) and two elements.
pub struct Tramp {
    pub func: usize,
    pub caps: Vec<Ty>,
    pub elem: Ty,
    /// `func` is a user `cmp(self, other: &T) -> Ordering` (for `sort()` on a `T: Ord`), not a
    /// closure returning "goes before".
    pub by_cmp: bool,
}

pub struct HStruct {
    pub ty: Ty,
    /// Struct fields (empty for an enum).
    pub fields: Vec<(String, Ty)>,
    /// Enum cases with their payload fields (empty for a struct).
    pub variants: Vec<(String, Vec<(String, Ty)>)>,
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
    /// An `inout` parameter: a pointer to the caller's place, used through `*`.
    pub inout: bool,
}

pub struct HStmt {
    pub kind: HStmtKind,
    /// Statement position used for output-failure diagnostics.
    pub span: Span,
}

pub enum HStmtKind {
    Let { local: LocalId, init: HExpr },
    /// `let (a, b) = init`: each element moves (or is copied) into its local; an element named
    /// `_` (`None`) is dropped at once.
    LetTuple { locals: Vec<Option<LocalId>>, init: HExpr },
    /// Evaluate and immediately discard without introducing a name or local slot.
    Discard(HExpr),
    /// `local.path[0].path[1]... = value`; `path` holds field indexes and is empty for a plain variable.
    Assign { local: LocalId, path: Vec<usize>, value: HExpr },
    Expr(HExpr),
    If { cond: HExpr, then: Vec<HStmt>, els: Vec<HStmt> },
    While { cond: HExpr, body: Vec<HStmt> },
    Loop { body: Vec<HStmt> },
    For { local: LocalId, start: HExpr, end: HExpr, body: Vec<HStmt> },
    /// `for [index,] elem in vec`: `elem` holds a copy (copyable elements) or a pointer
    /// (`by_ref`; `&T`, or `&inout T` for `in &v`).
    ForEach { vec: LocalId, index: Option<LocalId>, elem: LocalId, by_ref: bool, body: Vec<HStmt> },
    /// `for key, val in map`, in insertion order; `*_ref` bindings are pointers into the entry.
    ForMap { map: LocalId, key: LocalId, key_ref: bool, val: LocalId, val_ref: bool, body: Vec<HStmt> },
    /// `if let local = scrut` (and `match` on an optional): `then` runs with `local` bound to
    /// the value inside `scrut`, `els` runs when it is `none`.
    IfLet { scrut: HExpr, mode: BindMode, local: LocalId, then: Vec<HStmt>, els: Vec<HStmt> },
    Break,
    Continue,
    Return(Option<HExpr>),
    Print { fd: u8, newline: bool, parts: Vec<PrintPart> },
    Block(Vec<HStmt>),
    /// `match` on enums, results, or nested patterns; `owned`: `scrut` is a temporary whose
    /// parts are moved into the bindings (the rest is dropped when an arm starts).
    Match { scrut: HExpr, owned: bool, arms: Vec<HArm> },
}

/// A checked pattern.
pub enum HPat {
    Wild,
    /// Binds the value here; `by_ref`: the binding is a `&T` into an existing place.
    Bind { local: LocalId, by_ref: bool },
    /// `none`, or `some(p)`.
    Opt(Option<Box<HPat>>),
    /// `ok(p)` or `err(p)`.
    Res { ok: bool, inner: Box<HPat> },
    /// An enum case and one pattern per payload field.
    Case { variant: usize, fields: Vec<HPat> },
}

pub struct HArm {
    pub pat: HPat,
    pub body: Vec<HStmt>,
    /// The value of the arm of a `match` expression (absent when the arm leaves).
    pub value: Option<HExpr>,
}

pub enum CatchHandler {
    /// `catch e => value`; the value may be `return`/`break`/`continue`.
    Value(Box<HExpr>),
    /// `catch e { ... }`, which never falls through.
    Block(Vec<HStmt>),
}

pub enum PrintPart {
    Text(String),
    Value(HExpr),
}

/// How an `if let`/`match` binding gets the value inside an optional (semantic table C5, C7).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BindMode {
    /// The optional is an existing place and its payload is copyable: the binding is a copy.
    Copy,
    /// The optional is an existing place and its payload is not copyable: the binding is a
    /// `&T` into the place.
    Ref,
    /// The optional is an owned temporary: the binding takes its payload.
    Owned,
}

/// A writable place: a variable or a chain of struct fields rooted at one.
pub struct HPlace {
    pub local: LocalId,
    /// Field indexes from the variable to the place; empty for the variable itself.
    pub path: Vec<usize>,
    pub ty: Ty,
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
    /// `(a, b, ...)`: element values in order.
    TupleLit(Vec<HExpr>),
    /// `base[index]` for a copyable element, or a byte of a string.
    Index { base: Box<HExpr>, index: Box<HExpr> },
    Method { method: Builtin, recv: Box<HExpr> },
    /// `local.push(value)`; `value` is moved into a vector or appended to a `String`.
    Push { place: HPlace, value: Box<HExpr> },
    /// A `String` borrowed as a `str` (passing it to a `str` parameter).
    StrView(Box<HExpr>),
    /// Reading through a `&T`.
    Deref(Box<HExpr>),
    /// `&place` as a `&T` value (a hidden reference to a field being iterated).
    AddrOf(Box<HExpr>),
    /// `&local` passed to an `inout` parameter.
    InoutArg(LocalId),
    /// `&place` passed to an `inout` parameter (an `inout self` receiver).
    InoutPlace(HPlace),
    /// `none`, of the optional type in `ty`.
    None,
    /// `some(value)`, written or implied (a `T` used where a `?T` is expected).
    Some(Box<HExpr>),
    /// `lhs ?? rhs`; `rhs` runs only when `lhs` is `none`.
    Coalesce { lhs: Box<HExpr>, rhs: Box<HExpr> },
    /// `return`, `break`, `continue` on the right side of `??`.
    Return(Option<Box<HExpr>>),
    Break,
    Continue,
    /// `take(&place)`: the old value; the place is left with its default (all-zero) value.
    Take(HPlace),
    /// `replace(&place, value)`: the old value; `value` is stored in the place.
    Replace(HPlace, Box<HExpr>),
    /// `swap(&a, &b)`
    Swap(HPlace, HPlace),
    /// `local.pop()`: the last element of a vector, or `none`.
    Pop(HPlace),
    /// `[:]`, `[k: v, ...]`: entries in the order written.
    MapLit(Vec<(HExpr, HExpr)>),
    /// `recv.get(key)`, `recv.getCopy(key)`, `recv.has(key)`.
    MapRead { op: MapRead, recv: Box<HExpr>, key: Box<HExpr> },
    /// `local.set(key, value)`, `local.remove(key)`, `local.getMut(key)`.
    MapWrite { op: MapWrite, place: HPlace, args: Vec<HExpr> },
    /// `os.exit(code)`: flushes output and ends the process without running drops.
    Exit(Box<HExpr>),
    /// `Enum.Case(...)`: field values in the order written (`usize` is the field index).
    EnumLit { variant: usize, fields: Vec<(usize, HExpr)> },
    /// A success value of the result type in `ty` (`None` when the success type is `()`).
    Ok(Option<Box<HExpr>>),
    /// `err(value)`, of the result type in `ty`.
    ErrVal(Box<HExpr>),
    /// `try value`: the success value, or return the error from the current function.
    Try(Box<HExpr>),
    /// `value catch e => ...` / `value catch e { ... }`; `bind` is absent for `catch _`.
    Catch { value: Box<HExpr>, bind: Option<LocalId>, handler: CatchHandler },
    /// A `match` that produces a value.
    Match { scrut: Box<HExpr>, owned: bool, arms: Vec<HArm> },
    /// `panic(message)` / `assert(cond, message)`: `cond` is absent for `panic`.
    Panic { cond: Option<Box<HExpr>>, parts: Vec<PrintPart> },
    /// `format("...")`: a new `String` built from the parts.
    Format(Vec<PrintPart>),
    /// `min(a, b)` / `max(a, b)` on integers.
    MinMax { max: bool, a: Box<HExpr>, b: Box<HExpr> },
    /// `abs(x)` on a signed integer (overflow panics).
    Abs(Box<HExpr>),
    /// A method that takes arguments, or a text method without a borrowing result.
    Call2 { method: Method2, recv: Box<HExpr>, args: Vec<HExpr> },
    /// `place.sort()`.
    Sort(HPlace),
    /// `value.tag == tag` (or `!=` when `eq` is false): a test of an enum's case.
    TagTest { value: Box<HExpr>, tag: u32, eq: bool },
    /// `lhs == rhs` / `lhs != rhs` on an enum whose cases carry no payload.
    SameTag { lhs: Box<HExpr>, rhs: Box<HExpr>, eq: bool },
    /// `place.sortBy(closure)`: `tramp` adapts closure function to the runtime's sort; `caps`
    /// are the captured variables, read in place.
    SortBy { place: HPlace, tramp: usize, caps: Vec<HExpr> },
    /// `place.update(key, initial, closure)`: the value for `key` (or `initial`) becomes
    /// `func(caps..., value)`.
    MapUpdate { place: HPlace, key: Box<HExpr>, init: Box<HExpr>, func: usize, caps: Vec<HExpr> },
    /// `io.readAll()`, `io.readLine()`, `fs.readText(p)`, `fs.writeText(p, s)`.
    Io { op: IoOp, args: Vec<HExpr> },
    /// `os.args()`
    OsArgs,
    /// A `Vec[T]` (or anything holding its elements) viewed as a `[]T`.
    SliceOf(Box<HExpr>),
    /// `String{}`
    EmptyString,
    /// `Box(value)`: a new heap block holding `value`.
    BoxNew(Box<HExpr>),
    /// The content of a `Box`, read in place (fields, methods and indexing project through a
    /// box, A7). Like a field, it cannot be moved out of.
    Unbox(Box<HExpr>),
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method2 {
    Contains,
    StartsWith,
    EndsWith,
    Find,
    /// `s.split(sep)`: views into the receiver.
    Split,
    ToLower,
    ToUpper,
    ParseInt,
    /// `v.contains(x)` on a vector or slice.
    SeqContains,
    /// `v.join(sep)` on a vector or slice of `str` / `String`.
    Join,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IoOp {
    ReadAll,
    ReadLine,
    ReadText,
    WriteText,
}

/// The element type of a `Vec[T]` or `[]T`.
pub fn seq_elem(t: Ty) -> Option<Ty> {
    match t {
        Ty::Vec(v) => Some(vec_elem(v)),
        Ty::Slice(s) => Some(slice_elem(s)),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapRead {
    Get,
    GetCopy,
    Has,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapWrite {
    Set,
    Remove,
    GetMut,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Builtin {
    StrToString,
    StrLen,
    StringLen,
    StringClone,
    VecLen,
    VecClone,
    /// View-producing methods on `str`/`String`: the result points into the receiver's text.
    Trim,
    Fields,
    Lines,
    IsSome,
    IsNone,
    MapLen,
    MapClone,
}

/// Stands for a function that was already reported as unusable here.
const NO_FN: usize = usize::MAX;
const OUTPUT_FNS: [&str; 3] = ["println", "print", "eprintln"];
const UNSUPPORTED_TYPES: &[&str] = &[
    "f32", "f64", "Set", "Rc", "Arc", "Arena", "Id", "Mutex", "Atomic", "Channel", "Guard", "SendError", "Join",
    "Heap", "Deque",
];
/// Standard library modules and the public types each one brings in (stdlib contract §1).
const MODULES: &[(&str, &[&str])] = &[
    ("io", &[]),
    ("fs", &[]),
    ("os", &[]),
    ("math", &[]),
    ("time", &[]),
    ("rand", &[]),
    ("collections", &["Heap", "Deque"]),
    ("sync", &["Mutex", "Guard", "Channel", "SendError", "Join"]),
];
const UNSUPPORTED_FNS: &[&str] = &[
    "len", "Rc", "Arc", "Mutex",
];

struct Sig {
    params: Vec<Ty>,
    sinks: Vec<bool>,
    inouts: Vec<bool>,
    ret: Ty,
    /// Parameters a returned borrow may come from (`of a, b`); `Some(vec![])` for `of static`,
    /// `None` when no `of` was written.
    ret_of: Option<Vec<usize>>,
    /// A method with a receiver: parameter 0 is `self`.
    has_self: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LocalKind {
    Param,
    SinkParam,
    InoutParam,
    Let,
    Var,
    LoopVar,
    /// An `if let`/`match` binding into an existing place that is read-only.
    Bind,
    /// An `if let`/`match` binding into a writable place (writable by the spec, not supported yet).
    BindMut,
    /// An `if let`/`match` binding that owns the payload of a temporary.
    BindOwned,
}

struct LocalState {
    name: String,
    ty: Ty,
    kind: LocalKind,
    used: bool,
    span: Span,
    /// Statement number of the declaration (0 for parameters) and of the last statement of
    /// the block that contains it.
    decl: usize,
    scope_end: usize,
    /// Number of enclosing scopes; a smaller depth outlives a larger one.
    depth: usize,
    /// For a value that carries a borrow: the variables it points into.
    loans: Vec<Loan>,
    /// A key bound by `for k, v in m`: keys are never writable (A5).
    map_key: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Loan {
    /// Points into this variable (an owned `String`/`Vec`, or a parameter).
    Local(LocalId),
    /// Points into a temporary that is dropped at the end of the statement.
    Temp,
}

/// Statement numbering and name uses of one function body, computed before checking it.
/// The checker visits statements in the same order, so `FnCx::cur` matches these numbers.
#[derive(Default)]
struct Pre {
    next: usize,
    occ: Vec<Occ>,
    /// (first, last) statement number of every loop, condition included.
    loops: Vec<(usize, usize)>,
    /// Last statement number of the block containing each `let` (keyed by the `let`'s number).
    let_end: HashMap<usize, usize>,
    /// Last statement number of the block a statement binds a name for: the body of `for x in`,
    /// the `then` block of `if let` and each `match` arm (keyed by statement number and arm).
    bind_end: HashMap<(usize, usize), usize>,
    /// Last statement number inside each arm of a `match` expression and each `catch` handler,
    /// keyed by the expression's start offset (and arm; 0 for `catch`).
    expr_end: HashMap<(usize, usize), usize>,
}

struct Occ {
    idx: usize,
    line: u32,
    name: String,
    loops: Vec<usize>,
}

impl Pre {
    fn build(body: &Block) -> Pre {
        let mut p = Pre::default();
        p.block(&body.stmts, &mut Vec::new());
        p
    }

    fn block(&mut self, stmts: &[Stmt], stack: &mut Vec<usize>) {
        let mut lets = Vec::new();
        for s in stmts {
            if matches!(s.kind, StmtKind::Let { .. }) {
                lets.push(self.next + 1);
            }
            self.stmt(s, stack);
        }
        for l in lets {
            self.let_end.insert(l, self.next);
        }
    }

    fn stmt(&mut self, s: &Stmt, stack: &mut Vec<usize>) {
        self.next += 1;
        let idx = self.next;
        match &s.kind {
            StmtKind::Let { init, .. } => self.expr(init, idx, stack),
            StmtKind::LetTuple { init, .. } => self.expr(init, idx, stack),
            StmtKind::Assign { target, value, .. } => {
                self.expr(target, idx, stack);
                self.expr(value, idx, stack);
            }
            StmtKind::Expr(e) | StmtKind::Return(Some(e)) => self.expr(e, idx, stack),
            StmtKind::If { cond, then, els } => {
                self.expr(cond, idx, stack);
                self.block(&then.stmts, stack);
                match els {
                    Some(Else::Block(b)) => self.block(&b.stmts, stack),
                    Some(Else::If(s2)) => self.stmt(s2, stack),
                    None => {}
                }
            }
            StmtKind::IfLet { value, then, els, .. } => {
                self.expr(value, idx, stack);
                self.block(&then.stmts, stack);
                self.bind_end.insert((idx, 0), self.next);
                match els {
                    Some(Else::Block(b)) => self.block(&b.stmts, stack),
                    Some(Else::If(s2)) => self.stmt(s2, stack),
                    None => {}
                }
            }
            StmtKind::Match { value, arms } => {
                self.expr(value, idx, stack);
                for (i, arm) in arms.iter().enumerate() {
                    self.block(&arm.body.stmts, stack);
                    self.bind_end.insert((idx, i), self.next);
                }
            }
            StmtKind::While { cond, body } => {
                let l = self.open_loop(idx, stack);
                self.expr(cond, idx, stack);
                self.block(&body.stmts, stack);
                self.close_loop(l, stack);
            }
            StmtKind::Loop { body } => {
                let l = self.open_loop(idx, stack);
                self.block(&body.stmts, stack);
                self.close_loop(l, stack);
            }
            StmtKind::For { start, end, body, .. } => {
                self.expr(start, idx, stack);
                self.expr(end, idx, stack);
                let l = self.open_loop(idx, stack);
                self.block(&body.stmts, stack);
                self.close_loop(l, stack);
            }
            StmtKind::ForIn { iter, body, .. } => {
                self.expr(iter, idx, stack);
                let l = self.open_loop(idx, stack);
                self.block(&body.stmts, stack);
                self.bind_end.insert((idx, 0), self.next);
                self.close_loop(l, stack);
            }
            StmtKind::Block(b) => self.block(&b.stmts, stack),
            StmtKind::Break | StmtKind::Continue | StmtKind::Return(None) => {}
        }
    }

    fn open_loop(&mut self, idx: usize, stack: &mut Vec<usize>) -> usize {
        self.loops.push((idx, idx));
        stack.push(self.loops.len() - 1);
        self.loops.len() - 1
    }

    fn close_loop(&mut self, l: usize, stack: &mut Vec<usize>) {
        stack.pop();
        self.loops[l].1 = self.next;
    }

    fn expr(&mut self, e: &Expr, idx: usize, stack: &[usize]) {
        match &e.kind {
            ExprKind::Ident(n) => {
                self.occ.push(Occ { idx, line: e.span.line, name: n.clone(), loops: stack.to_vec() })
            }
            ExprKind::Int(_) | ExprKind::Byte(_) | ExprKind::Bool(_) | ExprKind::None | ExprKind::Break | ExprKind::Continue | ExprKind::Unit => {}
            ExprKind::Return(v) => {
                if let Some(v) = v {
                    self.expr(v, idx, stack);
                }
            }
            ExprKind::Case { args, .. } => {
                for a in args.iter().flatten() {
                    self.expr(a, idx, stack);
                }
            }
            ExprKind::Named { value, .. } | ExprKind::Err(value) | ExprKind::Try(value) => self.expr(value, idx, stack),
            ExprKind::Catch { value, handler, .. } => {
                self.expr(value, idx, stack);
                self.expr(handler, idx, stack);
                self.expr_end.insert((e.span.start, 0), self.next);
            }
            ExprKind::Match { value, arms } => {
                self.expr(value, idx, stack);
                for (i, arm) in arms.iter().enumerate() {
                    self.expr(&arm.body, idx, stack);
                    self.expr_end.insert((e.span.start, i), self.next);
                }
            }
            ExprKind::Block(b) => {
                let mut st = stack.to_vec();
                self.block(&b.stmts, &mut st);
            }
            ExprKind::Str(segs) => {
                for s in segs {
                    if let StrSeg::Expr { expr, .. } = s {
                        self.expr(expr, idx, stack);
                    }
                }
            }
            ExprKind::Call { callee, args } => {
                self.expr(callee, idx, stack);
                for a in args {
                    self.expr(a, idx, stack);
                }
            }
            ExprKind::Field { base, .. } => self.expr(base, idx, stack),
            ExprKind::StructLit { fields, .. } => {
                for f in fields {
                    self.expr(&f.value, idx, stack);
                }
            }
            ExprKind::VecLit(items) => {
                for i in items {
                    self.expr(i, idx, stack);
                }
            }
            ExprKind::Tuple(items) => {
                for i in items {
                    self.expr(i, idx, stack);
                }
            }
            // A closure body is checked as a function of its own (with its own numbering); here
            // only its uses of outer variables count, at the enclosing statement.
            ExprKind::Closure { body, .. } => {
                let mut names = Vec::new();
                idents_expr(body, &mut names);
                for (name, line) in names {
                    self.occ.push(Occ { idx, line, name, loops: stack.to_vec() });
                }
            }
            ExprKind::MapLit(entries) => {
                for (k, v) in entries {
                    self.expr(k, idx, stack);
                    self.expr(v, idx, stack);
                }
            }
            ExprKind::Index { base, index } => {
                self.expr(base, idx, stack);
                self.expr(index, idx, stack);
            }
            ExprKind::Unary { expr, .. } | ExprKind::Paren(expr) => self.expr(expr, idx, stack),
            ExprKind::Borrow(inner) => self.expr(inner, idx, stack),
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::Coalesce { lhs, rhs, .. } => {
                self.expr(lhs, idx, stack);
                self.expr(rhs, idx, stack);
            }
        }
    }

    /// If the variable `v` is used again at or after statement `p` (a use inside a loop that
    /// started after `v` was declared counts until the end of that loop), the line of that use.
    fn live_after(&self, v: &LocalState, p: usize) -> Option<u32> {
        self.occ.iter().filter(|o| o.name == v.name && o.idx > v.decl && o.idx <= v.scope_end).find_map(|o| {
            let eff = o
                .loops
                .iter()
                .map(|&l| self.loops[l])
                .filter(|&(start, _)| start > v.decl)
                .map(|(_, end)| end)
                .fold(o.idx, usize::max);
            (eff >= p).then_some(o.line)
        })
    }
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
    /// Statement numbering and name uses of this function; `cur` is the current statement.
    pre: Pre,
    cur: usize,
    /// Names listed after `of` in the signature (`static` included), if any.
    ret_of: Option<Vec<String>>,
    /// Vectors being iterated by enclosing `for` loops; they must not change meanwhile.
    iterating: Vec<LocalId>,
    /// Collections iterated by enclosing `for ... in &c` loops: the loop has exclusive access.
    iterating_mut: Vec<LocalId>,
    /// The expression (by span) that may be `x ?? return/break/continue`: the whole initializer
    /// of a `let` or right side of an assignment, or the right side of such a `??`.
    diverge_ok: Option<Span>,
    /// Checking the body of a `drop` method of a `T: Drop` type (V11).
    in_drop: bool,
    /// In a closure function: the parameters that stand for captured outer variables.
    captures: Vec<LocalId>,
}

type Moved = Vec<Option<Span>>;

impl FnCx {
    fn new(ret: Ty, pre: Pre) -> FnCx {
        FnCx {
            locals: Vec::new(),
            scopes: vec![Vec::new()],
            ret,
            loop_depth: 0,
            moved: Vec::new(),
            loops: Vec::new(),
            pre,
            cur: 0,
            ret_of: None,
            iterating: Vec::new(),
            iterating_mut: Vec::new(),
            diverge_ok: None,
            in_drop: false,
            captures: Vec::new(),
        }
    }
}

fn hlocals(locals: Vec<LocalState>) -> Vec<HLocal> {
    locals
        .into_iter()
        .map(|l| HLocal {
            owned: l.ty.needs_drop() && !matches!(l.kind, LocalKind::Param | LocalKind::InoutParam),
            inout: l.kind == LocalKind::InoutParam,
            name: l.name,
            ty: l.ty,
        })
        .collect()
}

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

/// One arm of an `if let` or `match` on an optional.
struct OptArm<'a> {
    pat: ArmPat<'a>,
    body: Body<'a>,
    span: Span,
}

enum ArmPat<'a> {
    Some(&'a str, Span),
    None,
    Wild,
}

enum Body<'a> {
    Block(&'a Block),
    /// `else if ...`
    Stmt(&'a Stmt),
    /// An `if let` without `else`.
    Empty,
}

/// One arm of a general `match`: a statement arm's block, or an expression arm's value.
struct GArm<'a> {
    pat: &'a Pattern,
    span: Span,
    body: GBody<'a>,
}

enum GBody<'a> {
    Block(&'a Block),
    Expr(&'a Expr),
}

/// How the bindings of one `match` arm are made.
struct PatCx<'a> {
    /// The scrutinee is an owned temporary: bindings take their parts.
    owned: bool,
    kind: LocalKind,
    /// What a binding into the scrutinee points into.
    loans: &'a [Loan],
    /// Last statement of the arm, if it has statements.
    scope_end: Option<usize>,
    /// The arm belongs to a `match` expression.
    in_expr: bool,
    /// The arm's pattern, for diagnostics.
    span: Span,
}

static WILD: HPat = HPat::Wild;

/// The constructor a pattern tests for (see `Checker::ctors_of`) and its sub-patterns; `None`
/// for a pattern that matches anything.
fn pat_ctor(p: &HPat) -> Option<(usize, Vec<&HPat>)> {
    match p {
        HPat::Wild | HPat::Bind { .. } => None,
        HPat::Opt(None) => Some((0, Vec::new())),
        HPat::Opt(Some(q)) => Some((1, vec![&**q])),
        HPat::Res { ok, inner } => Some((if *ok { 0 } else { 1 }, vec![&**inner])),
        HPat::Case { variant, fields } => Some((*variant, fields.iter().collect())),
    }
}

/// Whether the arms of a `match` are only `some(name)`, `none` and `_`: the optional-only form
/// handled by `match_opt` (which also serves `if let`).
fn simple_opt_arms<'a>(pats: impl Iterator<Item = &'a Pattern> + Clone) -> bool {
    pats.clone().any(|p| !matches!(p, Pattern::Wild))
        && pats.into_iter().all(|p| match p {
            Pattern::Some(inner) => matches!(**inner, Pattern::Bind { .. } | Pattern::Wild),
            Pattern::None | Pattern::Wild => true,
            _ => false,
        })
}

/// The variable an existing place belongs to: `x`, `x.a.b`, or a place read through a `&`.
fn place_root(h: &HExpr) -> Option<LocalId> {
    match &h.kind {
        HExprKind::Local(id) => Some(*id),
        HExprKind::Field { base, .. } | HExprKind::Deref(base) | HExprKind::Unbox(base) => place_root(base),
        _ => None,
    }
}

/// Whether `e` mentions the variable `name` anywhere.
fn mentions(e: &Expr, name: &str) -> bool {
    match &e.kind {
        ExprKind::Ident(n) => n == name,
        ExprKind::Int(_) | ExprKind::Byte(_) | ExprKind::Bool(_) | ExprKind::None | ExprKind::Break | ExprKind::Continue | ExprKind::Unit => false,
        ExprKind::Return(v) => v.as_ref().is_some_and(|v| mentions(v, name)),
        ExprKind::Case { args, .. } => args.iter().flatten().any(|a| mentions(a, name)),
        ExprKind::Named { value, .. } | ExprKind::Err(value) | ExprKind::Try(value) => mentions(value, name),
        ExprKind::Catch { value, handler, .. } => mentions(value, name) || mentions(handler, name),
        ExprKind::Match { value, arms } => mentions(value, name) || arms.iter().any(|a| mentions(&a.body, name)),
        // Statements inside a block are not searched: only the call's own arguments matter here.
        ExprKind::Block(_) => false,
        ExprKind::Str(segs) => segs.iter().any(|s| matches!(s, StrSeg::Expr { expr, .. } if mentions(expr, name))),
        ExprKind::Call { callee, args } => mentions(callee, name) || args.iter().any(|a| mentions(a, name)),
        ExprKind::Field { base, .. } => mentions(base, name),
        ExprKind::StructLit { fields, .. } => fields.iter().any(|f| mentions(&f.value, name)),
        ExprKind::VecLit(items) => items.iter().any(|i| mentions(i, name)),
        ExprKind::Tuple(items) => items.iter().any(|i| mentions(i, name)),
        ExprKind::Closure { body, .. } => {
            let mut names = Vec::new();
            idents_expr(body, &mut names);
            names.iter().any(|(n, _)| n == name)
        }
        ExprKind::MapLit(entries) => entries.iter().any(|(k, v)| mentions(k, name) || mentions(v, name)),
        ExprKind::Index { base, index } => mentions(base, name) || mentions(index, name),
        ExprKind::Unary { expr, .. } | ExprKind::Paren(expr) | ExprKind::Borrow(expr) => mentions(expr, name),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Coalesce { lhs, rhs, .. } => {
            mentions(lhs, name) || mentions(rhs, name)
        }
    }
}

/// Every name used in `e` (statements of nested blocks included), with its line.
fn idents_expr(e: &Expr, out: &mut Vec<(String, u32)>) {
    let sub = |x: &Expr, out: &mut Vec<(String, u32)>| idents_expr(x, out);
    match &e.kind {
        ExprKind::Ident(n) => out.push((n.clone(), e.span.line)),
        ExprKind::Int(_) | ExprKind::Byte(_) | ExprKind::Bool(_) | ExprKind::None | ExprKind::Break | ExprKind::Continue | ExprKind::Unit => {}
        ExprKind::Return(v) => {
            if let Some(v) = v {
                sub(v, out);
            }
        }
        ExprKind::Case { args, .. } => args.iter().flatten().for_each(|a| sub(a, out)),
        ExprKind::Named { value, .. } | ExprKind::Err(value) | ExprKind::Try(value) => sub(value, out),
        ExprKind::Catch { value, handler, .. } => {
            sub(value, out);
            sub(handler, out);
        }
        ExprKind::Match { value, arms } => {
            sub(value, out);
            arms.iter().for_each(|a| sub(&a.body, out));
        }
        ExprKind::Block(b) => idents_block(&b.stmts, out),
        ExprKind::Str(segs) => {
            for s in segs {
                if let StrSeg::Expr { expr, .. } = s {
                    sub(expr, out);
                }
            }
        }
        ExprKind::Call { callee, args } => {
            sub(callee, out);
            args.iter().for_each(|a| sub(a, out));
        }
        ExprKind::Field { base, .. } => sub(base, out),
        ExprKind::StructLit { fields, .. } => fields.iter().for_each(|f| sub(&f.value, out)),
        ExprKind::VecLit(items) | ExprKind::Tuple(items) => items.iter().for_each(|i| sub(i, out)),
        ExprKind::MapLit(entries) => entries.iter().for_each(|(k, v)| {
            sub(k, out);
            sub(v, out);
        }),
        ExprKind::Index { base, index } => {
            sub(base, out);
            sub(index, out);
        }
        ExprKind::Unary { expr, .. } | ExprKind::Paren(expr) | ExprKind::Borrow(expr) => sub(expr, out),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Coalesce { lhs, rhs, .. } => {
            sub(lhs, out);
            sub(rhs, out);
        }
        ExprKind::Closure { body, .. } => sub(body, out),
    }
}

fn idents_block(stmts: &[Stmt], out: &mut Vec<(String, u32)>) {
    for s in stmts {
        match &s.kind {
            StmtKind::Let { init, .. } | StmtKind::LetTuple { init, .. } => idents_expr(init, out),
            StmtKind::Assign { target, value, .. } => {
                idents_expr(target, out);
                idents_expr(value, out);
            }
            StmtKind::Expr(e) | StmtKind::Return(Some(e)) => idents_expr(e, out),
            StmtKind::If { cond: e, then, els } | StmtKind::IfLet { value: e, then, els, .. } => {
                idents_expr(e, out);
                idents_block(&then.stmts, out);
                let mut els = els.as_ref();
                while let Some(x) = els {
                    match x {
                        Else::Block(b) => {
                            idents_block(&b.stmts, out);
                            els = None;
                        }
                        Else::If(s2) => {
                            idents_block(std::slice::from_ref(s2.as_ref()), out);
                            els = None;
                        }
                    }
                }
            }
            StmtKind::Match { value, arms } => {
                idents_expr(value, out);
                arms.iter().for_each(|a| idents_block(&a.body.stmts, out));
            }
            StmtKind::While { cond, body } => {
                idents_expr(cond, out);
                idents_block(&body.stmts, out);
            }
            StmtKind::Loop { body } | StmtKind::Block(body) => idents_block(&body.stmts, out),
            StmtKind::For { start, end, body, .. } => {
                idents_expr(start, out);
                idents_expr(end, out);
                idents_block(&body.stmts, out);
            }
            StmtKind::ForIn { iter, body, .. } => {
                idents_expr(iter, out);
                idents_block(&body.stmts, out);
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Return(None) => {}
        }
    }
}

struct Checker<'d> {
    diags: &'d mut Diags,
    sigs: Vec<Sig>,
    index: HashMap<String, usize>,
    structs: Vec<StructDef>,
    struct_index: HashMap<String, usize>,
    /// Imported modules: where they were imported and whether anything used them.
    imports: HashMap<String, (Span, bool)>,
    /// User comparison protocols by struct/enum id (K2): the `cmp` function of a `T: Ord`, the
    /// `eq` function of a `T: Eq`.
    protocols: HashMap<u32, (Option<usize>, Option<usize>)>,
    /// Number of user functions; closure functions are numbered after them.
    n_fns: usize,
    /// Functions made from closures (O5), in order.
    lifted: Vec<HFn>,
    tramps: Vec<Tramp>,
}

struct StructDef {
    name: String,
    span: Span,
    ty: Ty,
    fields: Vec<(String, Ty, Span)>,
    /// For an enum: its cases and their payload fields.
    variants: Vec<VariantDef>,
}

struct VariantDef {
    name: String,
    fields: Vec<(String, Ty, Span)>,
}

/// Enums that standard library modules provide (stdlib contract §5).
const LIBRARY_ENUMS: &[(&str, &[(&str, &[&str])])] = &[
    ("io.Error", &[("Utf8", &["offset"]), ("Io", &["code"])]),
    ("fs.Error", &[("Utf8", &["offset"]), ("Io", &["code"]), ("InvalidPath", &[])]),
    // Prelude (K2): the result of `cmp`. Tags: Less 0, Equal 1, Greater 2.
    ("Ordering", &[("Less", &[]), ("Equal", &[]), ("Greater", &[])]),
];

pub fn check(prog: &Program, diags: &mut Diags) -> Option<HProgram> {
    let mut ck = Checker {
        diags,
        sigs: Vec::new(),
        index: HashMap::new(),
        structs: Vec::new(),
        struct_index: HashMap::new(),
        imports: HashMap::new(),
        protocols: HashMap::new(),
        n_fns: prog.fns.len(),
        lifted: Vec::new(),
        tramps: Vec::new(),
    };
    ck.collect_uses(prog);
    for (owner, impls) in prog.structs.iter().map(|s| (&s.name, &s.impls)).chain(prog.enums.iter().map(|e| (&e.name, &e.impls))) {
        for (i, sp) in impls {
            if matches!(i.as_str(), "Drop" | "Eq" | "Ord") {
                continue;
            }
            ck.diags.error("U0001", *sp, format!("implementing interfaces (`{owner}: {i}`) is not supported yet"));
        }
    }
    let order = ck.collect_structs(prog);
    ck.collect(prog);
    ck.check_drops(prog);
    ck.check_protocols(prog);
    let mut fns: Vec<HFn> = prog.fns.iter().enumerate().map(|(i, f)| ck.function(f, i)).collect();
    fns.append(&mut ck.lifted);
    let mut unused: Vec<(Span, String)> =
        ck.imports.iter().filter(|(_, (_, used))| !used).map(|(m, (sp, _))| (*sp, m.clone())).collect();
    unused.sort_by_key(|(sp, _)| sp.start);
    for (sp, m) in unused {
        ck.diags.warn("W0003", sp, format!("unused import `std/{m}`")).note("remove it");
    }
    if ck.diags.has_errors() {
        None
    } else {
        let structs = order
            .into_iter()
            .map(|i| {
                let s = &ck.structs[i];
                HStruct {
                    ty: s.ty,
                    fields: s.fields.iter().map(|(n, t, _)| (n.clone(), *t)).collect(),
                    variants: s
                        .variants
                        .iter()
                        .map(|v| (v.name.clone(), v.fields.iter().map(|(n, t, _)| (n.clone(), *t)).collect()))
                        .collect(),
                }
            })
            .collect();
        Some(HProgram { fns, structs, tramps: std::mem::take(&mut ck.tramps) })
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
    IntTy::from_name(name).is_some() || matches!(name, "bool" | "str" | "String" | "Vec" | "Box")
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
        // `os.exit(...)` never returns (V8).
        StmtKind::Expr(e) => is_exit_call(e),
        StmtKind::If { then, els: Some(els), .. } | StmtKind::IfLet { then, els: Some(els), .. } => {
            terminates(&then.stmts)
                && match els {
                    Else::Block(b) => terminates(&b.stmts),
                    Else::If(s) => stmt_terminates(s),
                }
        }
        // A `match` that does not cover every case is an error, so its arms are all that runs.
        StmtKind::Match { arms, .. } => !arms.is_empty() && arms.iter().all(|a| terminates(&a.body.stmts)),
        StmtKind::Loop { body } => !contains_break(&body.stmts),
        StmtKind::Block(b) => terminates(&b.stmts),
        _ => false,
    }
}

/// Whether `e` is a call that never returns: `os.exit(...)` or `panic(...)`.
fn is_exit_call(e: &Expr) -> bool {
    let ExprKind::Call { callee, .. } = &e.kind else { return false };
    matches!(&callee.kind, ExprKind::Field { base, name } if name == "exit" && matches!(&base.kind, ExprKind::Ident(m) if m == "os"))
        || matches!(&callee.kind, ExprKind::Ident(n) if n == "panic")
}

/// Whether `e` is `x ?? break` (possibly as the last link of a `??` chain).
fn expr_breaks(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Coalesce { rhs, .. } => matches!(rhs.kind, ExprKind::Break) || expr_breaks(rhs),
        ExprKind::Paren(inner) => expr_breaks(inner),
        _ => false,
    }
}

/// Whether a `break` in these statements targets the enclosing loop.
fn contains_break(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Break => true,
        StmtKind::Let { init: e, .. } | StmtKind::Assign { value: e, .. } | StmtKind::Expr(e) => expr_breaks(e),
        StmtKind::If { then, els, .. } | StmtKind::IfLet { then, els, .. } => {
            contains_break(&then.stmts)
                || match els {
                    Some(Else::Block(b)) => contains_break(&b.stmts),
                    Some(Else::If(s)) => contains_break(std::slice::from_ref(s.as_ref())),
                    None => false,
                }
        }
        StmtKind::Match { arms, .. } => arms.iter().any(|a| contains_break(&a.body.stmts)),
        StmtKind::Block(b) => contains_break(&b.stmts),
        _ => false,
    })
}

impl Checker<'_> {
    /// `use std/m` (stdlib contract §1).
    fn collect_uses(&mut self, prog: &Program) {
        for u in &prog.uses {
            if !MODULES.iter().any(|(m, _)| *m == u.module) {
                let known: Vec<&str> = MODULES.iter().map(|(m, _)| *m).collect();
                self.diags
                    .error("E0010", u.span, format!("there is no standard library module `std/{}`", u.module))
                    .note(format!("the modules are: {}", known.join(", ")));
            } else if let Some((first, _)) = self.imports.get(&u.module) {
                let line = first.line;
                self.diags
                    .error("E0011", u.span, format!("`std/{}` is imported more than once", u.module))
                    .note(format!("first imported on line {line}"));
            } else {
                self.imports.insert(u.module.clone(), (u.span, false));
            }
        }
    }

    /// A use of a type that a module provides marks that module's import as used.
    fn mark_type_used(&mut self, name: &str) {
        for (m, types) in MODULES {
            if types.contains(&name) {
                if let Some((_, used)) = self.imports.get_mut(*m) {
                    *used = true;
                }
            }
        }
    }

    /// `m` in `m.f(...)` / `m.name` when it is a standard library module rather than a variable.
    /// Reports a missing import; returns whether `m` names a module.
    fn module_ref(&mut self, m: &str, span: Span) -> bool {
        if !MODULES.iter().any(|(name, _)| *name == m) {
            return false;
        }
        match self.imports.get_mut(m) {
            Some((_, used)) => *used = true,
            None => {
                self.diags
                    .error("E0010", span, format!("module `{m}` is not imported"))
                    .note(format!("add `use std/{m}` at the top of the file"));
            }
        }
        true
    }

    /// Registers every struct and enum, resolves member types and returns them in dependency
    /// order. Library enums (`fs.Error`, ...) are registered too.
    fn collect_structs(&mut self, prog: &Program) -> Vec<usize> {
        let names: Vec<(&str, Span, bool)> = prog
            .structs
            .iter()
            .map(|s| (s.name.as_str(), s.name_span, false))
            .chain(prog.enums.iter().map(|e| (e.name.as_str(), e.name_span, true)))
            .collect();
        for &(name, span, is_enum) in &names {
            if is_type_name(name) || UNSUPPORTED_TYPES.contains(&name) || name == "Ordering" {
                self.diags.error("E0011", span, format!("`{name}` is a built-in type name and cannot be redefined"));
            } else if let Some(&prev) = self.struct_index.get(name) {
                let line = self.structs[prev].span.line;
                let what = if is_enum { "enum" } else { "struct" };
                self.diags
                    .error("E0011", span, format!("type `{name}` is defined more than once ({what})"))
                    .note(format!("first defined on line {line}"));
            } else {
                self.struct_index.insert(name.to_string(), self.structs.len());
            }
            // Every declaration gets an id, even a duplicate, so ids and indexes stay equal.
            let ty = if is_enum { Ty::Enum(register_enum(name)) } else { Ty::Struct(register_struct(name)) };
            self.structs.push(StructDef { name: name.to_string(), span, ty, fields: Vec::new(), variants: Vec::new() });
        }
        for (lname, cases) in LIBRARY_ENUMS {
            let ty = Ty::Enum(register_enum(lname));
            self.struct_index.insert(lname.to_string(), self.structs.len());
            let int = Ty::Int(IntTy::I64);
            let span = Span { start: 0, end: 0, line: 1, col: 1 };
            let variants = cases
                .iter()
                .map(|(n, fs)| VariantDef { name: n.to_string(), fields: fs.iter().map(|f| (f.to_string(), int, span)).collect() })
                .collect();
            self.structs.push(StructDef { name: lname.to_string(), span, ty, fields: Vec::new(), variants });
        }
        for (i, s) in prog.structs.iter().enumerate() {
            self.structs[i].fields = self.member_fields(&s.fields, &s.name);
        }
        let base = prog.structs.len();
        for (j, e) in prog.enums.iter().enumerate() {
            let mut variants: Vec<VariantDef> = Vec::new();
            for v in &e.variants {
                if let Some(prev) = e.variants.iter().take_while(|w| !std::ptr::eq(*w, v)).find(|w| w.name == v.name) {
                    let line = prev.span.line;
                    self.diags
                        .error("E0011", v.span, format!("case `{}` is declared more than once in `{}`", v.name, e.name))
                        .note(format!("first declared on line {line}"));
                }
                let fields = self.member_fields(&v.fields, &format!("{}.{}", e.name, v.name));
                variants.push(VariantDef { name: v.name.clone(), fields });
            }
            self.structs[base + j].variants = variants;
        }
        let mut state = vec![0u8; self.structs.len()];
        let mut order = Vec::new();
        for i in 0..self.structs.len() {
            self.visit_struct(i, &mut state, &mut order, &mut Vec::new());
        }
        // Copyability and drop obligations follow the members, which come first in `order`.
        let decls = prog.structs.iter().map(|s| &s.impls).chain(prog.enums.iter().map(|e| &e.impls));
        for (i, impls) in decls.enumerate() {
            if impls.iter().any(|(n, _)| n == "Drop") {
                let (Ty::Struct(id) | Ty::Enum(id)) = self.structs[i].ty else { unreachable!() };
                set_user_drop(id);
            }
        }
        for &i in &order {
            let s = &self.structs[i];
            let members: Vec<Vec<Ty>> = if matches!(s.ty, Ty::Enum(_)) {
                s.variants.iter().map(|v| v.fields.iter().map(|f| f.1).collect()).collect()
            } else {
                vec![s.fields.iter().map(|f| f.1).collect()]
            };
            let (Ty::Struct(id) | Ty::Enum(id)) = s.ty else { unreachable!() };
            finish_agg(id, members);
        }
        order
    }

    /// Resolves the fields of a struct or of an enum case. A view cannot be stored (rule 4).
    fn member_fields(&mut self, decls: &[FieldDecl], owner: &str) -> Vec<(String, Ty, Span)> {
        let mut fields: Vec<(String, Ty, Span)> = Vec::new();
        for f in decls {
            let ty = self.resolve_ty(&f.ty);
            if ty.carries_borrow() {
                self.diags
                    .error("E0301", f.ty.span, format!("field `{}` cannot hold a `{ty}`: a view cannot be stored in a field", f.name))
                    .note("store an owned value instead, e.g. `String` (call `.toString()`)");
            } else if matches!(ty, Ty::Res(_)) {
                self.diags.error("U0001", f.ty.span, format!("fields of type `{ty}` are not supported yet"));
            }
            if let Some((_, _, first)) = fields.iter().find(|(n, _, _)| *n == f.name) {
                let line = first.line;
                self.diags
                    .error("E0011", f.span, format!("field `{}` is declared more than once in `{owner}`", f.name))
                    .note(format!("first declared on line {line}"));
            }
            fields.push((f.name.clone(), ty, f.span));
        }
        fields
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
        // `?S` holds an `S` by value, so it counts as containment too.
        fn by_value(t: Ty, out: &mut Vec<usize>) {
            match t {
                Ty::Struct(id) | Ty::Enum(id) => out.push(id as usize),
                Ty::Opt(o) => by_value(opt_inner(o), out),
                Ty::Tuple(t) => {
                    for e in tuple_elems(t) {
                        by_value(e, out);
                    }
                }
                Ty::Res(r) => {
                    let (a, b) = res_parts(r);
                    by_value(a, out);
                    by_value(b, out);
                }
                _ => {}
            }
        }
        let mut deps: Vec<usize> = Vec::new();
        let s = &self.structs[i];
        for (_, t, _) in s.fields.iter().chain(s.variants.iter().flat_map(|v| v.fields.iter())) {
            by_value(*t, &mut deps);
        }
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
        if name == "String" && inits.is_empty() {
            return HExpr { kind: HExprKind::EmptyString, ty: Ty::String, span, at: span };
        }
        let Some(&si) = self.struct_index.get(name) else {
            if UNSUPPORTED_TYPES.contains(&name) {
                self.mark_type_used(name);
                self.diags.error("U0001", name_span, format!("constructing `{name}` is not supported in P0"));
            } else {
                self.diags.error("E0010", name_span, format!("cannot find struct `{name}`"));
            }
            for init in inits {
                self.expr(&init.value, None, cx);
            }
            return err_expr(span);
        };
        if matches!(self.structs[si].ty, Ty::Enum(_)) {
            self.diags
                .error("E0001", name_span, format!("`{name}` is an enum, not a struct"))
                .note(format!("construct one of its cases, e.g. `{name}.Case(...)`"));
            for init in inits {
                self.expr(&init.value, None, cx);
            }
            return err_expr(span);
        }
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
                    let h = self.fit(h, fty, cx);
                    let h = self.consume(h, cx);
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
        // An optional field that is not written is `none` (stdlib contract §1).
        for (fi, (_, fty)) in decl.iter().enumerate() {
            if !seen[fi] && matches!(fty, Ty::Opt(_)) {
                seen[fi] = true;
                out.push((fi, HExpr { kind: HExprKind::None, ty: *fty, span, at: span }));
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

    /// `T: Drop` needs exactly `fn drop(inout self)` (V10, E0802); the name `drop` is reserved
    /// for it.
    fn check_drops(&mut self, prog: &Program) {
        let decls = prog.structs.iter().map(|s| (&s.name, &s.impls)).chain(prog.enums.iter().map(|e| (&e.name, &e.impls)));
        for (name, impls) in decls {
            let declared = impls.iter().find(|(n, _)| n == "Drop").map(|(_, sp)| *sp);
            let full = format!("{name}.drop");
            let f = prog.fns.iter().find(|f| f.name == full);
            match (declared, f) {
                (Some(sp), None) => {
                    self.diags
                        .error("E0802", sp, format!("`{name}: Drop` needs a method `fn drop(inout self)`"))
                        .note("write it inside the type's body");
                }
                (Some(_), Some(f)) => {
                    let ok = f.params.len() == 1
                        && f.params[0].name == "self"
                        && f.params[0].inout
                        && f.ret.as_ref().is_none_or(|t| t.name == "()");
                    if !ok {
                        self.diags
                            .error("E0802", f.name_span, "a drop method must be declared exactly as `fn drop(inout self)`")
                            .note("it takes no other parameters and returns nothing; handle failures inside it, or panic");
                    }
                }
                (None, Some(f)) => {
                    self.diags
                        .error("E0802", f.name_span, format!("the method name `drop` is reserved for types that implement `Drop`"))
                        .note(format!("declare `{name}: Drop` to run it when a value is dropped, or rename the method"));
                }
                (None, None) => {}
            }
        }
    }

    /// `T: Ord` needs `fn cmp(self, other: &T) -> Ordering`; `T: Eq` needs
    /// `fn eq(self, other: &T) -> bool` (K2, E0502).
    fn check_protocols(&mut self, prog: &Program) {
        let decls = prog.structs.iter().map(|s| (&s.name, &s.impls)).chain(prog.enums.iter().map(|e| (&e.name, &e.impls)));
        for (name, impls) in decls {
            let Some(&si) = self.struct_index.get(name.as_str()) else { continue };
            let (Ty::Struct(id) | Ty::Enum(id)) = self.structs[si].ty else { continue };
            let mut found = (None, None);
            for (proto, sp) in impls {
                let (method, ret, shape) = match proto.as_str() {
                    "Ord" => ("cmp", "Ordering", "fn cmp(self, other: &T) -> Ordering"),
                    "Eq" => ("eq", "bool", "fn eq(self, other: &T) -> bool"),
                    _ => continue,
                };
                let full = format!("{name}.{method}");
                let Some(f) = prog.fns.iter().find(|f| f.name == full) else {
                    self.diags
                        .error("E0502", *sp, format!("`{name}: {proto}` needs a method `{}`", shape.replace('T', name)))
                        .note("write it inside the type's body");
                    continue;
                };
                let ok = f.params.len() == 2
                    && f.params[0].name == "self"
                    && !f.params[0].inout
                    && !f.params[0].sink
                    && !f.params[1].inout
                    && !f.params[1].sink
                    && f.params[1].ty.name == "&"
                    && f.params[1].ty.args.first().is_some_and(|a| a.name == *name && a.args.is_empty())
                    && f.ret.as_ref().is_some_and(|t| t.name == ret && t.args.is_empty());
                if !ok {
                    self.diags
                        .error("E0502", f.name_span, format!("`{name}: {proto}` needs exactly `{}`", shape.replace('T', name)))
                        .note("the comparison protocols have fixed signatures (semantic table K2)");
                    continue;
                }
                let fi = self.index.get(&full).copied();
                if proto == "Ord" {
                    found.0 = fi;
                } else {
                    found.1 = fi;
                }
            }
            if found != (None, None) {
                self.protocols.insert(id, found);
            }
        }
    }

    /// Whether values of `t` are compared through a user protocol: `(cmp, eq)` functions.
    fn protocol(&self, t: Ty) -> Option<(Option<usize>, Option<usize>)> {
        match t {
            Ty::Struct(id) | Ty::Enum(id) => self.protocols.get(&id).copied(),
            _ => None,
        }
    }

    /// `a op b` through a user `cmp` (`Ord`) or `eq` (`Eq`): `cmp(a, &b)` tested against an
    /// `Ordering` tag, or `eq(a, &b)` (negated for `!=`).
    fn protocol_compare(&mut self, op: BinOp, l: HExpr, r: HExpr, span: Span, at: Span) -> Option<HExpr> {
        let (cmp, eq) = self.protocol(l.ty)?;
        let addr = |r: HExpr| {
            let (s, a) = (r.span, r.at);
            let ty = Ty::Ref(ref_of(r.ty));
            HExpr { kind: HExprKind::AddrOf(Box::new(r)), ty, span: s, at: a }
        };
        if let Some(fi) = cmp {
            let ord = self.structs[self.struct_index["Ordering"]].ty;
            let call = HExpr { kind: HExprKind::Call { func: fi, args: vec![l, addr(r)] }, ty: ord, span, at };
            // Less 0, Equal 1, Greater 2.
            let (tag, eq) = match op {
                BinOp::Lt => (0, true),
                BinOp::Ge => (0, false),
                BinOp::Gt => (2, true),
                BinOp::Le => (2, false),
                BinOp::Eq => (1, true),
                _ => (1, false),
            };
            return Some(HExpr { kind: HExprKind::TagTest { value: Box::new(call), tag, eq }, ty: Ty::Bool, span, at });
        }
        let fi = eq?;
        if !matches!(op, BinOp::Eq | BinOp::Ne) {
            self.diags
                .error("E0501", span, format!("values of type `{}` cannot be ordered: it implements `Eq` but not `Ord`", l.ty))
                .note("declare `Ord` and write `fn cmp(self, other: &T) -> Ordering`");
            return Some(err_expr(span));
        }
        let call = HExpr { kind: HExprKind::Call { func: fi, args: vec![l, addr(r)] }, ty: Ty::Bool, span, at };
        if op == BinOp::Ne {
            return Some(HExpr { kind: HExprKind::Not(Box::new(call)), ty: Ty::Bool, span, at });
        }
        Some(call)
    }

    fn collect(&mut self, prog: &Program) {
        for f in &prog.fns {
            let params = f.params.iter().map(|p| self.resolve_ty(&p.ty)).collect();
            let sinks = f.params.iter().map(|p| p.sink).collect();
            let inouts = f.params.iter().map(|p| p.inout).collect();
            let ret = f.ret.as_ref().map(|t| self.resolve_ty(t)).unwrap_or(Ty::Unit);
            let ret_of = f.ret_of.as_ref().map(|names| {
                names.iter().filter_map(|(n, _)| f.params.iter().position(|p| p.name == *n)).collect()
            });
            let has_self = f.owner.is_some() && f.params.first().is_some_and(|p| p.name == "self");
            if f.owner.is_none() && (OUTPUT_FNS.contains(&f.name.as_str()) || is_type_name(&f.name)) {
                self.diags.error("E0011", f.name_span, format!("`{}` is a built-in name and cannot be redefined", f.name));
            } else if let Some(&prev) = self.index.get(&f.name) {
                let line = prog.fns[prev].name_span.line;
                let what = if f.owner.is_some() { "method" } else { "function" };
                self.diags
                    .error("E0011", f.name_span, format!("{what} `{}` is defined more than once", f.name))
                    .note(format!("first defined on line {line}"));
            } else {
                self.index.insert(f.name.clone(), self.sigs.len());
            }
            self.sigs.push(Sig { params, sinks, inouts, ret, ret_of, has_self });
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
        if t.name == "()" {
            return Ty::Unit;
        }
        if t.name == "(,)" {
            let elems: Vec<Ty> = t.args.iter().map(|a| self.resolve_ty(a)).collect();
            if elems.contains(&Ty::Error) {
                return Ty::Error;
            }
            if let Some(i) = elems.iter().position(|e| *e == Ty::Unit) {
                self.diags.error("E0001", t.args[i].span, "a tuple element cannot be `()`");
                return Ty::Error;
            }
            return Ty::Tuple(tuple_of(elems));
        }
        if t.name == "[]" {
            return match self.resolve_ty(&t.args[0]) {
                Ty::Error => Ty::Error,
                e => Ty::Slice(slice_of(e)),
            };
        }
        if t.name == "!" {
            let ok = self.resolve_ty(&t.args[0]);
            let err = self.resolve_ty(&t.args[1]);
            if ok == Ty::Error || err == Ty::Error {
                return Ty::Error;
            }
            if matches!(ok, Ty::Res(_)) || matches!(err, Ty::Res(_) | Ty::Unit) {
                self.diags.error("E0001", t.span, format!("`{ok} ! {err}` is not a valid result type"));
                return Ty::Error;
            }
            if err.carries_borrow() {
                self.diags
                    .error("E0301", t.args[1].span, format!("an error value cannot hold a `{err}`: a view cannot be stored in it"))
                    .note("store an owned value instead, e.g. `String`");
                return Ty::Error;
            }
            return Ty::Res(res_of(ok, err));
        }
        if let Some((m, _)) = t.name.split_once('.') {
            if !self.module_ref(m, t.span) {
                self.diags.error("E0010", t.span, format!("unknown type `{}`", t.name));
                return Ty::Error;
            }
            return match self.struct_index.get(&t.name) {
                Some(&i) => self.structs[i].ty,
                None => {
                    self.diags.error("U0001", t.span, format!("type `{}` is not supported yet", t.name));
                    Ty::Error
                }
            };
        }
        if t.name == "&" {
            return match t.args.first().map(|a| self.resolve_ty(a)) {
                Some(Ty::Error) | None => Ty::Error,
                Some(inner) => Ty::Ref(ref_of(inner)),
            };
        }
        if t.name == "?" {
            return match t.args.first().map(|a| self.resolve_ty(a)) {
                Some(Ty::Error) | None => Ty::Error,
                Some(inner) => Ty::Opt(opt_of(inner)),
            };
        }
        if t.name == "Map" {
            let [k, v] = t.args.as_slice() else {
                self.diags.error("E0001", t.span, "`Map` takes two type arguments, e.g. `Map[String, int]`");
                return Ty::Error;
            };
            let (k_ty, v_ty) = (self.resolve_ty(k), self.resolve_ty(v));
            if k_ty == Ty::Error || v_ty == Ty::Error {
                return Ty::Error;
            }
            if !k_ty.is_hashable() {
                self.diags
                    .error("E0501", k.span, format!("`{k_ty}` cannot be a Map key"))
                    .note("keys can be integers, bool, str or String");
                return Ty::Error;
            }
            return Ty::Map(map_of(k_ty, v_ty));
        }
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
        if t.name == "Box" {
            let [inner] = t.args.as_slice() else {
                self.diags.error("E0001", t.span, "`Box` takes exactly one type argument, e.g. `Box[Node]`");
                return Ty::Error;
            };
            return match self.resolve_ty(inner) {
                Ty::Error => Ty::Error,
                Ty::Unit => {
                    self.diags.error("E0001", inner.span, "a `Box` cannot hold `()`");
                    Ty::Error
                }
                // A box keeps its content for as long as it likes, so a view cannot go in (E1, E2).
                e if e.carries_borrow() => {
                    self.box_borrow_error(e, inner.span);
                    Ty::Error
                }
                e => Ty::Box(box_of(e)),
            };
        }
        if !t.args.is_empty() {
            if UNSUPPORTED_TYPES.contains(&t.name.as_str()) {
                self.mark_type_used(&t.name);
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
                self.mark_type_used(name);
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
        let mut cx = FnCx::new(ret, Pre::build(&f.body));
        cx.ret_of = f.ret_of.as_ref().map(|v| v.iter().map(|(n, _)| n.clone()).collect());
        cx.in_drop = f.owner.as_ref().is_some_and(|o| {
            f.name == format!("{o}.drop") && self.struct_index.get(o).is_some_and(|&i| has_user_drop(self.structs[i].ty))
        });
        self.check_ret_of(f, ret);
        let params = f
            .params
            .iter()
            .zip(param_tys)
            .map(|(p, ty)| {
                let kind = if p.sink {
                    LocalKind::SinkParam
                } else if p.inout {
                    LocalKind::InoutParam
                } else {
                    LocalKind::Param
                };
                let id = self.declare(&mut cx, &p.name, p.span, ty, kind);
                // A borrowed parameter that carries a borrow points into the caller's data.
                if ty.carries_borrow() || (!p.sink && !ty.is_copy()) {
                    cx.locals[id].loans = vec![Loan::Local(id)];
                }
                id
            })
            .collect();
        let body = self.stmts(&f.body.stmts, &mut cx);
        // `-> () ! E` succeeds when it reaches its end, like a function without a return type.
        let unit_res = matches!(ret, Ty::Res(r) if res_parts(r).0 == Ty::Unit);
        if ret != Ty::Unit && ret != Ty::Error && !unit_res && !terminates(&f.body.stmts) {
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
            locals: hlocals(cx.locals),
            body,
        }
    }

    /// Checks a closure passed to a library callback that cannot keep it (`sortBy`, `update`;
    /// O5) as a function of its own. The captured outer variables become its first parameters,
    /// read-only and passed in place for the duration of the call; the closure's own
    /// parameters follow. Returns the function's index and the captured values.
    fn lift_closure(&mut self, e: &Expr, param_tys: &[Ty], ret: Ty, what: &str, cx: &mut FnCx) -> Option<(usize, Vec<HExpr>)> {
        let ExprKind::Closure { params, body } = &e.kind else {
            self.diags.error("E0001", e.span, format!("`{what}` takes a closure here, e.g. `(a, b) => a < b`"));
            self.expr(e, None, cx);
            return None;
        };
        if params.len() != param_tys.len() {
            self.diags.error(
                "E0012",
                e.span,
                format!("the closure for `{what}` takes {} but this one has {}", plural(param_tys.len(), "parameter"), params.len()),
            );
            return None;
        }
        let mut names = Vec::new();
        idents_expr(body, &mut names);
        let mut caps: Vec<LocalId> = Vec::new();
        for (n, _) in &names {
            if params.iter().any(|(p, _, _)| p == n) {
                continue;
            }
            if let Some(id) = self.lookup(cx, n) {
                if !caps.contains(&id) {
                    caps.push(id);
                }
            }
        }
        let pre = match &body.kind {
            ExprKind::Block(b) => Pre::build(b),
            _ => Pre::default(),
        };
        let mut ccx = FnCx::new(ret, pre);
        let mut ids = Vec::new();
        let mut cap_exprs = Vec::new();
        for &id in &caps {
            let (name, ty) = (cx.locals[id].name.clone(), cx.locals[id].ty);
            if ty.contains_ref_mut() {
                self.diags.error("U0001", e.span, format!("a closure cannot use the `&inout` value `{name}` yet"));
                return None;
            }
            cx.locals[id].used = true;
            self.check_moved(cx, id, e.span);
            let pid = self.declare(&mut ccx, &name, e.span, ty, LocalKind::Param);
            ccx.locals[pid].used = true;
            ccx.captures.push(pid);
            if ty.carries_borrow() || !ty.is_copy() {
                ccx.locals[pid].loans = vec![Loan::Local(pid)];
            }
            ids.push(pid);
            cap_exprs.push(HExpr { kind: HExprKind::Local(id), ty, span: e.span, at: e.span });
        }
        for ((n, sp, t), &pt) in params.iter().zip(param_tys) {
            if let Some(t) = t {
                let d = self.resolve_ty(t);
                if d != pt && d != Ty::Error && pt != Ty::Error {
                    self.diags.error("E0001", t.span, format!("this parameter of the `{what}` closure is `{pt}`, not `{d}`"));
                }
            }
            let pid = self.declare(&mut ccx, n, *sp, pt, LocalKind::Param);
            if pt.carries_borrow() || !pt.is_copy() {
                ccx.locals[pid].loans = vec![Loan::Local(pid)];
            }
            ids.push(pid);
        }
        let stmts = match &body.kind {
            ExprKind::Block(b) => {
                let stmts = self.stmts(&b.stmts, &mut ccx);
                if ret != Ty::Unit && !terminates(&b.stmts) {
                    self.diags
                        .error("E0013", b.close, format!("this closure can reach its end without returning a value of type `{ret}`"))
                        .note("add a `return`, or write the closure as a single expression: `(a, b) => a < b`");
                }
                stmts
            }
            _ => {
                let h = self.expr(body, Some(ret), &mut ccx);
                let h = self.fit(h, ret, &mut ccx);
                let h = self.consume(h, &mut ccx);
                vec![HStmt { kind: HStmtKind::Return(Some(h)), span: body.span }]
            }
        };
        self.pop_scope(&mut ccx);
        let n = ids.len();
        let idx = self.n_fns + self.lifted.len();
        let name = format!("closure.{}", self.lifted.len());
        self.lifted.push(HFn { name, params: ids, sinks: vec![false; n], ret, locals: hlocals(ccx.locals), body: stmts });
        Some((idx, cap_exprs))
    }

    /// `v.sortBy((a, b) => ...)`: a stable sort where the closure says whether `a` goes before `b`.
    fn sort_by_call(&mut self, base: &Expr, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let Some(place) = self.mutable_place(base, "sortBy", cx) else { return err_expr(span) };
        let [arg] = args else {
            self.diags.error("E0012", span, "`sortBy` takes one argument: a closure such as `(a, b) => a < b`");
            return err_expr(span);
        };
        let elem = match place.ty {
            Ty::Vec(v) => vec_elem(v),
            Ty::Error => return err_expr(span),
            other => {
                self.diags.error("U0001", at, format!("method `sortBy` on `{other}` is not supported yet"));
                return err_expr(span);
            }
        };
        let Some((func, caps)) = self.lift_closure(arg, &[elem, elem], Ty::Bool, "sortBy", cx) else { return err_expr(span) };
        if caps.iter().any(|c| matches!(c.kind, HExprKind::Local(id) if id == place.local)) {
            let n = cx.locals[place.local].name.clone();
            self.diags
                .error("E0102", arg.span, format!("`{n}` is being sorted, so the closure cannot use it"))
                .note("copy what the closure needs into a variable first");
            return err_expr(span);
        }
        let tramp = self.tramps.len();
        self.tramps.push(Tramp { func, caps: caps.iter().map(|c| c.ty).collect(), elem, by_cmp: false });
        HExpr { kind: HExprKind::SortBy { place, tramp, caps }, ty: Ty::Unit, span, at }
    }

    /// `m.update(key, initial, (v) => ...)`: the closure runs exactly once, on the stored value
    /// or on `initial` (stdlib contract §3; v1: a copyable value type without borrows).
    #[allow(clippy::too_many_arguments)]
    fn update_call(&mut self, base: &Expr, mid: u32, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let Some(place) = self.mutable_place(base, "update", cx) else { return err_expr(span) };
        let [ka, ia, fa] = args else {
            self.diags.error("E0012", span, "`update` takes a key, an initial value and a closure, e.g. `m.update(k, 0, (n) => n + 1)`");
            return err_expr(span);
        };
        let id = place.local;
        let local = cx.locals[id].name.clone();
        if let Some(a) = [ka, ia].into_iter().find(|a| mentions(a, &local)) {
            self.diags
                .error("E0403", a.span, format!("`{local}` is already borrowed by `{local}.update(...)`, so the argument cannot use it"))
                .note(format!("compute the value first: `let x = ...` then `{local}.update(x, ...)`"));
            return err_expr(span);
        }
        let (k, v) = map_parts(mid);
        if !v.is_copy() || v.carries_borrow() {
            self.diags
                .error("E0001", at, format!("`update` needs a copyable value type without borrows, and `{v}` is not"))
                .note("use `getMut` and change the value in place");
            return err_expr(span);
        }
        let kh = self.expr(ka, Some(k), cx);
        let kh = self.fit(kh, k, cx);
        let kh = self.consume(kh, cx);
        if kh.ty.carries_borrow() {
            let loans = self.loans_of(&kh, cx);
            if !loans.is_empty() {
                if matches!(cx.locals[id].ty, Ty::RefMut(_)) || cx.locals[id].kind == LocalKind::InoutParam {
                    self.diags.error("E0306", span, format!("cannot store a borrow into the caller's `{local}`"));
                } else {
                    self.store_loans(cx, id, loans, span, false);
                }
            }
        }
        let ih = self.expr(ia, Some(v), cx);
        let ih = self.fit(ih, v, cx);
        let Some((func, caps)) = self.lift_closure(fa, &[v], v, "update", cx) else { return err_expr(span) };
        if caps.iter().any(|c| matches!(c.kind, HExprKind::Local(c_id) if c_id == id)) {
            self.diags.error("E0102", fa.span, format!("`{local}` is being updated, so the closure cannot use it"));
            return err_expr(span);
        }
        HExpr { kind: HExprKind::MapUpdate { place, key: Box::new(kh), init: Box::new(ih), func, caps }, ty: Ty::Unit, span, at }
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
        let decl = cx.cur;
        let fn_end = cx.pre.next;
        let scope_end = if decl == 0 { fn_end } else { cx.pre.let_end.get(&decl).copied().unwrap_or(fn_end) };
        let depth = cx.scopes.len();
        cx.locals.push(LocalState {
            name: name.to_string(),
            ty,
            kind,
            used: false,
            span,
            decl,
            scope_end,
            depth,
            loans: Vec::new(),
            map_key: false,
        });
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
            if !l.used && !l.name.starts_with('_') && l.name != "self" && l.ty != Ty::Error {
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
        if let Ty::Opt(o) = expected {
            if opt_inner(o) == Ty::Error {
                return;
            }
        }
        if let Ty::Res(r) = e.ty {
            if res_parts(r).0 == expected {
                self.diags
                    .error("E0001", e.span, format!("this value can fail (`{}`), so it cannot be used as a `{expected}`", e.ty))
                    .note("handle the error first: `try f()`, `f() catch e => value`, or `match f() { ok(v) => ... err(e) => ... }`");
                return;
            }
        }
        if let Ty::Opt(o) = e.ty {
            if opt_inner(o) == expected {
                self.diags
                    .error("E0001", e.span, format!("this value may be absent (`{}`), so it cannot be used as a `{expected}`", e.ty))
                    .note("handle `none` first: `if let`, `?? default`, or `match` with `some(x)` / `none`");
                return;
            }
        }
        if let Ty::Ref(id) = e.ty {
            if ref_inner(id) == expected && !expected.is_copy() {
                self.diags
                    .error("E0404", e.span, format!("cannot take a `{expected}` out of a borrow (`{}`)", e.ty))
                    .note("make a copy with `.clone()`, or keep using the reference");
                return;
            }
        }
        if let Ty::RefMut(id) = e.ty {
            if ref_mut_inner(id) == expected && !expected.is_copy() {
                self.diags
                    .error("E0404", e.span, format!("cannot take a `{expected}` out of a borrow (`{}`)", e.ty))
                    .note("make a copy with `.clone()`, or keep using the reference");
                return;
            }
        }
        let d = self.diags.error("E0001", e.span, format!("mismatched types: expected `{expected}`, found `{}`", e.ty));
        if let (Ty::Int(_), Ty::Int(_)) = (e.ty, expected) {
            d.note(format!("Yavo has no implicit conversions; convert explicitly with `{expected}(...)`"));
        }
    }

    /// A `T` used where a `?T` is expected is wrapped as `some(...)` (section 8 whitelist), which
    /// moves it; a copyable `T` read through a `&T` is copied first.
    fn coerce(&mut self, h: HExpr, expected: Ty, cx: &mut FnCx) -> HExpr {
        // A copyable `T` is read out of a `&T` / `&inout T` where a `T` is expected (C4).
        let h = match h.ty {
            Ty::Ref(r) if ref_inner(r) == expected && expected.is_copy() => self.deref(h),
            Ty::RefMut(r) if ref_mut_inner(r) == expected && expected.is_copy() => self.deref(h),
            _ => h,
        };
        // In a function returning `T ! E`, a `T` is a success value (section 8 whitelist).
        if let Ty::Res(r) = expected {
            let ok = res_parts(r).0;
            if h.ty == expected || h.ty == Ty::Error || ok == Ty::Unit || ok == Ty::Error {
                return h;
            }
            let h = self.coerce(h, ok, cx);
            if h.ty != ok {
                return h;
            }
            let (span, at) = (h.span, h.at);
            let h = self.consume(h, cx);
            return HExpr { kind: HExprKind::Ok(Some(Box::new(h))), ty: expected, span, at };
        }
        // A vector is viewed as a slice where `[]T` is expected (section 8 whitelist).
        if let (Ty::Slice(s), Ty::Vec(v)) = (expected, h.ty) {
            if slice_elem(s) == vec_elem(v) {
                let (span, at) = (h.span, h.at);
                return HExpr { kind: HExprKind::SliceOf(Box::new(h)), ty: expected, span, at };
            }
        }
        let Ty::Opt(o) = expected else { return h };
        let inner = opt_inner(o);
        let h = match h.ty {
            Ty::Ref(r) if ref_inner(r) == inner && inner.is_copy() => self.deref(h),
            _ => h,
        };
        if h.ty != inner || inner == Ty::Error {
            return h;
        }
        let (span, at) = (h.span, h.at);
        let h = self.consume(h, cx);
        HExpr { kind: HExprKind::Some(Box::new(h)), ty: expected, span, at }
    }

    /// `coerce`, then report a type mismatch.
    fn fit(&mut self, h: HExpr, expected: Ty, cx: &mut FnCx) -> HExpr {
        let h = self.coerce(h, expected, cx);
        self.expect_ty(&h, expected);
        h
    }

    /// Reads through a `&T` where the value is only read (printing, operators, method
    /// receivers, indexing, read-only arguments).
    fn deref(&self, h: HExpr) -> HExpr {
        match h.ty {
            Ty::Ref(_) | Ty::RefMut(_) => {
                let inner = match h.ty {
                    Ty::Ref(id) => ref_inner(id),
                    Ty::RefMut(id) => ref_mut_inner(id),
                    _ => unreachable!(),
                };
                let (span, at) = (h.span, h.at);
                HExpr { kind: HExprKind::Deref(Box::new(h)), ty: inner, span, at }
            }
            _ => h,
        }
    }

    /// `&place` for an `inout` parameter.
    fn inout_arg(&mut self, a: &Expr, exp: Option<Ty>, borrowed: &[LocalId], mut_borrowed: &mut Vec<LocalId>, cx: &mut FnCx) -> HExpr {
        let span = a.span;
        let ExprKind::Borrow(inner) = &a.kind else {
            self.diags.error("E0001", span, "this parameter is `inout`: pass a variable with `&`, e.g. `&v`");
            self.expr(a, None, cx);
            return err_expr(span);
        };
        let ExprKind::Ident(name) = &inner.kind else {
            // `&x.f.g`: a field of a writable variable, possibly through `&inout` and `Box`.
            if !matches!(&inner.kind, ExprKind::Field { .. } | ExprKind::Paren(_)) {
                self.diags.error("U0001", span, "only variables and their fields can be passed as `inout` for now");
                return err_expr(span);
            }
            let Some(place) = self.place_expr(inner, "pass as `inout`", cx) else { return err_expr(span) };
            if borrowed.contains(&place.local) || mut_borrowed.contains(&place.local) {
                let name = cx.locals[place.local].name.clone();
                self.diags
                    .error("E0102", span, format!("`{name}` is already borrowed by an earlier argument of this call"))
                    .note("a variable passed as `inout` cannot be used by any other argument of the same call");
            }
            mut_borrowed.push(place.local);
            let ty = place.ty;
            let h = HExpr { kind: HExprKind::InoutPlace(place), ty, span, at: span };
            if let Some(t) = exp {
                self.expect_ty(&h, t);
            }
            return h;
        };
        let Some(id) = self.lookup(cx, name) else {
            self.diags.error("E0010", inner.span, format!("cannot find variable `{name}` in this scope"));
            return err_expr(span);
        };
        cx.locals[id].used = true;
        self.check_moved(cx, id, inner.span);
        // An `&inout T` (a writable binding, `getMut`'s result) passes its target on: `&n`.
        if let Ty::RefMut(r) = cx.locals[id].ty {
            let target = ref_mut_inner(r);
            if exp.is_none_or(|t| t == target) {
                if borrowed.contains(&id) || mut_borrowed.contains(&id) {
                    self.diags
                        .error("E0102", span, format!("`{name}` is already borrowed by an earlier argument of this call"))
                        .note("a variable passed as `inout` cannot be used by any other argument of the same call");
                }
                self.check_exclusive(cx, id, inner.span);
                mut_borrowed.push(id);
                let place = HPlace { local: id, path: Vec::new(), ty: target };
                return HExpr { kind: HExprKind::InoutPlace(place), ty: target, span, at: span };
            }
        }
        match cx.locals[id].kind {
            LocalKind::Var | LocalKind::InoutParam => {}
            LocalKind::Let => {
                self.diags
                    .error("E0014", inner.span, format!("cannot pass `{name}` as `inout`: it was declared with `let`"))
                    .note(format!("declare it with `var {name}` to make it mutable"));
            }
            LocalKind::Bind | LocalKind::BindMut | LocalKind::BindOwned => {
                self.binding_write(cx, id, inner.span);
            }
            _ => {
                self.diags.error("E0014", inner.span, format!("cannot pass `{name}` as `inout`: parameters and loop variables are read-only"));
            }
        }
        if borrowed.contains(&id) || mut_borrowed.contains(&id) {
            self.diags
                .error("E0102", span, format!("`{name}` is already borrowed by an earlier argument of this call"))
                .note("a variable passed as `inout` cannot be used by any other argument of the same call");
        }
        self.check_not_borrowed(cx, id, inner.span, "borrow mutably");
        mut_borrowed.push(id);
        let h = HExpr { kind: HExprKind::InoutArg(id), ty: cx.locals[id].ty, span, at: span };
        if let Some(t) = exp {
            self.expect_ty(&h, t);
        }
        h
    }

    fn block(&mut self, b: &Block, cx: &mut FnCx) -> Vec<HStmt> {
        cx.scopes.push(Vec::new());
        let out = self.stmts(&b.stmts, cx);
        self.pop_scope(cx);
        out
    }

    /// Writing through an `if let`/`match` binding. A binding into a read-only place is
    /// read-only (A2); writable bindings are valid v0.5 but not implemented yet.
    fn binding_write(&mut self, cx: &FnCx, id: LocalId, span: Span) {
        let name = cx.locals[id].name.clone();
        if cx.locals[id].kind == LocalKind::Bind {
            self.diags
                .error("E0101", span, format!("cannot modify `{name}`: it is bound to a read-only value"))
                .note("a binding made by `if let` or `match` has the access of the value it matches");
        } else {
            self.diags.error("U0001", span, format!("modifying the `if let`/`match` binding `{name}` is not supported yet"));
        }
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

    /// Signature rules for returned borrows (semantic table O1, O2).
    fn check_ret_of(&mut self, f: &FnDecl, ret: Ty) {
        let span = f.ret.as_ref().map(|t| t.span).unwrap_or(f.name_span);
        let Some(names) = &f.ret_of else {
            if ret.carries_borrow() {
                self.diags
                    .error("E0302", span, format!("`{ret}` points into other data, so the signature must say where it comes from"))
                    .note("add `of <parameter>`, e.g. `-> str of text`, or `of static` for string literals");
            }
            return;
        };
        for (n, sp) in names {
            if n == "static" {
                continue;
            }
            match f.params.iter().find(|p| p.name == *n) {
                None => {
                    self.diags.error("E0010", *sp, format!("`of {n}`: there is no parameter named `{n}`"));
                }
                Some(p) if p.sink => {
                    self.diags
                        .error("E0304", *sp, format!("`of {n}` names a `sink` parameter, which is dropped when the function returns"))
                        .note(format!("borrow it instead (`{n}: {}`), or return an owned value", p.ty.name));
                }
                Some(_) => {}
            }
        }
    }

    /// The variables a value of a borrow-carrying type points into.
    fn loans_of(&self, h: &HExpr, cx: &FnCx) -> Vec<Loan> {
        let recv_loans = |r: &HExpr| -> Vec<Loan> {
            match (&r.kind, r.ty) {
                (HExprKind::Local(id), Ty::String) => vec![Loan::Local(*id)],
                (HExprKind::InoutArg(id), _) => vec![Loan::Local(*id)],
                (HExprKind::InoutPlace(p), _) => vec![Loan::Local(p.local)],
                // A `String` stored in a field or an element, or read through a reference.
                (HExprKind::Field { .. } | HExprKind::Deref(_) | HExprKind::Unbox(_), Ty::String) => self.storage_loans(r, cx),
                (_, Ty::String) => vec![Loan::Temp],
                // A struct or enum (e.g. a method receiver): a view points into its storage.
                (
                    HExprKind::Local(_) | HExprKind::Field { .. } | HExprKind::Deref(_) | HExprKind::Unbox(_),
                    Ty::Struct(_) | Ty::Enum(_),
                ) => {
                    self.storage_loans(r, cx)
                }
                (_, Ty::Struct(_) | Ty::Enum(_)) => vec![Loan::Temp],
                _ => self.loans_of(r, cx),
            }
        };
        match &h.kind {
            HExprKind::Local(id) | HExprKind::Move(id) => cx.locals[*id].loans.clone(),
            HExprKind::Deref(inner) => self.loans_of(inner, cx),
            HExprKind::AddrOf(inner) => self.storage_loans(inner, cx),
            HExprKind::SliceOf(inner) => match inner.kind {
                HExprKind::Local(_) | HExprKind::Field { .. } | HExprKind::Deref(_) | HExprKind::Unbox(_) => {
                    self.storage_loans(inner, cx)
                }
                _ => {
                    let mut out = vec![Loan::Temp];
                    out.extend(self.loans_of(inner, cx));
                    out
                }
            },
            HExprKind::Call2 { method: Method2::Split, recv, .. } => recv_loans(recv),
            HExprKind::Some(inner) => self.loans_of(inner, cx),
            HExprKind::Ok(Some(inner)) | HExprKind::ErrVal(inner) | HExprKind::Try(inner) => self.loans_of(inner, cx),
            HExprKind::Catch { value, handler, .. } => {
                let mut out = self.loans_of(value, cx);
                if let CatchHandler::Value(v) = handler {
                    for l in self.loans_of(v, cx) {
                        if !out.contains(&l) {
                            out.push(l);
                        }
                    }
                }
                out
            }
            HExprKind::Coalesce { lhs, rhs } => {
                let mut out = self.loans_of(lhs, cx);
                for l in self.loans_of(rhs, cx) {
                    if !out.contains(&l) {
                        out.push(l);
                    }
                }
                out
            }
            // Elements taken out of a vector keep pointing where the vector's elements point.
            HExprKind::Pop(p) => cx.locals[p.local].loans.clone(),
            HExprKind::Take(p) | HExprKind::Replace(p, _) => cx.locals[p.local].loans.clone(),
            // `get` borrows the map's slot; `getCopy` copies a value that keeps the elements' loans.
            HExprKind::MapRead { op: MapRead::Get, recv, .. } => self.storage_loans(recv, cx),
            HExprKind::MapRead { recv, .. } => self.loans_of(recv, cx),
            HExprKind::MapWrite { op: MapWrite::GetMut, place, .. } => {
                let mut out = vec![Loan::Local(place.local)];
                out.extend(cx.locals[place.local].loans.iter().copied());
                out
            }
            HExprKind::MapWrite { place, .. } => cx.locals[place.local].loans.clone(),
            HExprKind::Method { method: Builtin::MapClone, recv } if h.ty.carries_borrow() => self.loans_of(recv, cx),
            HExprKind::MapLit(entries) => {
                let mut out = Vec::new();
                for (k, v) in entries {
                    for l in self.loans_of(k, cx).into_iter().chain(self.loans_of(v, cx)) {
                        if !out.contains(&l) {
                            out.push(l);
                        }
                    }
                }
                out
            }
            // `&v[i]` points into `v`'s storage (and into whatever `v`'s elements point into).
            HExprKind::Index { base, .. } if matches!(h.ty, Ty::Ref(_)) => self.storage_loans(base, cx),
            HExprKind::StrView(inner) => recv_loans(inner),
            HExprKind::Method { method: Builtin::Trim | Builtin::Fields | Builtin::Lines, recv } => recv_loans(recv),
            // A cloned `Vec[str]` owns new storage, but its elements still point into the same text.
            HExprKind::Method { method: Builtin::VecClone, recv } if h.ty.carries_borrow() => self.loans_of(recv, cx),
            HExprKind::Index { base, .. } if h.ty.carries_borrow() => self.loans_of(base, cx),
            HExprKind::VecLit(items) | HExprKind::TupleLit(items) => {
                let mut out = Vec::new();
                for i in items {
                    for l in self.loans_of(i, cx) {
                        if !out.contains(&l) {
                            out.push(l);
                        }
                    }
                }
                out
            }
            // A tuple element that carries a borrow points where the tuple points.
            HExprKind::Field { base, .. } if h.ty.carries_borrow() => self.loans_of(base, cx),
            HExprKind::Call { func, args } if h.ty.carries_borrow() => {
                let picked: Vec<usize> = match &self.sigs[*func].ret_of {
                    Some(p) => p.clone(),
                    None => (0..args.len()).collect(),
                };
                let mut out = Vec::new();
                for i in picked {
                    if let Some(a) = args.get(i) {
                        for l in recv_loans(a) {
                            if !out.contains(&l) {
                                out.push(l);
                            }
                        }
                    }
                }
                out
            }
            _ => Vec::new(),
        }
    }

    /// What a borrow of `h`'s own storage points into: the variable it lives in (a variable,
    /// a field of one, an element of one), or what the reference it is read through points
    /// into, or a temporary.
    fn storage_loans(&self, h: &HExpr, cx: &FnCx) -> Vec<Loan> {
        match &h.kind {
            HExprKind::Local(id) => {
                let mut out = vec![Loan::Local(*id)];
                out.extend(cx.locals[*id].loans.iter().copied());
                out
            }
            HExprKind::Deref(inner) => self.loans_of(inner, cx),
            HExprKind::Field { base, .. } => self.storage_loans(base, cx),
            HExprKind::Unbox(base) => self.storage_loans(base, cx),
            HExprKind::Index { base, .. } => self.storage_loans(base, cx),
            _ => vec![Loan::Temp],
        }
    }

    fn outlives(cx: &FnCx, s: LocalId, v: LocalId) -> bool {
        let (a, b) = (&cx.locals[s], &cx.locals[v]);
        a.depth < b.depth || (a.depth == b.depth && s < v)
    }

    /// Records that variable `v` now points into `loans`; each source must live at least as
    /// long as `v` (semantic table E4).
    fn store_loans(&mut self, cx: &mut FnCx, v: LocalId, loans: Vec<Loan>, span: Span, replace: bool) {
        for l in &loans {
            match *l {
                Loan::Temp => {
                    self.diags
                        .error("E0305", span, "this value points into a temporary that is dropped at the end of the statement")
                        .note("store the owned value in a variable first, then take the view from it");
                }
                Loan::Local(s) if !Self::outlives(cx, s, v) => {
                    let (sn, vn) = (cx.locals[s].name.clone(), cx.locals[v].name.clone());
                    self.diags
                        .error("E0305", span, format!("`{sn}` does not live long enough: `{vn}` would still point into it after it is dropped"))
                        .note(format!("declare `{sn}` before `{vn}`, in the same or an enclosing block"));
                }
                Loan::Local(_) => {}
            }
        }
        // A loan of a temporary has been reported here; copies of `v` do not report it again.
        let loans: Vec<Loan> = loans.into_iter().filter(|l| *l != Loan::Temp).collect();
        if replace {
            cx.locals[v].loans = loans;
        } else {
            for l in loans {
                if !cx.locals[v].loans.contains(&l) {
                    cx.locals[v].loans.push(l);
                }
            }
        }
    }

    /// Moving or modifying `s` while a view into it is still used later is an error (A3).
    fn check_not_borrowed(&mut self, cx: &FnCx, s: LocalId, span: Span, what: &str) {
        let p = cx.cur;
        if cx.iterating.contains(&s) {
            let sn = cx.locals[s].name.clone();
            self.diags
                .error("E0102", span, format!("cannot {what} `{sn}` while a `for` loop is iterating over it"))
                .note("collect the changes in another variable and apply them after the loop");
            return;
        }
        for (v, lv) in cx.locals.iter().enumerate() {
            if v == s || !lv.loans.contains(&Loan::Local(s)) || !(lv.decl < p && p <= lv.scope_end) {
                continue;
            }
            if let Some(line) = cx.pre.live_after(lv, p) {
                let (sn, vn) = (cx.locals[s].name.clone(), lv.name.clone());
                self.diags
                    .error("E0102", span, format!("cannot {what} `{sn}` while `{vn}` still points into it"))
                    .note(format!("`{vn}` is used again on line {line}"));
                return;
            }
        }
    }

    /// Any use of `s` while an `&inout` into it is still used later is an error (A3): that
    /// reference has exclusive access.
    fn check_exclusive(&mut self, cx: &FnCx, s: LocalId, span: Span) {
        let name = cx.locals[s].name.clone();
        if cx.iterating_mut.contains(&s) {
            self.diags
                .error("E0102", span, format!("cannot use `{name}` inside `for ... in &{name}`: the loop has exclusive access to it"))
                .note("collect what you need before the loop, or use it after the loop");
            return;
        }
        let p = cx.cur;
        for (v, lv) in cx.locals.iter().enumerate() {
            if v == s || !lv.ty.contains_ref_mut() || !lv.loans.contains(&Loan::Local(s)) || !(lv.decl < p && p <= lv.scope_end) {
                continue;
            }
            // An `&inout` that was moved away no longer holds the access; its new owner does.
            if cx.moved.get(v).copied().flatten().is_some() {
                continue;
            }
            if let Some(line) = cx.pre.live_after(lv, p) {
                let vn = lv.name.clone();
                self.diags
                    .error("E0102", span, format!("cannot use `{name}` while `{vn}` has exclusive (`&inout`) access to it"))
                    .note(format!("`{vn}` is used again on line {line}"));
                return;
            }
        }
    }

    /// The value of `h` is moved: stored, returned, or passed to a `sink` parameter. A local of a
    /// non-copyable type is moved out and must not be used again.
    fn consume(&mut self, mut h: HExpr, cx: &mut FnCx) -> HExpr {
        if let HExprKind::Some(inner) = h.kind {
            h.kind = HExprKind::Some(Box::new(self.consume(*inner, cx)));
            return h;
        }
        if let HExprKind::Deref(_) = h.kind {
            if !h.ty.is_copy() {
                self.diags
                    .error("E0401", h.span, format!("cannot move a `{}` out of a reference", h.ty))
                    .note("make a copy with `.clone()`");
            }
            return h;
        }
        if let HExprKind::Field { .. } = h.kind {
            if !h.ty.is_copy() && h.ty != Ty::Error {
                self.diags
                    .error("E0401", h.span, format!("cannot move a `{}` out of a field", h.ty))
                    .note("take it and leave a default with `take(&x.field)`, copy it with `.clone()`, or only borrow it");
            }
            return h;
        }
        if let HExprKind::Unbox(_) = h.kind {
            if !h.ty.is_copy() && h.ty != Ty::Error {
                self.diags
                    .error("E0401", h.span, format!("cannot move a `{}` out of a `Box`", h.ty))
                    .note("move the whole box, `take` a field that has a default, or copy it with `.clone()`");
            }
            return h;
        }
        let HExprKind::Local(id) = h.kind else { return h };
        if h.ty.is_copy() || h.ty == Ty::Error {
            return h;
        }
        if matches!(cx.locals[id].kind, LocalKind::Param | LocalKind::InoutParam) {
            let name = cx.locals[id].name.clone();
            if cx.in_drop && name == "self" {
                self.diags.error("E0803", h.span, "a drop method cannot move `self` away");
                return h;
            }
            self.diags
                .error("E0401", h.span, format!("cannot move out of `{name}`: it belongs to the caller"))
                .note(format!("declare it as `{name}: sink {}` to take ownership, or pass `{name}.clone()`", h.ty));
            return h;
        }
        if cx.moved.len() <= id {
            cx.moved.resize(id + 1, None);
        }
        self.check_not_borrowed(cx, id, h.span, "move");
        cx.moved[id] = Some(h.span);
        h.kind = HExprKind::Move(id);
        h
    }

    /// A returned borrow may only point into parameters listed after `of` (O2).
    fn check_returned(&mut self, h: &HExpr, cx: &FnCx) {
        let Some(names) = &cx.ret_of else { return };
        for l in self.loans_of(h, cx) {
            match l {
                Loan::Temp => {
                    self.diags.error("E0303", h.span, "the returned value points into a temporary that is dropped here");
                }
                Loan::Local(id) => {
                    let local = &cx.locals[id];
                    let is_param = matches!(local.kind, LocalKind::Param | LocalKind::SinkParam | LocalKind::InoutParam);
                    if is_param && names.contains(&local.name) {
                        continue;
                    }
                    let d = self.diags.error(
                        "E0303",
                        h.span,
                        format!("the returned value points into `{}`, which the signature's `of` does not list", local.name),
                    );
                    if is_param {
                        d.note(format!("list it: `of {}`, or `of (a, b)` for several sources", local.name));
                    } else {
                        d.note("a local variable is dropped when the function returns; return an owned value instead");
                    }
                }
            }
        }
    }

    /// Checks a loop body. A value moved in one iteration must not be used by the next, so when
    /// the body moves something, it is checked a second time (diagnostics only) from the state
    /// at the end of the first iteration.
    fn loop_body(&mut self, body: &Block, cx: &mut FnCx, infinite: bool) -> Vec<HStmt> {
        let before = cx.moved.clone();
        let start_cur = cx.cur;
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
            let end_cur = cx.cur;
            cx.cur = start_cur;
            cx.moved = next.clone();
            cx.loops.push(LoopFlow::default());
            cx.loop_depth += 1;
            let _ = self.block(body, cx);
            cx.loop_depth -= 1;
            cx.cur = end_cur;
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
        cx.cur += 1;
        let kind = match &s.kind {
            StmtKind::Let { mutable, name, name_span, ty, init } => {
                let declared = ty.as_ref().map(|t| self.resolve_ty(t));
                cx.diverge_ok = Some(init.span);
                let init = self.expr(init, declared, cx);
                cx.diverge_ok = None;
                if init.ty == Ty::Unit && name != "_" {
                    self.diags.error("E0001", init.span, "this call returns nothing, so it cannot initialize a variable");
                }
                let (init, ty) = match declared {
                    Some(d) => (self.fit(init, d, cx), d),
                    None => {
                        let t = init.ty;
                        (init, t)
                    }
                };
                if name == "_" {
                    // `let _ = x` neither moves nor drops a variable; any other value is dropped now.
                    HStmtKind::Discard(init)
                } else {
                    let init = self.consume(init, cx);
                    let loans = if ty.carries_borrow() { self.loans_of(&init, cx) } else { Vec::new() };
                    let kind = if *mutable { LocalKind::Var } else { LocalKind::Let };
                    let local = self.declare(cx, name, *name_span, ty, kind);
                    if ty.carries_borrow() {
                        self.store_loans(cx, local, loans, init.span, true);
                    }
                    HStmtKind::Let { local, init }
                }
            }
            StmtKind::LetTuple { mutable, names, init } => {
                let init = self.expr(init, None, cx);
                let elems = match init.ty {
                    Ty::Tuple(t) => tuple_elems(t),
                    Ty::Error => return None,
                    other => {
                        self.diags.error("E0001", init.span, format!("`let (...)` needs a tuple, found `{other}`"));
                        return None;
                    }
                };
                if elems.len() != names.len() {
                    self.diags.error(
                        "E0001",
                        init.span,
                        format!("this tuple has {} but the pattern names {}", plural(elems.len(), "element"), names.len()),
                    );
                    return None;
                }
                let init = self.consume(init, cx);
                let loans = if init.ty.carries_borrow() { self.loans_of(&init, cx) } else { Vec::new() };
                let kind = if *mutable { LocalKind::Var } else { LocalKind::Let };
                let mut locals = Vec::new();
                for ((name, sp), ty) in names.iter().zip(elems) {
                    if name == "_" {
                        locals.push(None);
                        continue;
                    }
                    let local = self.declare(cx, name, *sp, ty, kind);
                    if ty.carries_borrow() {
                        self.store_loans(cx, local, loans.clone(), init.span, true);
                    }
                    locals.push(Some(local));
                }
                HStmtKind::LetTuple { locals, init }
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
                if cx.in_drop && name == "self" && fields_rev.is_empty() {
                    self.diags
                        .error("E0803", target.span, "a drop method cannot overwrite `self`")
                        .note("it may change fields, or `take`/`replace` a field that has a replacement value");
                }
                let Some(id) = self.lookup(cx, name) else {
                    self.diags.error("E0010", target.span, format!("cannot find variable `{name}` in this scope"));
                    self.expr(value, None, cx);
                    return None;
                };
                let root_ty = cx.locals[id].ty;
                // Writing through a reference or into the caller's place (an `inout` parameter),
                // or attempting to write a key, uses the variable.
                if matches!(root_ty, Ty::Ref(_) | Ty::RefMut(_)) && !fields_rev.is_empty()
                    || cx.locals[id].map_key
                    || cx.locals[id].kind == LocalKind::InoutParam
                {
                    cx.locals[id].used = true;
                }
                // `r.f = v` writes through a reference: a `&T` never allows it (A1), an `&inout T`
                // always does, whatever the binding itself was declared with (rule 3).
                let through = !fields_rev.is_empty() && matches!(root_ty, Ty::Ref(_) | Ty::RefMut(_));
                if cx.locals[id].map_key {
                    self.diags
                        .error("E0103", target.span, format!("cannot modify the Map key `{name}`"))
                        .note("remove the entry and insert it again under the new key");
                    self.expr(value, None, cx);
                    return None;
                }
                if through {
                    if let Ty::Ref(_) = root_ty {
                        self.diags
                            .error("E0101", target.span, format!("cannot modify through `{name}`: it is a read-only reference (`{root_ty}`)"))
                            .note("get a writable reference instead, e.g. with `getMut`");
                        self.expr(value, None, cx);
                        return None;
                    }
                    self.check_moved(cx, id, root.span);
                } else {
                match cx.locals[id].kind {
                    LocalKind::Var | LocalKind::InoutParam => {}
                    LocalKind::Let => {
                        self.diags
                            .error("E0014", target.span, format!("cannot assign to `{name}` because it was declared with `let`"))
                            .note(format!("declare it with `var {name}` to make it mutable"));
                    }
                    LocalKind::Param if cx.captures.contains(&id) => {
                        self.diags
                            .error("E0014", target.span, format!("a closure cannot assign to `{name}`, which it borrows from the enclosing function"))
                            .note("closures read the variables they use; compute the new value after the call instead");
                    }
                    LocalKind::Param | LocalKind::SinkParam => {
                        self.diags
                            .error("E0014", target.span, format!("cannot assign to parameter `{name}`; parameters are read-only"))
                            .note(format!("copy it into a local first: `var {name}2 = {name}`"));
                    }
                    LocalKind::LoopVar => {
                        self.diags.error("E0014", target.span, format!("cannot assign to loop variable `{name}`"));
                    }
                    LocalKind::Bind | LocalKind::BindMut | LocalKind::BindOwned => self.binding_write(cx, id, target.span),
                }
                }
                let lty = match root_ty {
                    Ty::RefMut(r) if through => ref_mut_inner(r),
                    t => t,
                };
                let Some((path, lty)) = self.field_path(lty, &fields_rev) else {
                    self.expr(value, None, cx);
                    return None;
                };
                let value = match op {
                    None => {
                        self.check_not_borrowed(cx, id, target.span, "assign to");
                        cx.diverge_ok = Some(value.span);
                        let v = self.expr(value, Some(lty), cx);
                        cx.diverge_ok = None;
                        let v = self.fit(v, lty, cx);
                        let v = self.consume(v, cx);
                        if cx.locals[id].kind == LocalKind::InoutParam && lty.carries_borrow() && !self.loans_of(&v, cx).is_empty() {
                            self.diags
                                .error("E0306", v.span, format!("cannot store a borrow into the caller's `{name}`"))
                                .note("return it instead, e.g. `-> Vec[str] of text`");
                        } else if path.is_empty() && lty.carries_borrow() {
                            let loans = self.loans_of(&v, cx);
                            self.store_loans(cx, id, loans, v.span, true);
                        }
                        if path.is_empty() && id < cx.moved.len() {
                            // Assigning a new value makes a moved-from variable usable again.
                            cx.moved[id] = None;
                        }
                        v
                    }
                    Some(op) => {
                        cx.locals[id].used = true;
                        self.check_not_borrowed(cx, id, target.span, "assign to");
                        let mut lhs =
                            HExpr { kind: HExprKind::Local(id), ty: cx.locals[id].ty, span: root.span, at: root.span };
                        if through {
                            lhs = self.deref(lhs);
                        }
                        let mut ty = lhs.ty;
                        for &fi in &path {
                            if fi == BOX_STEP {
                                lhs = self.unbox(lhs);
                                ty = lhs.ty;
                                continue;
                            }
                            let Ty::Struct(sid) = ty else { unreachable!() };
                            ty = self.structs[sid as usize].fields[fi].1;
                            lhs = HExpr { kind: HExprKind::Field { base: Box::new(lhs), field: fi }, ty, span: target.span, at: target.span };
                        }
                        let rhs = self.expr(value, Some(lty), cx);
                        // A copyable value read through `&T`/`&inout T` is copied (C4).
                        let rhs = self.coerce(rhs, lty, cx);
                        self.arith(*op, lhs, rhs, *op_span, s.span)
                    }
                };
                HStmtKind::Assign { local: id, path, value }
            }
            StmtKind::Expr(e) => {
                if matches!(e.kind, ExprKind::Try(_) | ExprKind::Catch { .. } | ExprKind::Match { .. }) {
                    cx.diverge_ok = Some(e.span);
                    let h = self.expr(e, None, cx);
                    cx.diverge_ok = None;
                    if let Ty::Res(_) = h.ty {
                        self.unhandled(&h);
                    }
                    return Some(HStmt { kind: HStmtKind::Expr(h), span: s.span });
                }
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
                let h = self.expr(e, None, cx);
                if let Ty::Res(_) = h.ty {
                    self.unhandled(&h);
                }
                HStmtKind::Expr(h)
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
            StmtKind::IfLet { name, name_span, value, then, els } => {
                let mut arms = vec![OptArm { pat: ArmPat::Some(name, *name_span), body: Body::Block(then), span: *name_span }];
                arms.push(OptArm {
                    pat: ArmPat::None,
                    body: match els {
                        None => Body::Empty,
                        Some(Else::Block(b)) => Body::Block(b),
                        Some(Else::If(s)) => Body::Stmt(s),
                    },
                    span: s.span,
                });
                self.match_opt(value, &arms, "if let", s.span, cx)?
            }
            StmtKind::Match { value, arms } => {
                if simple_opt_arms(arms.iter().map(|a| &a.pat)) {
                    let arms: Vec<OptArm> = arms
                        .iter()
                        .map(|a| OptArm {
                            pat: match &a.pat {
                                Pattern::Some(inner) => match &**inner {
                                    Pattern::Bind { name, span } => ArmPat::Some(name, *span),
                                    _ => ArmPat::Some("_", a.span),
                                },
                                Pattern::None => ArmPat::None,
                                _ => ArmPat::Wild,
                            },
                            body: Body::Block(&a.body),
                            span: a.span,
                        })
                        .collect();
                    self.match_opt(value, &arms, "match", s.span, cx)?
                } else {
                    let garms: Vec<GArm> =
                        arms.iter().map(|a| GArm { pat: &a.pat, span: a.span, body: GBody::Block(&a.body) }).collect();
                    let (scrut, owned, arms, _) = self.match_general(value, &garms, s.span, None, None, cx)?;
                    HStmtKind::Match { scrut, owned, arms }
                }
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
            StmtKind::ForIn { var, var_span, var2, iter, body } => self.for_in((var, *var_span), var2.as_ref(), iter, body, s.span, cx)?,
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
                HStmtKind::Return(self.return_value(value.as_ref(), s.span, cx))
            }
            StmtKind::Block(b) => HStmtKind::Block(self.block(b, cx)),
        };
        Some(HStmt { kind, span: s.span })
    }

    /// The checked value of `return [value]`.
    fn return_value(&mut self, value: Option<&Expr>, span: Span, cx: &mut FnCx) -> Option<HExpr> {
        let ret = cx.ret;
        // `return` or `return ()` in a function returning `() ! E` is a success.
        if let Ty::Res(r) = ret {
            if res_parts(r).0 == Ty::Unit && value.is_none_or(|e| matches!(e.kind, ExprKind::Unit)) {
                return Some(HExpr { kind: HExprKind::Ok(None), ty: ret, span, at: span });
            }
        }
        match value {
            Some(e) if matches!(e.kind, ExprKind::Unit) && ret == Ty::Unit => None,
            None => {
                if ret != Ty::Unit && ret != Ty::Error {
                    self.diags.error("E0001", span, format!("this function must return a value of type `{ret}`"));
                }
                None
            }
            Some(e) if ret == Ty::Unit => {
                self.diags
                    .error("E0001", e.span, "this function has no return type, so `return` cannot carry a value")
                    .note("add `-> T` to the function signature");
                self.expr(e, None, cx);
                None
            }
            Some(e) => {
                let h = self.expr(e, Some(ret), cx);
                // `return self.name` in a function returning `str`: a view of the stored text
                // (its `of` is checked below); a temporary `String` cannot be viewed.
                let h = if ret == Ty::Str
                    && h.ty == Ty::String
                    && matches!(h.kind, HExprKind::Local(_) | HExprKind::Field { .. } | HExprKind::Deref(_) | HExprKind::Unbox(_))
                {
                    let (span, at) = (h.span, h.at);
                    HExpr { kind: HExprKind::StrView(Box::new(h)), ty: Ty::Str, span, at }
                } else {
                    h
                };
                let h = self.fit(h, ret, cx);
                let h = self.consume(h, cx);
                if ret.carries_borrow() {
                    self.check_returned(&h, cx);
                }
                Some(h)
            }
        }
    }

    /// `for x in v`, `for i, x in v`, `for k, v in m`, and `in &v` / `in &m` (A2, A5).
    #[allow(clippy::too_many_arguments)]
    fn for_in(
        &mut self,
        first: (&String, Span),
        second: Option<&(String, Span)>,
        iter: &Expr,
        body: &Block,
        span: Span,
        cx: &mut FnCx,
    ) -> Option<HStmtKind> {
        let stmt_idx = cx.cur;
        let iterable = |t: Ty| matches!(t, Ty::Vec(_) | Ty::Map(_) | Ty::Slice(_));
        let not_iterable = |this: &mut Self, t: Ty, sp: Span| {
            this.diags
                .error("E0001", sp, format!("cannot iterate over a value of type `{t}`"))
                .note("iterate over a `Vec[T]`, a `Map[K, V]`, or a range `a..b`");
        };
        let (mutable, target) = match &iter.kind {
            ExprKind::Borrow(e) => (true, &**e),
            _ => (false, iter),
        };
        let mut prelude = None;
        // Variables that must not change while the loop runs, besides the collection variable.
        let mut guarded: Vec<LocalId> = Vec::new();
        let coll = if mutable {
            // `in &c` needs a writable variable; the loop then has exclusive access to it.
            let id = self.mutable_receiver(target, "for ... in &", cx)?;
            match cx.locals[id].ty {
                Ty::RefMut(_) => {
                    self.diags.error("U0001", iter.span, "iterating through an `&inout` reference is not supported yet");
                    return None;
                }
                Ty::Error => return None,
                Ty::Slice(_) => {
                    self.diags
                        .error("E0101", iter.span, "a slice (`[]T`) is read-only, so `for ... in &` cannot modify its elements")
                        .note("iterate over the vector it views instead");
                    return None;
                }
                t if !iterable(t) => {
                    not_iterable(self, t, iter.span);
                    return None;
                }
                _ => id,
            }
        } else {
            let it = self.expr(target, None, cx);
            let it = self.deref(it);
            let it = self.unbox(it);
            if it.ty == Ty::Error {
                return None;
            }
            if !iterable(it.ty) {
                not_iterable(self, it.ty, iter.span);
                return None;
            }
            match it.kind {
                HExprKind::Local(id) => id,
                HExprKind::Deref(_) | HExprKind::Field { .. } | HExprKind::Unbox(_) => {
                    // An existing place (a field, or a collection read through a `&`) is
                    // iterated through a hidden `&` to it; what it lives in must not change.
                    let loans = self.storage_loans(&it, cx);
                    guarded = loans.iter().filter_map(|l| if let Loan::Local(v) = l { Some(*v) } else { None }).collect();
                    let rty = Ty::Ref(ref_of(it.ty));
                    let (isp, at) = (it.span, it.at);
                    let init = HExpr { kind: HExprKind::AddrOf(Box::new(it)), ty: rty, span: isp, at };
                    let hidden = format!("$iter{}", cx.locals.len());
                    let id = self.declare(cx, &hidden, iter.span, rty, LocalKind::Let);
                    cx.locals[id].used = true;
                    self.store_loans(cx, id, loans, iter.span, true);
                    prelude = Some(HStmt { kind: HStmtKind::Let { local: id, init }, span });
                    id
                }
                _ => {
                    // A temporary is kept in a hidden variable until the loop ends.
                    let loans = self.loans_of(&it, cx);
                    let it = self.consume(it, cx);
                    let hidden = format!("$iter{}", cx.locals.len());
                    let id = self.declare(cx, &hidden, iter.span, it.ty, LocalKind::Let);
                    cx.locals[id].used = true;
                    // Views of a temporary would dangle: it is dropped before the loop runs.
                    self.store_loans(cx, id, loans, iter.span, true);
                    prelude = Some(HStmt { kind: HStmtKind::Let { local: id, init: it }, span });
                    id
                }
            }
        };
        let coll_ty = match cx.locals[coll].ty {
            Ty::Ref(r) => ref_inner(r),
            t => t,
        };
        // A slice variable views another variable's elements: that one must not change either.
        if let Ty::Slice(_) = coll_ty {
            guarded.extend(cx.locals[coll].loans.iter().filter_map(|l| if let Loan::Local(v) = l { Some(*v) } else { None }));
        }
        let mut ref_loans = vec![Loan::Local(coll)];
        ref_loans.extend(cx.locals[coll].loans.iter().copied());
        let elem_loans = cx.locals[coll].loans.clone();
        let end = cx.pre.bind_end.get(&(stmt_idx, 0)).copied();
        // Elements: `&inout T` for `in &c`; otherwise copies of copyable values, else `&T`.
        let access = |t: Ty| {
            if mutable {
                Ty::RefMut(ref_mut_of(t))
            } else if t.is_copy() {
                t
            } else {
                Ty::Ref(ref_of(t))
            }
        };
        cx.scopes.push(Vec::new());
        let bind = |this: &mut Self, cx: &mut FnCx, (name, sp): (&String, Span), ty: Ty| {
            let x = this.declare(cx, name, sp, ty, LocalKind::LoopVar);
            if let Some(e) = end {
                cx.locals[x].scope_end = e;
            }
            if matches!(ty, Ty::Ref(_) | Ty::RefMut(_)) {
                cx.locals[x].loans = ref_loans.clone();
            } else if ty.carries_borrow() {
                cx.locals[x].loans = elem_loans.clone();
            }
            x
        };
        enum Shape {
            Vec { index: Option<LocalId>, elem: LocalId, by_ref: bool },
            Map { key: LocalId, key_ref: bool, val: LocalId, val_ref: bool },
        }
        let shape = match coll_ty {
            Ty::Vec(_) | Ty::Slice(_) => {
                let (index, x) = match second {
                    Some((n2, s2)) => (Some(first), (n2, *s2)),
                    None => (None, first),
                };
                let index = index.map(|(n, sp)| self.declare(cx, n, sp, Ty::Int(IntTy::I64), LocalKind::LoopVar));
                let xty = access(seq_elem(coll_ty).expect("a vector or slice"));
                let elem = bind(self, cx, x, xty);
                Shape::Vec { index, elem, by_ref: matches!(xty, Ty::Ref(_) | Ty::RefMut(_)) }
            }
            Ty::Map(mid) => {
                let Some((n2, s2)) = second else {
                    self.diags
                        .error("E0001", first.1, "a `Map` is iterated with two variables: `for k, v in m`")
                        .note("use `_k` or `_v` for a part you do not need");
                    self.pop_scope(cx);
                    return None;
                };
                let (k, v) = map_parts(mid);
                // Keys are read-only in both forms (A5).
                let kty = if k.is_copy() { k } else { Ty::Ref(ref_of(k)) };
                let key = bind(self, cx, first, kty);
                cx.locals[key].map_key = true;
                let vty = access(v);
                let val = bind(self, cx, (n2, *s2), vty);
                Shape::Map { key, key_ref: !k.is_copy(), val, val_ref: matches!(vty, Ty::Ref(_) | Ty::RefMut(_)) }
            }
            _ => unreachable!("checked: a Vec or a Map"),
        };
        cx.iterating.push(coll);
        let n_guarded = guarded.len();
        cx.iterating.extend(guarded);
        if mutable {
            cx.iterating_mut.push(coll);
        }
        let body = self.loop_body(body, cx, false);
        if mutable {
            cx.iterating_mut.pop();
        }
        for _ in 0..=n_guarded {
            cx.iterating.pop();
        }
        self.pop_scope(cx);
        let kind = match shape {
            Shape::Vec { index, elem, by_ref } => HStmtKind::ForEach { vec: coll, index, elem, by_ref, body },
            Shape::Map { key, key_ref, val, val_ref } => HStmtKind::ForMap { map: coll, key, key_ref, val, val_ref, body },
        };
        Some(match prelude {
            Some(p) => HStmtKind::Block(vec![p, HStmt { kind, span }]),
            None => kind,
        })
    }

    /// `if let` and `match` on an optional (semantic table C5, C7). Arms are checked in source
    /// order, so statement numbers stay in step with `Pre`.
    fn match_opt(&mut self, value: &Expr, arms: &[OptArm], what: &str, span: Span, cx: &mut FnCx) -> Option<HStmtKind> {
        let stmt_idx = cx.cur;
        let scrut = self.expr(value, None, cx);
        let opt_ty = match scrut.ty {
            Ty::Ref(r) => ref_inner(r),
            t => t,
        };
        let mut inner = match opt_ty {
            Ty::Opt(o) => opt_inner(o),
            Ty::Error => Ty::Error,
            other => {
                self.diags
                    .error("E0001", scrut.span, format!("`{what}` needs an optional value (`?T`), found `{other}`"))
                    .note("only optional values can be matched for now");
                Ty::Error
            }
        };
        // An existing place (a variable, its fields, or something read through a `&`) is only
        // borrowed; an owned temporary gives its payload to the binding.
        let via_ref = matches!(scrut.ty, Ty::Ref(_));
        let root = place_root(&scrut);
        let mode = match (via_ref || root.is_some(), inner.is_copy()) {
            (true, true) => BindMode::Copy,
            (true, false) => BindMode::Ref,
            (false, _) => BindMode::Owned,
        };
        if mode == BindMode::Ref && inner.contains_ref_mut() {
            self.diags.error("U0001", scrut.span, format!("matching a stored `{opt_ty}` is not supported yet"));
            // Checked on as an error type, so the arms produce no follow-on diagnostics.
            inner = Ty::Error;
        }
        let scrut = if mode == BindMode::Owned { self.consume(scrut, cx) } else { scrut };
        let (bind_ty, kind) = match mode {
            _ if inner == Ty::Error => (Ty::Error, LocalKind::Bind),
            BindMode::Owned => (inner, LocalKind::BindOwned),
            _ => {
                let writable = !via_ref
                    && root.is_some_and(|r| matches!(cx.locals[r].kind, LocalKind::Var | LocalKind::InoutParam));
                // A binding into a writable place may be written through (rule 3, T-A3): it is an
                // `&inout` with exclusive access while it is used.
                let ty = match mode {
                    BindMode::Ref if writable => Ty::RefMut(ref_mut_of(inner)),
                    BindMode::Ref => Ty::Ref(ref_of(inner)),
                    _ => inner,
                };
                (ty, if writable { LocalKind::BindMut } else { LocalKind::Bind })
            }
        };
        // A binding into an existing place borrows it, even when the payload is copied: the
        // place must not change while the binding is still used (A2, C7).
        let loans = match (mode, root) {
            (BindMode::Ref | BindMode::Copy, Some(r)) if !via_ref => {
                let mut l = vec![Loan::Local(r)];
                l.extend(cx.locals[r].loans.iter().copied());
                l
            }
            _ if bind_ty.carries_borrow() => self.loans_of(&scrut, cx),
            _ => Vec::new(),
        };

        // Which arm handles `some`, which handles `none`; `_` takes whatever is left.
        #[derive(Clone, Copy, PartialEq)]
        enum Role {
            Some,
            None,
            Both,
            Unreachable,
        }
        let (mut has_some, mut has_none) = (false, false);
        let roles: Vec<Role> = arms
            .iter()
            .map(|a| {
                let role = match a.pat {
                    ArmPat::Some(..) if !has_some => Role::Some,
                    ArmPat::None if !has_none => Role::None,
                    ArmPat::Wild if !has_some && !has_none => Role::Both,
                    ArmPat::Wild if !has_some => Role::Some,
                    ArmPat::Wild if !has_none => Role::None,
                    _ => Role::Unreachable,
                };
                has_some |= matches!(role, Role::Some | Role::Both);
                has_none |= matches!(role, Role::None | Role::Both);
                role
            })
            .collect();
        let exhaustive = has_some && has_none;
        if !exhaustive && inner != Ty::Error {
            let missing = if has_some { "none" } else { "some(_)" };
            self.diags
                .error("E0602", span, format!("this `match` does not handle `{missing}`"))
                .note(format!("add a `{missing} => ...` arm, or `_ => ...` for every remaining case"));
        }

        let before = cx.moved.clone();
        let mut local = None;
        let (mut then_h, mut els_h) = (Vec::new(), Vec::new());
        let mut both_h = None;
        let mut ends: Vec<(Moved, bool)> = Vec::new();
        for (i, (arm, role)) in arms.iter().zip(&roles).enumerate() {
            cx.moved = before.clone();
            if *role == Role::Unreachable {
                self.diags
                    .warn("W0002", arm.span, "this arm can never run: an earlier arm already handles its case")
                    .note("remove it");
            }
            let out = if *role == Role::Some {
                cx.scopes.push(Vec::new());
                let name = match arm.pat {
                    ArmPat::Some(n, _) => n,
                    _ => "_",
                };
                let name_span = match arm.pat {
                    ArmPat::Some(_, sp) => sp,
                    _ => arm.span,
                };
                let x = self.declare(cx, name, name_span, bind_ty, kind);
                if let Some(&end) = cx.pre.bind_end.get(&(stmt_idx, i)) {
                    cx.locals[x].scope_end = end;
                }
                if name == "_" {
                    cx.locals[x].used = true;
                }
                if !loans.is_empty() || bind_ty.carries_borrow() {
                    self.store_loans(cx, x, loans.clone(), scrut.span, true);
                }
                local = Some(x);
                let out = match arm.body {
                    Body::Block(b) => self.stmts(&b.stmts, cx),
                    _ => unreachable!("an `if let` binds in its `then` block"),
                };
                self.pop_scope(cx);
                out
            } else {
                match arm.body {
                    Body::Block(b) => self.block(b, cx),
                    Body::Stmt(s) => self.stmt(s, cx).into_iter().collect(),
                    Body::Empty => Vec::new(),
                }
            };
            let div = match arm.body {
                Body::Block(b) => diverges(&b.stmts),
                Body::Stmt(s) => diverges(std::slice::from_ref(s)),
                Body::Empty => false,
            };
            match role {
                Role::Some => then_h = out,
                Role::None => els_h = out,
                Role::Both => both_h = Some(out),
                Role::Unreachable => continue,
            }
            ends.push((std::mem::take(&mut cx.moved), div));
        }
        // A branch that cannot fall through does not affect the code after the `match`.
        let live: Vec<&Moved> = ends.iter().filter(|(_, d)| !d).map(|(m, _)| m).collect();
        let merged: Vec<&Moved> = if live.is_empty() { ends.iter().map(|(m, _)| m).collect() } else { live };
        cx.moved = match merged.split_first() {
            Some((first, rest)) => rest.iter().fold((*first).clone(), |acc, m| union(&acc, m)),
            None => before,
        };
        if !exhaustive || inner == Ty::Error {
            return None;
        }
        if let Some(body) = both_h {
            // Only `_`: the value is not bound. A temporary is dropped before the arm runs.
            let mut out = vec![HStmt { kind: HStmtKind::Discard(scrut), span }];
            out.extend(body);
            return Some(HStmtKind::Block(out));
        }
        Some(HStmtKind::IfLet { scrut, mode, local: local.expect("a some arm"), then: then_h, els: els_h })
    }

    fn print(&mut self, name: &str, args: &[Expr], span: Span, cx: &mut FnCx) -> Option<HStmtKind> {
        let fd = if name == "eprintln" { 2 } else { 1 };
        let newline = name != "print";
        let parts = match args {
            [] if newline => Vec::new(),
            [arg] => self.print_parts(arg, cx),
            _ => {
                self.diags
                    .error("E0012", span, format!("`{name}` takes exactly one argument"))
                    .note("use interpolation to print several values: `println(\"${a} ${b}\")`");
                return None;
            }
        };
        Some(HStmtKind::Print { fd, newline, parts })
    }

    /// The pieces of a message: an interpolated string's text and values, or one value.
    fn print_parts(&mut self, arg: &Expr, cx: &mut FnCx) -> Vec<PrintPart> {
        let mut parts = Vec::new();
        match &arg.kind {
            ExprKind::Str(segs) => {
                for seg in segs {
                    match seg {
                        StrSeg::Text(t) => parts.push(PrintPart::Text(t.clone())),
                        StrSeg::Expr { expr, spec } => {
                            if spec.is_some() {
                                self.diags.error("U0001", expr.span, "format specifiers such as `${x:.2}` are not supported in P0");
                            }
                            let h = self.expr(expr, None, cx);
                            let h = self.deref(h);
                            self.printable(&h);
                            parts.push(PrintPart::Value(h));
                        }
                    }
                }
            }
            _ => {
                let h = self.expr(arg, None, cx);
                let h = self.deref(h);
                self.printable(&h);
                parts.push(PrintPart::Value(h));
            }
        }
        parts
    }

    /// A call whose result can fail was used as a statement without handling it.
    fn unhandled(&mut self, h: &HExpr) {
        self.diags
            .error("E0001", h.span, format!("the result of this call is not handled (`{}`)", h.ty))
            .note("handle it with `try`, `catch` or `match`, or discard it explicitly with `let _ = ...`");
    }

    fn printable(&mut self, h: &HExpr) {
        if h.ty == Ty::Unit {
            self.diags.error("E0001", h.span, "this call returns nothing, so there is nothing to print");
        }
        if let Ty::Res(_) = h.ty {
            self.diags
                .error("E0001", h.span, format!("a value that can fail (`{}`) cannot be printed directly", h.ty))
                .note("handle the error first with `try`, `catch` or `match`");
        }
        if let Ty::Struct(_) | Ty::Vec(_) | Ty::Map(_) | Ty::Slice(_) = h.ty {
            self.diags
                .error("E0001", h.span, format!("a value of type `{}` cannot be printed directly", h.ty))
                .note("print its parts instead, e.g. `println(\"${p.x} ${p.y}\")`");
        }
        if let Ty::Opt(_) = h.ty {
            self.diags
                .error("U0001", h.span, format!("printing an optional value (`{}`) is not supported yet", h.ty))
                .note("print the value inside with `if let`, or give a default with `??`");
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
        let hint = |t: Ty, fallback: Option<Ty>| if matches!(t, Ty::Int(_) | Ty::Enum(_)) { Some(t) } else { fallback };
        if is_untyped_literal(a) && !is_untyped_literal(b) {
            let hb = self.expr(b, expected, cx);
            let hb = self.deref(hb);
            let ha = self.expr(a, hint(hb.ty, expected), cx);
            (self.deref(ha), hb)
        } else {
            let ha = self.expr(a, expected, cx);
            let ha = self.deref(ha);
            let hb = self.expr(b, hint(ha.ty, expected), cx);
            (ha, self.deref(hb))
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
            ExprKind::Byte(b) => HExpr { kind: HExprKind::Int(*b as i128), ty: Ty::Int(IntTy::U8), span, at: span },
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
                    self.check_exclusive(cx, id, span);
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
                if let Some(ei) = self.enum_path(base, cx) {
                    return self.enum_case(ei, name, None, span, cx);
                }
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
                        if self.module_ref(tn, base.span) {
                            self.diags.error("U0001", span, format!("`{tn}.{name}` is not supported yet"));
                            return err_expr(span);
                        }
                    }
                }
                // A field of a struct value. A bare name that is not a variable (a type or module)
                // keeps the "not supported" diagnostic below.
                let named_non_local = matches!(&base.kind, ExprKind::Ident(n) if self.lookup(cx, n).is_none());
                if !named_non_local {
                    let b = self.expr(base, None, cx);
                    let b = self.deref(b);
                    let b = self.unbox(b);
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
                        Ty::Tuple(t) => {
                            let elems = tuple_elems(t);
                            return match name.parse::<usize>().ok().filter(|i| *i < elems.len()) {
                                Some(field) => {
                                    let ty = elems[field];
                                    HExpr { kind: HExprKind::Field { base: Box::new(b), field }, ty, span, at: span }
                                }
                                None => {
                                    self.diags.error(
                                        "E0010",
                                        span,
                                        format!("`{}` has no element `{name}` (elements are `.0` to `.{}`)", b.ty, elems.len() - 1),
                                    );
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
            ExprKind::Tuple(items) => {
                let exp = match expected {
                    Some(Ty::Tuple(t)) => Some(tuple_elems(t)).filter(|e| e.len() == items.len()),
                    _ => None,
                };
                let mut hs = Vec::new();
                let mut failed = false;
                for (i, it) in items.iter().enumerate() {
                    let et = exp.as_ref().map(|e| e[i]);
                    let h = self.expr(it, et, cx);
                    let h = match et {
                        Some(t) => self.fit(h, t, cx),
                        None => h,
                    };
                    // A mismatch was reported for the element; do not report the whole tuple too.
                    failed |= et.is_some_and(|t| h.ty != t);
                    if h.ty == Ty::Unit {
                        self.diags.error("E0001", h.span, "a tuple element cannot be `()`");
                    }
                    hs.push(self.consume(h, cx));
                }
                if failed || hs.iter().any(|h| matches!(h.ty, Ty::Error | Ty::Unit)) {
                    return err_expr(span);
                }
                let ty = Ty::Tuple(tuple_of(hs.iter().map(|h| h.ty).collect()));
                HExpr { kind: HExprKind::TupleLit(hs), ty, span, at: span }
            }
            ExprKind::Closure { .. } => {
                self.diags
                    .error("U0001", span, "closures can only be passed directly to `sortBy` or `update` for now")
                    .note("closures that are stored or passed to your own functions are not supported yet");
                err_expr(span)
            }
            ExprKind::MapLit(entries) => self.map_lit(entries, expected, span, cx),
            ExprKind::Index { base, index } => self.index(base, index, span, cx),
            // `&x` where a `&T` is expected (e.g. `a.cmp(&b)`): a read-only reference to the place.
            ExprKind::Borrow(inner) if matches!(expected, Some(Ty::Ref(_))) => {
                let Some(Ty::Ref(r)) = expected else { unreachable!() };
                let want = ref_inner(r);
                let h = self.expr(inner, Some(want), cx);
                let h = self.deref(h);
                if h.ty == Ty::Error {
                    return err_expr(span);
                }
                if h.ty != want {
                    self.diags.error("E0001", span, format!("mismatched types: expected `&{want}`, found `&{}`", h.ty));
                    return err_expr(span);
                }
                HExpr { kind: HExprKind::AddrOf(Box::new(h)), ty: Ty::Ref(r), span, at: span }
            }
            ExprKind::Borrow(_) => {
                self.diags.error("U0001", span, "borrow expressions (`&x`) are only supported as `inout` arguments in P0");
                err_expr(span)
            }
            ExprKind::Call { callee, args } => self.call(callee, args, expected, span, cx),
            ExprKind::None => match expected {
                Some(t @ Ty::Opt(_)) => HExpr { kind: HExprKind::None, ty: t, span, at: span },
                Some(Ty::Error) => err_expr(span),
                _ => {
                    self.diags
                        .error("E0001", span, "cannot tell which optional type this `none` has")
                        .note("give the variable a type, e.g. `let x: ?int = none`");
                    err_expr(span)
                }
            },
            ExprKind::Coalesce { op_span, lhs, rhs } => self.coalesce(lhs, rhs, *op_span, expected, span, cx),
            ExprKind::Return(_) | ExprKind::Break | ExprKind::Continue => {
                // The parser only produces these on the right side of `??`, which handles them.
                self.diags.error("E0020", span, "`return`, `break` and `continue` are statements here");
                err_expr(span)
            }
            ExprKind::Unit => {
                self.diags
                    .error("E0001", span, "`()` has no value to use here")
                    .note("`()` is only written as `return ()` or `ok(())` in a function returning `() ! E`");
                err_expr(span)
            }
            ExprKind::Case { name, args } => match self.case_target(expected) {
                Some(ei) => self.enum_case(ei, name, args.as_deref(), span, cx),
                None => {
                    if expected != Some(Ty::Error) {
                        self.diags
                            .error("E0001", span, format!("cannot tell which enum `.{name}` belongs to"))
                            .note(format!("write the enum name, e.g. `Kind.{name}`, or use it where the enum type is known"));
                    }
                    for a in args.iter().flatten() {
                        let v = match &a.kind {
                            ExprKind::Named { value, .. } => value,
                            _ => a,
                        };
                        self.expr(v, Some(Ty::Error), cx);
                    }
                    err_expr(span)
                }
            },
            ExprKind::Named { name_span, value, .. } => {
                self.diags.error("E0001", *name_span, "named arguments (`name: value`) are only used for enum case payloads");
                self.expr(value, None, cx);
                err_expr(span)
            }
            ExprKind::Err(v) => self.result_value(false, v, expected, span, cx),
            ExprKind::Try(v) => self.try_expr(v, span, cx),
            ExprKind::Catch { value, name, name_span, handler, block } => {
                self.catch_expr(value, name, *name_span, handler, *block, span, cx)
            }
            ExprKind::Match { value, arms } => {
                let garms: Vec<GArm> = arms.iter().map(|a| GArm { pat: &a.pat, span: a.span, body: GBody::Expr(&a.body) }).collect();
                match self.match_general(value, &garms, span, Some(span.start), expected, cx) {
                    Some((scrut, owned, arms, ty)) => {
                        if ty == Ty::Unit {
                            self.diags.error("E0001", span, "every arm of this `match` leaves, so it has no value");
                            return err_expr(span);
                        }
                        HExpr { kind: HExprKind::Match { scrut: Box::new(scrut), owned, arms }, ty, span, at: span }
                    }
                    None => err_expr(span),
                }
            }
            ExprKind::Block(_) => {
                self.diags.error("E0020", span, "a block cannot be used as a value here");
                err_expr(span)
            }
        }
    }

    /// `lhs ?? rhs` (semantic table V9): `lhs` is consumed as a whole; `rhs` runs only when it is
    /// `none`, and may leave with `return`, `break` or `continue`.
    #[allow(clippy::too_many_arguments)]
    fn coalesce(&mut self, lhs: &Expr, rhs: &Expr, at: Span, expected: Option<Ty>, span: Span, cx: &mut FnCx) -> HExpr {
        let allowed = cx.diverge_ok == Some(span);
        cx.diverge_ok = None;
        let hint = match expected {
            Some(t) if !matches!(t, Ty::Error | Ty::Unit) => Some(Ty::Opt(opt_of(t))),
            _ => None,
        };
        let l = self.expr(lhs, hint, cx);
        let l = self.deref(l);
        let l = self.consume(l, cx);
        let inner = match l.ty {
            Ty::Opt(o) => opt_inner(o),
            Ty::Error => Ty::Error,
            other => {
                self.diags
                    .error("E0001", l.span, format!("the left side of `??` must be an optional value (`?T`), found `{other}`"));
                Ty::Error
            }
        };
        let r = match &rhs.kind {
            ExprKind::Return(_) | ExprKind::Break | ExprKind::Continue => {
                if !allowed {
                    self.diags.error(
                        "U0001",
                        rhs.span,
                        "`?? return`, `?? break` and `?? continue` are only supported as a whole `let` initializer or assigned value for now",
                    );
                    return err_expr(span);
                }
                // The jumping path does not reach the code after this expression.
                let before = cx.moved.clone();
                let r = self.diverge(rhs, cx);
                cx.moved = before;
                r
            }
            _ => {
                cx.diverge_ok = if allowed { Some(rhs.span) } else { None };
                let r = self.expr(rhs, (inner != Ty::Error).then_some(inner), cx);
                cx.diverge_ok = None;
                let r = match r.ty {
                    Ty::Ref(x) if ref_inner(x) == inner && inner.is_copy() => self.deref(r),
                    _ => r,
                };
                if inner != Ty::Error {
                    self.expect_ty(&r, inner);
                }
                self.consume(r, cx)
            }
        };
        HExpr { kind: HExprKind::Coalesce { lhs: Box::new(l), rhs: Box::new(r) }, ty: inner, span, at }
    }

    /// `return [value]`, `break` or `continue` on the right side of `??`.
    fn diverge(&mut self, e: &Expr, cx: &mut FnCx) -> HExpr {
        let span = e.span;
        let kind = match &e.kind {
            ExprKind::Return(v) => HExprKind::Return(self.return_value(v.as_deref(), span, cx).map(Box::new)),
            _ => {
                let is_break = matches!(e.kind, ExprKind::Break);
                if cx.loop_depth == 0 {
                    let word = if is_break { "break" } else { "continue" };
                    self.diags.error("E0015", span, format!("`{word}` can only be used inside a loop"));
                    return err_expr(span);
                }
                let state = cx.moved.clone();
                if let Some(flow) = cx.loops.last_mut() {
                    union_into(if is_break { &mut flow.breaks } else { &mut flow.continues }, &state);
                }
                if is_break {
                    HExprKind::Break
                } else {
                    HExprKind::Continue
                }
            }
        };
        HExpr { kind, ty: Ty::Unit, span, at: span }
    }

    /// `some(value)`: moves `value` into a new optional.
    fn some_call(&mut self, args: &[Expr], expected: Option<Ty>, span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let [arg] = args else {
            self.diags.error("E0012", span, "`some` takes exactly one argument");
            return err_expr(span);
        };
        let exp_inner = match expected {
            Some(Ty::Opt(o)) => Some(opt_inner(o)),
            _ => None,
        };
        let h = self.expr(arg, exp_inner, cx);
        let h = match (h.ty, exp_inner) {
            (Ty::Ref(r), Some(i)) if ref_inner(r) == i && i.is_copy() => self.deref(h),
            _ => h,
        };
        match h.ty {
            Ty::Error => return err_expr(span),
            Ty::Unit => {
                self.diags.error("E0001", h.span, "this call returns nothing, so it cannot be wrapped in `some`");
                return err_expr(span);
            }
            _ => {}
        }
        let h = self.consume(h, cx);
        let ty = Ty::Opt(opt_of(h.ty));
        HExpr { kind: HExprKind::Some(Box::new(h)), ty, span, at }
    }

    /// `&place` given to `take`, `replace` or `swap`: a writable variable or field of one.
    fn place(&mut self, a: &Expr, what: &str, cx: &mut FnCx) -> Option<HPlace> {
        let ExprKind::Borrow(inner) = &a.kind else {
            self.diags.error("E0001", a.span, format!("`{what}` needs a place written with `&`, e.g. `{what}(&x)`"));
            return None;
        };
        if cx.in_drop && matches!(&inner.kind, ExprKind::Ident(n) if n == "self") {
            self.diags
                .error("E0803", a.span, format!("a drop method cannot `{what}` the whole `self`"))
                .note("it may `take` or `replace` a field that has a replacement value");
            return None;
        }
        self.place_expr(inner, what, cx)
    }

    /// The writable place `inner` names (after the `&`): a variable or a field chain rooted at
    /// one, looking through `&inout` and `Box` on the way.
    fn place_expr(&mut self, inner: &Expr, what: &str, cx: &mut FnCx) -> Option<HPlace> {
        let mut fields_rev: Vec<(&str, Span)> = Vec::new();
        let mut root: &Expr = inner;
        loop {
            match &root.kind {
                ExprKind::Field { base, name } => {
                    fields_rev.push((name.as_str(), root.span));
                    root = base;
                }
                ExprKind::Paren(e) => root = e,
                _ => break,
            }
        }
        let ExprKind::Ident(name) = &root.kind else {
            self.diags.error("U0001", inner.span, format!("only variables and their fields can be passed to `{what}` for now"));
            return None;
        };
        let Some(id) = self.lookup(cx, name) else {
            self.diags.error("E0010", root.span, format!("cannot find variable `{name}` in this scope"));
            return None;
        };
        cx.locals[id].used = true;
        self.check_moved(cx, id, root.span);
        let root_ty = cx.locals[id].ty;
        let through = !fields_rev.is_empty() && matches!(root_ty, Ty::Ref(_) | Ty::RefMut(_));
        if through {
            if let Ty::Ref(_) = root_ty {
                self.diags.error("E0101", root.span, format!("cannot modify through `{name}`: it is a read-only reference (`{root_ty}`)"));
                return None;
            }
        } else if !self.writable(cx, id, root.span) {
            return None;
        }
        let ty = match root_ty {
            Ty::RefMut(r) if through => ref_mut_inner(r),
            t => t,
        };
        let (path, ty) = self.field_path(ty, &fields_rev)?;
        self.check_not_borrowed(cx, id, inner.span, "modify");
        Some(HPlace { local: id, path, ty })
    }

    /// Walks a field chain (`fields_rev` lists the fields outermost first) from a value of
    /// type `ty`; a `Box` on the way is looked through (`BOX_STEP`). Reports a missing field.
    fn field_path(&mut self, mut ty: Ty, fields_rev: &[(&str, Span)]) -> Option<(Vec<usize>, Ty)> {
        let mut path = Vec::new();
        for (fname, fspan) in fields_rev.iter().rev() {
            while let Ty::Box(b) = ty {
                path.push(BOX_STEP);
                ty = box_inner(b);
            }
            match self.struct_field(ty, fname) {
                Some((fi, fty)) => {
                    path.push(fi);
                    ty = fty;
                }
                None => {
                    match ty {
                        Ty::Struct(_) => self.no_field(ty, fname, *fspan),
                        Ty::Error => {}
                        other => {
                            self.diags.error("E0001", *fspan, format!("a value of type `{other}` has no fields"));
                        }
                    }
                    return None;
                }
            }
        }
        Some((path, ty))
    }

    /// The content of a `Box` (of nested boxes) where a field, method or element is used (A7).
    fn unbox(&self, mut h: HExpr) -> HExpr {
        while let Ty::Box(b) = h.ty {
            let (span, at) = (h.span, h.at);
            h = HExpr { kind: HExprKind::Unbox(Box::new(h)), ty: box_inner(b), span, at };
        }
        h
    }

    fn box_borrow_error(&mut self, t: Ty, span: Span) {
        self.diags
            .error("E0301", span, format!("a `Box` cannot hold a `{t}`: a view cannot be stored in it"))
            .note("store owned values instead, e.g. `String` (call `.toString()` on each view)");
    }

    /// `Box(value)`: moves `value` into a new heap block.
    fn box_new(&mut self, args: &[Expr], expected: Option<Ty>, span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let [arg] = args else {
            self.diags.error("E0012", span, "`Box` takes exactly one argument, e.g. `Box(Node{key: 1})`");
            for a in args {
                self.expr(a, None, cx);
            }
            return err_expr(span);
        };
        let want = match expected {
            Some(Ty::Box(b)) => Some(box_inner(b)),
            Some(Ty::Opt(o)) => match opt_inner(o) {
                Ty::Box(b) => Some(box_inner(b)),
                _ => None,
            },
            _ => None,
        };
        let h = self.expr(arg, want, cx);
        let h = match want {
            Some(t) => self.coerce(h, t, cx),
            None => match h.ty {
                Ty::Ref(r) if ref_inner(r).is_copy() => self.deref(h),
                Ty::RefMut(r) if ref_mut_inner(r).is_copy() => self.deref(h),
                _ => h,
            },
        };
        match h.ty {
            Ty::Error => return err_expr(span),
            Ty::Unit => {
                self.diags.error("E0001", h.span, "`()` cannot be boxed");
                return err_expr(span);
            }
            t if t.carries_borrow() => {
                // Already reported when the expected type (`Box[str]`) was resolved.
                if expected != Some(Ty::Error) {
                    self.box_borrow_error(t, h.span);
                }
                return err_expr(span);
            }
            _ => {}
        }
        let h = self.consume(h, cx);
        let ty = Ty::Box(box_of(h.ty));
        HExpr { kind: HExprKind::BoxNew(Box::new(h)), ty, span, at }
    }

    /// Whether variable `id` may be modified; reports why not.
    fn writable(&mut self, cx: &FnCx, id: LocalId, span: Span) -> bool {
        let name = cx.locals[id].name.clone();
        if cx.locals[id].map_key {
            self.diags
                .error("E0103", span, format!("cannot modify the Map key `{name}`"))
                .note("remove the entry and insert it again under the new key");
            return false;
        }
        match cx.locals[id].kind {
            LocalKind::Var | LocalKind::InoutParam => return true,
            LocalKind::Let => {
                self.diags
                    .error("E0014", span, format!("cannot modify `{name}` because it was declared with `let`"))
                    .note(format!("declare it with `var {name}` to make it mutable"));
            }
            LocalKind::Param | LocalKind::SinkParam | LocalKind::LoopVar => {
                self.diags.error("E0014", span, format!("cannot modify `{name}`: parameters and loop variables are read-only"));
            }
            LocalKind::Bind | LocalKind::BindMut | LocalKind::BindOwned => self.binding_write(cx, id, span),
        }
        false
    }

    /// `take(&p)`, `replace(&p, v)`, `swap(&a, &b)` (stdlib contract §1).
    fn place_builtin(&mut self, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let want = if name == "take" { 1 } else { 2 };
        if args.len() != want {
            self.diags.error(
                "E0012",
                span,
                format!("`{name}` takes {} but {} {} supplied", plural(want, "argument"), args.len(), if args.len() == 1 { "was" } else { "were" }),
            );
            return err_expr(span);
        }
        match name {
            "take" => {
                let Some(p) = self.place(&args[0], name, cx) else { return err_expr(span) };
                if !p.ty.has_default() && p.ty != Ty::Error {
                    self.diags
                        .error("E0405", args[0].span, format!("`take` needs a type with a default value, and `{}` has none", p.ty))
                        .note("defaults exist for integers, bool, optionals, String and Vec; use `replace(&x, value)` or `swap(&a, &b)`");
                    return err_expr(span);
                }
                let ty = p.ty;
                HExpr { kind: HExprKind::Take(p), ty, span, at }
            }
            "replace" => {
                let Some(p) = self.place(&args[0], name, cx) else { return err_expr(span) };
                let root = cx.locals[p.local].name.clone();
                if mentions(&args[1], &root) {
                    self.diags
                        .error("E0102", args[1].span, format!("`{root}` is borrowed by the first argument of `replace`, so the new value cannot use it"))
                        .note(format!("compute the new value first: `let x = ...` then `replace(&{root}, x)`"));
                    return err_expr(span);
                }
                let v = self.expr(&args[1], Some(p.ty), cx);
                let v = self.fit(v, p.ty, cx);
                let v = self.consume(v, cx);
                if p.ty.carries_borrow() {
                    let loans = self.loans_of(&v, cx);
                    if cx.locals[p.local].kind == LocalKind::InoutParam && !loans.is_empty() {
                        self.diags
                            .error("E0306", v.span, format!("cannot store a borrow into the caller's `{root}`"))
                            .note("return it instead, e.g. `-> Vec[str] of text`");
                    } else {
                        self.store_loans(cx, p.local, loans, v.span, false);
                    }
                }
                let ty = p.ty;
                HExpr { kind: HExprKind::Replace(p, Box::new(v)), ty, span, at }
            }
            _ => {
                let a = self.place(&args[0], name, cx);
                let b = self.place(&args[1], name, cx);
                let (Some(a), Some(b)) = (a, b) else { return err_expr(span) };
                if a.ty != b.ty && a.ty != Ty::Error && b.ty != Ty::Error {
                    self.diags.error(
                        "E0001",
                        args[1].span,
                        format!("`swap` needs two places of the same type, found `{}` and `{}`", a.ty, b.ty),
                    );
                    return err_expr(span);
                }
                if a.local == b.local && (a.path.starts_with(&b.path) || b.path.starts_with(&a.path)) {
                    self.diags
                        .error("E0102", args[1].span, "the two places given to `swap` overlap")
                        .note("`swap` needs two different places");
                    return err_expr(span);
                }
                if a.ty.carries_borrow() {
                    let inout = |p: &HPlace| cx.locals[p.local].kind == LocalKind::InoutParam;
                    if inout(&a) || inout(&b) {
                        self.diags.error("U0001", span, "swapping values that point into other data through an `inout` parameter is not supported yet");
                        return err_expr(span);
                    }
                    let (la, lb) = (cx.locals[a.local].loans.clone(), cx.locals[b.local].loans.clone());
                    self.store_loans(cx, a.local, lb, args[1].span, false);
                    self.store_loans(cx, b.local, la, args[0].span, false);
                }
                HExpr { kind: HExprKind::Swap(a, b), ty: Ty::Unit, span, at }
            }
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
                if l.ty == r.ty && self.protocol(l.ty).is_some() {
                    return self.protocol_compare(op, l, r, span, at).expect("checked: the type has a protocol");
                }
                // Enums whose cases carry no payload (such as `Ordering`) compare by case.
                if let (Ty::Enum(id), true) = (l.ty, l.ty == r.ty && matches!(op, BinOp::Eq | BinOp::Ne)) {
                    let si = self.structs.iter().position(|s| s.ty == Ty::Enum(id)).expect("registered enum");
                    if self.structs[si].variants.iter().all(|v| v.fields.is_empty()) {
                        let eq = op == BinOp::Eq;
                        return HExpr { kind: HExprKind::SameTag { lhs: Box::new(l), rhs: Box::new(r), eq }, ty: Ty::Bool, span, at };
                    }
                }
                if l.ty != Ty::Error && r.ty != Ty::Error {
                    let ordering = !matches!(op, BinOp::Eq | BinOp::Ne);
                    if l.ty != r.ty {
                        let d = self.diags.error("E0001", span, format!("cannot compare `{}` with `{}`", l.ty, r.ty));
                        if let (Ty::Int(a), Ty::Int(_)) = (l.ty, r.ty) {
                            d.note(format!("convert one side explicitly, e.g. `{a}(...)`"));
                        }
                    } else if let Ty::Opt(_) = l.ty {
                        self.diags
                            .error("U0001", span, format!("comparing optional values (`{}`) is not supported yet", l.ty))
                            .note("use `if let`, `isSome()` or `??` instead");
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

    fn call(&mut self, callee: &Expr, args: &[Expr], expected: Option<Ty>, span: Span, cx: &mut FnCx) -> HExpr {
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
                // Prelude functions, unless the program defines a function with the same name.
                if !self.index.contains_key(name) {
                    match name.as_str() {
                        "some" => return self.some_call(args, expected, span, callee.span, cx),
                        "Box" => return self.box_new(args, expected, span, callee.span, cx),
                        "take" | "replace" | "swap" => return self.place_builtin(name, args, span, callee.span, cx),
                        "ok" => {
                            let [arg] = args else {
                                self.diags.error("E0012", span, "`ok` takes exactly one argument");
                                return err_expr(span);
                            };
                            return self.result_value(true, arg, expected, span, cx);
                        }
                        "panic" | "assert" => return self.panic_call(name, args, span, callee.span, cx),
                        "format" => {
                            let [arg] = args else {
                                self.diags.error("E0012", span, "`format` takes exactly one argument (an interpolated string)");
                                return err_expr(span);
                            };
                            let parts = self.print_parts(arg, cx);
                            return HExpr { kind: HExprKind::Format(parts), ty: Ty::String, span, at: callee.span };
                        }
                        "min" | "max" => return self.min_max(name == "max", args, expected, span, callee.span, cx),
                        "abs" => {
                            let [arg] = args else {
                                self.diags.error("E0012", span, "`abs` takes exactly one argument");
                                return err_expr(span);
                            };
                            let h = self.expr(arg, expected, cx);
                            let h = self.deref(h);
                            return match h.ty {
                                Ty::Int(t) if t.signed() => HExpr { kind: HExprKind::Abs(Box::new(h)), ty: Ty::Int(t), span, at: callee.span },
                                Ty::Int(_) => h,
                                Ty::Error => err_expr(span),
                                other => {
                                    self.diags.error("E0001", h.span, format!("`abs` needs an integer, found `{other}`"));
                                    err_expr(span)
                                }
                            };
                        }
                        _ => {}
                    }
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
                self.call_fn(fi, name, None, args, span, callee.span, cx)
            }
            ExprKind::Field { base, name } => self.method_call(base, name, args, span, callee.span, cx),
            _ => {
                self.diags.error("U0001", callee.span, "only calls to named functions are supported in P0");
                err_expr(span)
            }
        }
    }

    /// A call of user function `fi`. For a method call, `recv` is the receiver, already
    /// borrowed, moved or passed as `inout`; it is the first parameter and `args` the rest.
    fn call_fn(&mut self, fi: usize, name: &str, recv: Option<HExpr>, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        {
            let skip = usize::from(recv.is_some());
                let params = self.sigs[fi].params.clone();
                let ret = self.sigs[fi].ret;
                if args.len() + skip != params.len() {
                    let what = if skip == 1 { "method" } else { "function" };
                    self.diags.error(
                        "E0012",
                        span,
                        format!(
                            "{what} `{name}` takes {} but {} {} supplied",
                            plural(params.len() - skip, "argument"),
                            args.len(),
                            if args.len() == 1 { "was" } else { "were" }
                        ),
                    );
                }
                // Arguments are processed left to right; each is borrowed or (for a `sink`
                // parameter) moved before the next one is evaluated.
                let sinks = self.sigs[fi].sinks.clone();
                let mut borrowed: Vec<LocalId> = Vec::new();
                let inouts = self.sigs[fi].inouts.clone();
                let mut mut_borrowed: Vec<LocalId> = Vec::new();
                let mut hargs = Vec::new();
                if let Some(r) = recv {
                    match &r.kind {
                        HExprKind::InoutPlace(p) => mut_borrowed.push(p.local),
                        _ => {
                            if let Some(id) = place_root(&r) {
                                if !r.ty.is_copy() {
                                    borrowed.push(id);
                                }
                            }
                        }
                    }
                    hargs.push(r);
                }
                for (i, a) in args.iter().enumerate() {
                    let i = i + skip;
                    let exp = params.get(i).copied();
                    if inouts.get(i).copied().unwrap_or(false) {
                        let h = self.inout_arg(a, exp, &borrowed, &mut mut_borrowed, cx);
                        hargs.push(h);
                        continue;
                    }
                    let h = self.expr(a, exp, cx);
                    // A `&T` passed to a parameter of type `T` is read in place.
                    let h = if matches!(h.ty, Ty::Ref(_) | Ty::RefMut(_)) && !matches!(exp, Some(Ty::Ref(_) | Ty::RefMut(_))) {
                        self.deref(h)
                    } else {
                        h
                    };
                    // Passing a `String` to a `str` parameter borrows it for the call (whitelist).
                    let h = if exp == Some(Ty::Str) && h.ty == Ty::String {
                        let (span, at) = (h.span, h.at);
                        HExpr { kind: HExprKind::StrView(Box::new(h)), ty: Ty::Str, span, at }
                    } else {
                        h
                    };
                    // Passing a `Vec[T]` to a `[]T` parameter borrows it for the call (whitelist).
                    let h = match (exp, h.ty) {
                        (Some(Ty::Slice(s)), Ty::Vec(v)) if slice_elem(s) == vec_elem(v) => {
                            let (span, at) = (h.span, h.at);
                            HExpr { kind: HExprKind::SliceOf(Box::new(h)), ty: Ty::Slice(s), span, at }
                        }
                        _ => h,
                    };
                    if let HExprKind::StrView(inner) | HExprKind::SliceOf(inner) = &h.kind {
                        if let HExprKind::Local(id) = inner.kind {
                            borrowed.push(id);
                        }
                    }
                    let used = match &h.kind {
                        HExprKind::Local(id) => Some(*id),
                        HExprKind::StrView(x) | HExprKind::Deref(x) | HExprKind::SliceOf(x) => match x.kind {
                            HExprKind::Local(id) => Some(id),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(id) = used {
                        if mut_borrowed.contains(&id) {
                            let n = cx.locals[id].name.clone();
                            self.diags
                                .error("E0102", h.span, format!("`{n}` is passed as `inout` by an earlier argument, so it cannot be used here"))
                                .note("a variable passed as `inout` cannot be used by any other argument of the same call");
                        }
                    }
                    let h = match exp {
                        Some(t) => self.fit(h, t, cx),
                        None => h,
                    };
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
                HExpr { kind: HExprKind::Call { func: fi, args: hargs }, ty: ret, span, at }
        }
    }

    fn method_call(&mut self, base: &Expr, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        // `Type.f(args)`: a function written inside a struct or enum, without `self`.
        if let ExprKind::Ident(tn) = &base.kind {
            if self.lookup(cx, tn).is_none() {
                let full = format!("{tn}.{name}");
                if let Some(&fi) = self.index.get(&full) {
                    if self.sigs[fi].has_self {
                        self.diags
                            .error("E0001", at, format!("`{full}` is a method: call it on a value, e.g. `x.{name}(...)`"));
                        return err_expr(span);
                    }
                    return self.call_fn(fi, &full, None, args, span, at, cx);
                }
            }
        }
        // `x.method(args)` on a variable or a field of one.
        if let Some(t) = self.peek_place_ty(base, cx) {
            let mut t = match t {
                Ty::Ref(r) => ref_inner(r),
                t => t,
            };
            while let Ty::Box(b) = t {
                t = box_inner(b);
            }
            if let Some((fi, full)) = self.user_method(t, name, at) {
                return self.method_on_place(fi, &full, base, args, span, at, cx);
            }
        }
        if let Some(ei) = self.enum_path(base, cx) {
            return self.enum_case(ei, name, Some(args), span, cx);
        }
        if let ExprKind::Ident(tn) = &base.kind {
            if self.lookup(cx, tn).is_none() {
                if name == "wrap" {
                    if let Some(to) = IntTy::from_name(tn) {
                        return self.conversion(to, false, args, at, span, cx);
                    }
                }
                if self.module_ref(tn, base.span) {
                    if tn == "os" && name == "exit" {
                        return self.exit_call(args, span, at, cx);
                    }
                    if let Some(h) = self.module_call(tn, name, args, span, at, cx) {
                        return h;
                    }
                    self.diags.error("U0001", at, format!("`{tn}.{name}` is not supported yet"));
                    return err_expr(span);
                }
                self.diags.error("U0001", at, format!("`{tn}.{name}(...)` is not supported in P0"));
                return err_expr(span);
            }
        }
        if name == "push" {
            return self.push_call(base, args, span, at, cx);
        }
        if name == "pop" {
            return self.pop_call(base, args, span, at, cx);
        }
        if name == "sort" {
            return self.sort_call(base, args, span, at, cx);
        }
        if name == "sortBy" {
            return self.sort_by_call(base, args, span, at, cx);
        }
        if matches!(name, "set" | "remove" | "getMut" | "update") {
            if let Some(Ty::Map(mid)) = self.peek_place_ty(base, cx) {
                return self.map_write_call(base, mid, name, args, span, at, cx);
            }
        }
        let recv = self.expr(base, None, cx);
        let recv = self.deref(recv);
        let recv = self.unbox(recv);
        if let Some((fi, full)) = self.user_method(recv.ty, name, at) {
            if fi == NO_FN {
                return err_expr(span);
            }
            if self.sigs[fi].inouts[0] {
                self.diags
                    .error("E0014", base.span, format!("`{full}` takes `inout self`, so it must be called on a variable or a field of one"))
                    .note("store the value in a `var` first");
                return err_expr(span);
            }
            let recv = if self.sigs[fi].sinks[0] { self.consume(recv, cx) } else { recv };
            return self.call_fn(fi, &full, Some(recv), args, span, at, cx);
        }
        let recv = match self.method2(recv, name, args, span, at, cx) {
            Ok(h) => return h,
            Err(r) => r,
        };
        if let Ty::Map(mid) = recv.ty {
            match name {
                "get" | "getCopy" | "has" => return self.map_read(recv, mid, name, args, span, at, cx),
                "set" | "remove" | "getMut" | "update" => {
                    self.diags.error("U0001", at, format!("`{name}` is only supported on a variable for now"));
                    return err_expr(span);
                }
                _ => {}
            }
        }
        let (method, ty) = match (recv.ty, name) {
            (Ty::Str, "toString") => (Builtin::StrToString, Ty::String),
            (Ty::Str, "len") => (Builtin::StrLen, Ty::Int(IntTy::I64)),
            (Ty::String, "len") => (Builtin::StringLen, Ty::Int(IntTy::I64)),
            (Ty::String, "clone") => (Builtin::StringClone, Ty::String),
            (Ty::Vec(_), "len") => (Builtin::VecLen, Ty::Int(IntTy::I64)),
            (Ty::Slice(_), "len") => (Builtin::VecLen, Ty::Int(IntTy::I64)),
            (Ty::Vec(_), "clone") => (Builtin::VecClone, recv.ty),
            (Ty::Str | Ty::String, "trim") => (Builtin::Trim, Ty::Str),
            (Ty::Str | Ty::String, "fields") => (Builtin::Fields, Ty::Vec(vec_of(Ty::Str))),
            (Ty::Str | Ty::String, "lines") => (Builtin::Lines, Ty::Vec(vec_of(Ty::Str))),
            (Ty::Opt(_), "isSome") => (Builtin::IsSome, Ty::Bool),
            (Ty::Opt(_), "isNone") => (Builtin::IsNone, Ty::Bool),
            (Ty::Map(_), "len") => (Builtin::MapLen, Ty::Int(IntTy::I64)),
            (Ty::Map(_), "clone") => (Builtin::MapClone, recv.ty),
            (Ty::Error, _) => return err_expr(span),
            (t @ (Ty::Struct(_) | Ty::Enum(_)), m) => {
                self.diags
                    .error("E0010", at, format!("type `{t}` has no method `{m}`"))
                    .note("methods are written inside the struct or enum body: `fn name(self) { ... }`");
                for a in args {
                    self.expr(a, None, cx);
                }
                return err_expr(span);
            }
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

    /// The method `name` of a struct or enum type, with its full name `Type.name`. A function
    /// of the type without `self` is reported here and returned as `NO_FN`.
    fn user_method(&mut self, t: Ty, name: &str, at: Span) -> Option<(usize, String)> {
        if !matches!(t, Ty::Struct(_) | Ty::Enum(_)) {
            return None;
        }
        let tn = &self.structs.iter().find(|s| s.ty == t)?.name;
        let full = format!("{tn}.{name}");
        let &fi = self.index.get(&full)?;
        if name == "drop" && has_user_drop(t) {
            self.diags
                .error("E0801", at, format!("`{full}` is called by the compiler when the value is dropped; it cannot be called directly"))
                .note("to drop a value early, write `let _ = value`");
            return Some((NO_FN, full));
        }
        if !self.sigs[fi].has_self {
            self.diags
                .error("E0001", at, format!("`{full}` has no `self` parameter: call it on the type, as `{full}(...)`"));
            return Some((NO_FN, full));
        }
        Some((fi, full))
    }

    /// `place.method(args)`: the receiver is borrowed (`self`), passed as `inout` (`inout self`,
    /// which needs a writable place) or moved (`sink self`).
    fn method_on_place(&mut self, fi: usize, full: &str, base: &Expr, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        if fi == NO_FN {
            for a in args {
                self.expr(a, None, cx);
            }
            return err_expr(span);
        }
        let recv = if self.sigs[fi].inouts[0] {
            let Some(place) = self.mutable_place(base, full, cx) else {
                for a in args {
                    self.expr(a, None, cx);
                }
                return err_expr(span);
            };
            let ty = place.ty;
            HExpr { kind: HExprKind::InoutPlace(place), ty, span: base.span, at: base.span }
        } else {
            let h = self.expr(base, None, cx);
            let h = self.deref(h);
            let h = self.unbox(h);
            if self.sigs[fi].sinks[0] {
                self.consume(h, cx)
            } else {
                h
            }
        };
        self.call_fn(fi, full, Some(recv), args, span, at, cx)
    }

    /// `os.exit(code)`.
    fn exit_call(&mut self, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let [arg] = args else {
            self.diags.error("E0012", span, "`os.exit` takes exactly one argument (the exit status)");
            return err_expr(span);
        };
        let int = Ty::Int(IntTy::I64);
        let h = self.expr(arg, Some(int), cx);
        let h = self.fit(h, int, cx);
        HExpr { kind: HExprKind::Exit(Box::new(h)), ty: Ty::Unit, span, at }
    }

    /// The key argument of a map lookup: `String` and `str` keys are looked up with a `str`
    /// (C6), other keys by value.
    fn map_query(&mut self, arg: &Expr, key: Ty, cx: &mut FnCx) -> HExpr {
        let q = if matches!(key, Ty::String | Ty::Str) { Ty::Str } else { key };
        let h = self.expr(arg, Some(q), cx);
        let h = self.deref(h);
        let h = if q == Ty::Str && h.ty == Ty::String {
            let (span, at) = (h.span, h.at);
            HExpr { kind: HExprKind::StrView(Box::new(h)), ty: Ty::Str, span, at }
        } else {
            h
        };
        self.expect_ty(&h, q);
        h
    }

    /// `m.get(k)`, `m.getCopy(k)`, `m.has(k)` (stdlib contract §3).
    #[allow(clippy::too_many_arguments)]
    fn map_read(&mut self, recv: HExpr, mid: u32, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let (k, v) = map_parts(mid);
        let [arg] = args else {
            self.diags.error("E0012", span, format!("`{name}` takes exactly one argument (the key)"));
            return err_expr(span);
        };
        let key = self.map_query(arg, k, cx);
        let (op, ty) = match name {
            "get" => (MapRead::Get, Ty::Opt(opt_of(Ty::Ref(ref_of(v))))),
            "getCopy" => {
                if !v.is_copy() {
                    self.diags
                        .error("E0404", span, format!("`getCopy` needs a copyable value type, and `{v}` is not"))
                        .note("borrow the value with `get`, or clone it");
                    return err_expr(span);
                }
                (MapRead::GetCopy, Ty::Opt(opt_of(v)))
            }
            _ => (MapRead::Has, Ty::Bool),
        };
        HExpr { kind: HExprKind::MapRead { op, recv: Box::new(recv), key: Box::new(key) }, ty, span, at }
    }

    /// `m.set(k, v)`, `m.remove(k)`, `m.getMut(k)` on a writable map variable.
    #[allow(clippy::too_many_arguments)]
    fn map_write_call(&mut self, base: &Expr, mid: u32, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        if name == "update" {
            return self.update_call(base, mid, args, span, at, cx);
        }
        let Some(place) = self.mutable_place(base, name, cx) else { return err_expr(span) };
        let id = place.local;
        let local = cx.locals[id].name.clone();
        // v1 has no two-phase borrows: the map is borrowed before the arguments run (V2).
        if let Some(a) = args.iter().find(|a| mentions(a, &local)) {
            self.diags
                .error("E0403", a.span, format!("`{local}` is already borrowed by `{local}.{name}(...)`, so the argument cannot use it"))
                .note(format!("compute the value first: `let x = ...` then `{local}.{name}(x)`"));
            return err_expr(span);
        }
        let (k, v) = map_parts(mid);
        let want = if name == "set" { 2 } else { 1 };
        if args.len() != want {
            let what = if want == 2 { "a key and a value" } else { "the key" };
            self.diags.error("E0012", span, format!("`{name}` takes {} ({what})", plural(want, "argument")));
            return err_expr(span);
        }
        let (op, hargs, ty) = match name {
            "set" => {
                let kh = self.expr(&args[0], Some(k), cx);
                let kh = self.fit(kh, k, cx);
                let kh = self.consume(kh, cx);
                let vh = self.expr(&args[1], Some(v), cx);
                let vh = self.fit(vh, v, cx);
                let vh = self.consume(vh, cx);
                let mut loans = Vec::new();
                for h in [&kh, &vh] {
                    if h.ty.carries_borrow() {
                        loans.extend(self.loans_of(h, cx));
                    }
                }
                if !loans.is_empty() {
                    if matches!(cx.locals[id].ty, Ty::RefMut(_)) {
                        self.diags.error("U0001", span, "storing a borrowed value through an `&inout` reference is not supported yet");
                    } else if cx.locals[id].kind == LocalKind::InoutParam {
                        self.diags
                            .error("E0306", span, format!("cannot store a borrow into the caller's `{local}`"))
                            .note("return the map instead, e.g. `-> Map[str, int] of text`");
                    } else {
                        self.store_loans(cx, id, loans, span, false);
                    }
                }
                (MapWrite::Set, vec![kh, vh], Ty::Unit)
            }
            "remove" => (MapWrite::Remove, vec![self.map_query(&args[0], k, cx)], Ty::Opt(opt_of(v))),
            _ => (MapWrite::GetMut, vec![self.map_query(&args[0], k, cx)], Ty::Opt(opt_of(Ty::RefMut(ref_mut_of(v))))),
        };
        HExpr { kind: HExprKind::MapWrite { op, place, args: hargs }, ty, span, at }
    }

    /// The type of a variable or a field chain rooted at one, without reporting anything (an
    /// `&inout T` variable stands for its target).
    fn peek_place_ty(&self, base: &Expr, cx: &FnCx) -> Option<Ty> {
        match &base.kind {
            ExprKind::Ident(n) => self.lookup(cx, n).map(|id| Self::receiver_ty(cx, id)),
            ExprKind::Paren(e) => self.peek_place_ty(e, cx),
            ExprKind::Field { base: b, name } => {
                let t = match self.peek_place_ty(b, cx)? {
                    Ty::Ref(r) => ref_inner(r),
                    Ty::RefMut(r) => ref_mut_inner(r),
                    t => t,
                };
                let mut t = t;
                while let Ty::Box(bx) = t {
                    t = box_inner(bx);
                }
                self.struct_field(t, name).map(|(_, t)| t)
            }
            _ => None,
        }
    }

    /// The receiver of a modifying method (`push`, `pop`, `set`, ...): a writable variable or a
    /// field chain rooted at one. An `&inout T` variable stands for its target.
    fn mutable_place(&mut self, base: &Expr, method: &str, cx: &mut FnCx) -> Option<HPlace> {
        let mut fields_rev: Vec<(&str, Span)> = Vec::new();
        let mut root = base;
        loop {
            match &root.kind {
                ExprKind::Field { base, name } => {
                    fields_rev.push((name.as_str(), root.span));
                    root = base;
                }
                ExprKind::Paren(e) => root = e,
                _ => break,
            }
        }
        if fields_rev.is_empty() {
            let id = self.mutable_receiver(base, method, cx)?;
            let (mut path, mut ty) = (Vec::new(), Self::receiver_ty(cx, id));
            // A method of the content of a box (`b.push(x)` on a `Box[Vec[int]]`) works in place.
            while let Ty::Box(b) = ty {
                path.push(BOX_STEP);
                ty = box_inner(b);
            }
            return Some(HPlace { local: id, path, ty });
        }
        let ExprKind::Ident(name) = &root.kind else {
            self.diags.error("U0001", base.span, format!("`{method}` is only supported on a variable or a field of one for now"));
            return None;
        };
        let Some(id) = self.lookup(cx, name) else {
            self.diags.error("E0010", root.span, format!("cannot find variable `{name}` in this scope"));
            return None;
        };
        cx.locals[id].used = true;
        self.check_moved(cx, id, root.span);
        let root_ty = cx.locals[id].ty;
        match root_ty {
            Ty::Ref(_) => {
                self.diags
                    .error("E0101", root.span, format!("cannot modify through `{name}`: it is a read-only reference (`{root_ty}`)"))
                    .note("get a writable reference instead, e.g. with `getMut`");
                return None;
            }
            Ty::RefMut(_) => self.check_exclusive(cx, id, root.span),
            _ => {
                if !self.writable(cx, id, root.span) {
                    return None;
                }
            }
        }
        let (mut path, mut ty) = self.field_path(Self::receiver_ty(cx, id), &fields_rev)?;
        while let Ty::Box(b) = ty {
            path.push(BOX_STEP);
            ty = box_inner(b);
        }
        self.check_not_borrowed(cx, id, base.span, "modify");
        Some(HPlace { local: id, path, ty })
    }

    /// The variable a modifying method (`push`, `pop`, `set`, ...) is called on. An `&inout T`
    /// variable stands for its target, which it may always modify.
    fn mutable_receiver(&mut self, base: &Expr, method: &str, cx: &mut FnCx) -> Option<LocalId> {
        let ExprKind::Ident(name) = &base.kind else {
            self.diags.error("U0001", base.span, format!("`{method}` is only supported on a variable for now"));
            return None;
        };
        let Some(id) = self.lookup(cx, name) else {
            self.diags.error("E0010", base.span, format!("cannot find variable `{name}` in this scope"));
            return None;
        };
        cx.locals[id].used = true;
        self.check_moved(cx, id, base.span);
        self.check_not_borrowed(cx, id, base.span, "modify");
        if matches!(cx.locals[id].ty, Ty::RefMut(_)) || self.writable(cx, id, base.span) {
            Some(id)
        } else {
            None
        }
    }

    /// The type a receiver variable stands for: the target of an `&inout T`.
    fn receiver_ty(cx: &FnCx, id: LocalId) -> Ty {
        match cx.locals[id].ty {
            Ty::RefMut(r) => ref_mut_inner(r),
            t => t,
        }
    }

    /// `v.pop()`: the last element as an owned `?T`.
    fn pop_call(&mut self, base: &Expr, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let Some(place) = self.mutable_place(base, "pop", cx) else { return err_expr(span) };
        if !args.is_empty() {
            self.diags.error("E0012", span, "`pop` takes no arguments");
            return err_expr(span);
        }
        match place.ty {
            Ty::Vec(vid) => HExpr { kind: HExprKind::Pop(place), ty: Ty::Opt(opt_of(vec_elem(vid))), span, at },
            Ty::Error => err_expr(span),
            other => {
                self.diags.error("U0001", at, format!("method `pop` on `{other}` is not supported yet"));
                err_expr(span)
            }
        }
    }

    /// `v.push(x)` moves `x` into the vector; `s.push(t)` appends text to a `String`.
    fn push_call(&mut self, base: &Expr, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let Some(place) = self.mutable_place(base, "push", cx) else { return err_expr(span) };
        let id = place.local;
        let name = cx.locals[id].name.clone();
        let name = &name;
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
        let rty = place.ty;
        let value = match rty {
            Ty::Vec(vid) => {
                let et = vec_elem(vid);
                let h = self.expr(arg, Some(et), cx);
                let h = self.fit(h, et, cx);
                let h = self.consume(h, cx);
                if et.carries_borrow() {
                    let loans = self.loans_of(&h, cx);
                    if matches!(cx.locals[id].ty, Ty::RefMut(_)) && !loans.is_empty() {
                        self.diags.error("U0001", h.span, "storing a borrowed value through an `&inout` reference is not supported yet");
                    } else if cx.locals[id].kind == LocalKind::InoutParam && !loans.is_empty() {
                        self.diags
                            .error("E0306", h.span, format!("cannot store a borrow into the caller's `{name}`"))
                            .note("return the collected views instead, e.g. `-> Vec[str] of text`");
                    } else {
                        self.store_loans(cx, id, loans, h.span, false);
                    }
                }
                h
            }
            Ty::Ref(_) => {
                self.diags.error("E0014", base.span, format!("cannot modify through `{name}`: it is a read-only reference"));
                return err_expr(span);
            }
            Ty::String => {
                let h = self.expr(arg, None, cx);
                let h = self.deref(h);
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
        HExpr { kind: HExprKind::Push { place, value: Box::new(value) }, ty: Ty::Unit, span, at }
    }

    fn vec_lit(&mut self, items: &[Expr], expected: Option<Ty>, span: Span, cx: &mut FnCx) -> HExpr {
        if expected == Some(Ty::Error) && items.is_empty() {
            return err_expr(span);
        }
        let mut elem = match expected {
            Some(Ty::Vec(id)) => Some(vec_elem(id)),
            _ => None,
        };
        let mut out = Vec::new();
        for it in items {
            let h = self.expr(it, elem, cx);
            let h = match elem {
                Some(e) => self.fit(h, e, cx),
                None => h,
            };
            match elem {
                Some(_) => {}
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
        if elem.contains_ref_mut() {
            self.diags.error("U0001", span, "collections of `&inout` references are not supported yet");
            return err_expr(span);
        }
        HExpr { kind: HExprKind::VecLit(out), ty: Ty::Vec(vec_of(elem)), span, at: span }
    }

    /// `[:]` or `[k: v, ...]`; keys and values are evaluated and moved in the order written.
    fn map_lit(&mut self, entries: &[(Expr, Expr)], expected: Option<Ty>, span: Span, cx: &mut FnCx) -> HExpr {
        if expected == Some(Ty::Error) && entries.is_empty() {
            return err_expr(span);
        }
        let mut parts = match expected {
            Some(Ty::Map(id)) => Some(map_parts(id)),
            _ => None,
        };
        let mut out = Vec::new();
        for (k, v) in entries {
            let kh = self.expr(k, parts.map(|p| p.0), cx);
            let vh = self.expr(v, parts.map(|p| p.1), cx);
            let (kh, vh) = match parts {
                Some((kt, vt)) => (self.fit(kh, kt, cx), self.fit(vh, vt, cx)),
                None => {
                    if kh.ty != Ty::Error && vh.ty != Ty::Error {
                        parts = Some((kh.ty, vh.ty));
                    }
                    (kh, vh)
                }
            };
            let kh = self.consume(kh, cx);
            let vh = self.consume(vh, cx);
            out.push((kh, vh));
        }
        let Some((kt, vt)) = parts else {
            if entries.is_empty() {
                self.diags
                    .error("E0001", span, "cannot infer the key and value types of `[:]`")
                    .note("annotate it, e.g. `let m: Map[String, int] = [:]`");
            }
            return err_expr(span);
        };
        if kt == Ty::Error || vt == Ty::Error {
            return err_expr(span);
        }
        if !kt.is_hashable() {
            self.diags.error("E0501", span, format!("`{kt}` cannot be a Map key")).note("keys can be integers, bool, str or String");
            return err_expr(span);
        }
        if vt.contains_ref_mut() {
            self.diags.error("U0001", span, "collections of `&inout` references are not supported yet");
            return err_expr(span);
        }
        HExpr { kind: HExprKind::MapLit(out), ty: Ty::Map(map_of(kt, vt)), span, at: span }
    }

    fn index(&mut self, base: &Expr, index: &Expr, span: Span, cx: &mut FnCx) -> HExpr {
        if let ExprKind::Ident(n) = &base.kind {
            if self.lookup(cx, n).is_none()
                && (is_type_name(n) || UNSUPPORTED_TYPES.contains(&n.as_str()) || self.struct_index.contains_key(n))
            {
                self.mark_type_used(n);
                self.diags.error("U0001", span, "type arguments in expressions (`Name[T]`) are not supported in P0");
                return err_expr(span);
            }
        }
        let b = self.expr(base, None, cx);
        let b = self.deref(b);
        let b = self.unbox(b);
        let i = self.expr(index, None, cx);
        let elem = match (b.ty, i.ty) {
            (Ty::Error, _) | (_, Ty::Error) => return err_expr(span),
            (Ty::Vec(_) | Ty::Slice(_), Ty::Int(_)) => seq_elem(b.ty).expect("a vector or slice"),
            (Ty::Str | Ty::String, Ty::Int(_)) => Ty::Int(IntTy::U8),
            (Ty::Vec(_) | Ty::Slice(_) | Ty::Str | Ty::String, other) => {
                self.diags.error("E0001", i.span, format!("an index must be an integer, found `{other}`"));
                return err_expr(span);
            }
            (other, _) => {
                self.diags.error("E0001", b.span, format!("a value of type `{other}` cannot be indexed"));
                return err_expr(span);
            }
        };
        // A non-copyable element is read in place: `v[i]` is a `&T` into `v`.
        let ty = if elem.is_copy() { elem } else { Ty::Ref(ref_of(elem)) };
        HExpr { kind: HExprKind::Index { base: Box::new(b), index: Box::new(i) }, ty, span, at: span }
    }

    fn conversion(&mut self, to: IntTy, checked: bool, args: &[Expr], at: Span, span: Span, cx: &mut FnCx) -> HExpr {
        self.conversion_inner(to, checked, args, at, span, cx)
    }

    /// The enum a type path names: `Shape`, or `fs.Error` (a module's enum). Not a variable.
    fn enum_path(&mut self, base: &Expr, cx: &FnCx) -> Option<usize> {
        let name = match &base.kind {
            ExprKind::Ident(n) if self.lookup(cx, n).is_none() => n.clone(),
            ExprKind::Field { base: m, name } => match &m.kind {
                ExprKind::Ident(mn) if self.lookup(cx, mn).is_none() && MODULES.iter().any(|(x, _)| x == mn) => {
                    let full = format!("{mn}.{name}");
                    if !self.struct_index.contains_key(&full) {
                        return None;
                    }
                    self.module_ref(mn, m.span);
                    full
                }
                _ => return None,
            },
            _ => return None,
        };
        let &i = self.struct_index.get(&name)?;
        matches!(self.structs[i].ty, Ty::Enum(_)).then_some(i)
    }

    /// `Enum.Case`, `Enum.Case(field: value, ...)`, or the same with the enum taken from the
    /// context (`.Case(...)`). Payload values are moved into the new value in the order written.
    fn enum_case(&mut self, ei: usize, case: &str, args: Option<&[Expr]>, span: Span, cx: &mut FnCx) -> HExpr {
        let ty = self.structs[ei].ty;
        let ename = self.structs[ei].name.clone();
        let Some(vi) = self.structs[ei].variants.iter().position(|v| v.name == case) else {
            let names: Vec<String> = self.structs[ei].variants.iter().map(|v| v.name.clone()).collect();
            self.diags
                .error("E0010", span, format!("enum `{ename}` has no case `{case}`"))
                .note(format!("its cases are: {}", names.join(", ")));
            for a in args.unwrap_or(&[]) {
                let v = match &a.kind {
                    ExprKind::Named { value, .. } => value,
                    _ => a,
                };
                self.expr(v, None, cx);
            }
            return err_expr(span);
        };
        let decl: Vec<(String, Ty)> = self.structs[ei].variants[vi].fields.iter().map(|(n, t, _)| (n.clone(), *t)).collect();
        let list = |d: &[(String, Ty)]| d.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", ");
        let Some(args) = args else {
            if !decl.is_empty() {
                self.diags
                    .error("E0012", span, format!("case `{ename}.{case}` carries a payload ({})", list(&decl)))
                    .note(format!("write `{ename}.{case}({})`", decl.iter().map(|(n, _)| format!("{n}: ...")).collect::<Vec<_>>().join(", ")));
                return err_expr(span);
            }
            return HExpr { kind: HExprKind::EnumLit { variant: vi, fields: Vec::new() }, ty, span, at: span };
        };
        let mut seen = vec![false; decl.len()];
        let mut out = Vec::new();
        let mut ok = true;
        let mut next_pos = 0usize;
        for a in args {
            let (fi, value) = match &a.kind {
                ExprKind::Named { name, name_span, value } => match decl.iter().position(|(n, _)| n == name) {
                    Some(fi) => (Some(fi), &**value),
                    None => {
                        let d = self.diags.error("E0010", *name_span, format!("case `{ename}.{case}` has no field `{name}`"));
                        if !decl.is_empty() {
                            d.note(format!("its fields are: {}", list(&decl)));
                        }
                        (None, &**value)
                    }
                },
                _ => {
                    let fi = (next_pos < decl.len()).then_some(next_pos);
                    next_pos += 1;
                    if fi.is_none() {
                        self.diags.error(
                            "E0012",
                            a.span,
                            format!("case `{ename}.{case}` has {}", plural(decl.len(), "payload field")),
                        );
                    }
                    (fi, a)
                }
            };
            let Some(fi) = fi else {
                self.expr(value, None, cx);
                ok = false;
                continue;
            };
            if seen[fi] {
                self.diags.error("E0011", a.span, format!("field `{}` is given more than once", decl[fi].0));
                self.expr(value, None, cx);
                ok = false;
                continue;
            }
            seen[fi] = true;
            let fty = decl[fi].1;
            let h = self.expr(value, Some(fty), cx);
            let h = self.fit(h, fty, cx);
            let h = self.consume(h, cx);
            out.push((fi, h));
        }
        let missing: Vec<String> = decl.iter().zip(&seen).filter(|(_, s)| !**s).map(|((n, _), _)| format!("`{n}`")).collect();
        if ok && !missing.is_empty() {
            self.diags.error(
                "E0018",
                span,
                format!("missing {} in `{ename}.{case}`: {}", plural(missing.len(), "field"), missing.join(", ")),
            );
            ok = false;
        }
        if !ok {
            return err_expr(span);
        }
        HExpr { kind: HExprKind::EnumLit { variant: vi, fields: out }, ty, span, at: span }
    }

    /// The enum a `.Case` shorthand refers to, from the expected type.
    fn case_target(&self, expected: Option<Ty>) -> Option<usize> {
        let t = match expected? {
            Ty::Opt(o) => opt_inner(o),
            Ty::Res(r) => res_parts(r).0,
            t => t,
        };
        match t {
            Ty::Enum(id) => Some(id as usize),
            _ => None,
        }
    }

    /// `err(value)` and `ok(value)`: a result of the expected type.
    fn result_value(&mut self, is_ok: bool, arg: &Expr, expected: Option<Ty>, span: Span, cx: &mut FnCx) -> HExpr {
        let word = if is_ok { "ok" } else { "err" };
        let Some(Ty::Res(r)) = expected else {
            if expected != Some(Ty::Error) {
                self.diags
                    .error("E0001", span, format!("cannot tell which result type this `{word}(...)` has"))
                    .note("use it where a `T ! E` is expected, e.g. `return err(...)` in a function returning `T ! E`");
            }
            self.expr(arg, Some(Ty::Error), cx);
            return err_expr(span);
        };
        let ty = expected.unwrap();
        let (okt, errt) = res_parts(r);
        if is_ok && okt == Ty::Unit {
            if !matches!(arg.kind, ExprKind::Unit) {
                self.diags.error("E0001", arg.span, "this result has no success value: write `ok(())`");
                return err_expr(span);
            }
            return HExpr { kind: HExprKind::Ok(None), ty, span, at: span };
        }
        let want = if is_ok { okt } else { errt };
        let h = self.expr(arg, Some(want), cx);
        let h = self.fit(h, want, cx);
        if h.ty == Ty::Error {
            return err_expr(span);
        }
        let h = self.consume(h, cx);
        let kind = if is_ok { HExprKind::Ok(Some(Box::new(h))) } else { HExprKind::ErrVal(Box::new(h)) };
        HExpr { kind, ty, span, at: span }
    }

    /// `try value` (semantic table V3, V12): the success value; an error is returned from the
    /// current function, whose error type must be the same.
    fn try_expr(&mut self, inner: &Expr, span: Span, cx: &mut FnCx) -> HExpr {
        let h = self.expr(inner, None, cx);
        let Ty::Res(r) = h.ty else {
            if h.ty != Ty::Error {
                self.diags
                    .error("E0001", h.span, format!("`try` needs a value that can fail (`T ! E`), found `{}`", h.ty))
                    .note("for an optional value (`?T`) use `??` or `if let`");
            }
            return err_expr(span);
        };
        let (okt, errt) = res_parts(r);
        match cx.ret {
            Ty::Res(fr) => {
                let ferr = res_parts(fr).1;
                if ferr != errt && ferr != Ty::Error && errt != Ty::Error {
                    self.diags
                        .error("E0001", span, format!("`try` would return a `{errt}` error, but this function returns `{ferr}` errors"))
                        .note(format!("handle it here with `catch` or `match`, and return an error of type `{ferr}`"));
                    return err_expr(span);
                }
            }
            Ty::Error => return err_expr(span),
            other => {
                self.diags
                    .error("E0001", span, "`try` can only be used in a function that returns `T ! E`")
                    .note(format!("this function returns `{other}`; handle the error with `catch` or `match` instead"));
                return err_expr(span);
            }
        }
        let h = self.consume(h, cx);
        HExpr { kind: HExprKind::Try(Box::new(h)), ty: okt, span, at: span }
    }

    /// `value catch e => handler` / `value catch e { ... }` (semantic table V8, V12).
    #[allow(clippy::too_many_arguments)]
    fn catch_expr(
        &mut self,
        value: &Expr,
        name: &str,
        name_span: Span,
        handler: &Expr,
        block: bool,
        span: Span,
        cx: &mut FnCx,
    ) -> HExpr {
        let allowed = cx.diverge_ok == Some(span);
        cx.diverge_ok = None;
        let v = self.expr(value, None, cx);
        let (okt, errt) = match v.ty {
            Ty::Res(r) => res_parts(r),
            Ty::Error => (Ty::Error, Ty::Error),
            other => {
                self.diags
                    .error("E0001", v.span, format!("`catch` needs a value that can fail (`T ! E`), found `{other}`"))
                    .note("for an optional value (`?T`) use `??` or `if let`");
                (Ty::Error, Ty::Error)
            }
        };
        let v = self.consume(v, cx);
        let before = cx.moved.clone();
        cx.scopes.push(Vec::new());
        let bind = if name == "_" {
            None
        } else {
            let x = self.declare(cx, name, name_span, errt, LocalKind::BindOwned);
            // Uses later in this same statement count as later uses (see `Pre`).
            cx.locals[x].decl = cx.cur.saturating_sub(1);
            cx.locals[x].scope_end = cx.pre.expr_end.get(&(span.start, 0)).copied().unwrap_or(cx.cur).max(cx.cur);
            Some(x)
        };
        let (h, div) = if block {
            let ExprKind::Block(b) = &handler.kind else { unreachable!("the parser builds a block") };
            let out = self.stmts(&b.stmts, cx);
            let div = diverges(&b.stmts);
            if !div {
                self.diags
                    .error("E0601", name_span, "this `catch` block can reach its closing `}`")
                    .note("leave the block with `return`, `break`, `continue` or `panic(...)`, or give a replacement value with `catch e => value`");
            }
            (CatchHandler::Block(out), true)
        } else {
            match &handler.kind {
                ExprKind::Return(_) | ExprKind::Break | ExprKind::Continue => {
                    if !allowed {
                        self.diags.error(
                            "U0001",
                            handler.span,
                            "`catch e => return` is only supported as a whole `let` initializer, assigned value or statement for now; use the block form `catch e { return }`",
                        );
                    }
                    (CatchHandler::Value(Box::new(self.diverge(handler, cx))), true)
                }
                // `() ! E` has no success value: the handler is a call run for its effect.
                ExprKind::Call { callee, args } if okt == Ty::Unit => {
                    let print = match &callee.kind {
                        ExprKind::Ident(n) if OUTPUT_FNS.contains(&n.as_str()) && self.lookup(cx, n).is_none() => Some(n.clone()),
                        _ => None,
                    };
                    let kind = match print {
                        Some(n) => self.print(&n, args, handler.span, cx),
                        None => {
                            let h = self.expr(handler, None, cx);
                            if let Ty::Res(_) = h.ty {
                                self.unhandled(&h);
                            }
                            Some(HStmtKind::Expr(h))
                        }
                    };
                    let stmts = kind.map(|kind| HStmt { kind, span: handler.span }).into_iter().collect();
                    (CatchHandler::Block(stmts), false)
                }
                _ => {
                    let exp = (okt != Ty::Error).then_some(okt);
                    let r = self.expr(handler, exp, cx);
                    let r = match exp {
                        Some(t) => self.fit(r, t, cx),
                        None => r,
                    };
                    (CatchHandler::Value(Box::new(self.consume(r, cx))), false)
                }
            }
        };
        self.pop_scope(cx);
        let after = std::mem::replace(&mut cx.moved, before.clone());
        // The error path continues after the expression only when the handler gives a value.
        cx.moved = if div { before } else { union(&before, &after) };
        if okt == Ty::Error {
            return err_expr(span);
        }
        HExpr { kind: HExprKind::Catch { value: Box::new(v), bind, handler: h }, ty: okt, span, at: span }
    }

    /// `match` on enums, results and nested patterns (semantic table C7), as a statement
    /// (`expr_start` is `None`) or an expression. Returns the scrutinee, whether it is an owned
    /// temporary, the arms and (for an expression) the value type.
    #[allow(clippy::too_many_arguments)]
    fn match_general(
        &mut self,
        value: &Expr,
        arms: &[GArm],
        span: Span,
        expr_start: Option<usize>,
        expected: Option<Ty>,
        cx: &mut FnCx,
    ) -> Option<(HExpr, bool, Vec<HArm>, Ty)> {
        let stmt_idx = cx.cur;
        let scrut = self.expr(value, None, cx);
        let via_ref = matches!(scrut.ty, Ty::Ref(_));
        let mty = match scrut.ty {
            Ty::Ref(r) => ref_inner(r),
            t => t,
        };
        let root = place_root(&scrut);
        let owned = !via_ref && root.is_none();
        let scrut = if owned { self.consume(scrut, cx) } else { scrut };
        let (kind, loans) = if owned {
            let l = if mty.carries_borrow() { self.loans_of(&scrut, cx) } else { Vec::new() };
            (LocalKind::BindOwned, l)
        } else if via_ref {
            (LocalKind::Bind, self.loans_of(&scrut, cx))
        } else {
            let r = root.expect("a place");
            let writable = matches!(cx.locals[r].kind, LocalKind::Var | LocalKind::InoutParam);
            let mut l = vec![Loan::Local(r)];
            l.extend(cx.locals[r].loans.iter().copied());
            (if writable { LocalKind::BindMut } else { LocalKind::Bind }, l)
        };
        let before = cx.moved.clone();
        let mut harms: Vec<HArm> = Vec::new();
        let mut ends: Vec<(Moved, bool)> = Vec::new();
        let mut all_ok = mty != Ty::Error;
        let mut pats_ok: Vec<bool> = Vec::new();
        let mut result_ty = expected.filter(|t| !matches!(t, Ty::Error | Ty::Unit));
        for (i, arm) in arms.iter().enumerate() {
            cx.moved = before.clone();
            cx.scopes.push(Vec::new());
            let scope_end = match expr_start {
                None => cx.pre.bind_end.get(&(stmt_idx, i)).copied(),
                Some(s) => cx.pre.expr_end.get(&(s, i)).copied().map(|e| e.max(cx.cur)),
            };
            let pc = PatCx { owned, kind, loans: &loans, scope_end, in_expr: expr_start.is_some(), span: arm.span };
            let mut ok = true;
            let pat = self.check_pat(arm.pat, mty, &pc, &mut ok, cx);
            all_ok &= ok;
            pats_ok.push(ok);
            let (body, value, div) = match arm.body {
                GBody::Block(b) => {
                    let out = self.stmts(&b.stmts, cx);
                    (out, None, diverges(&b.stmts))
                }
                GBody::Expr(e) => match &e.kind {
                    ExprKind::Block(b) => {
                        let out = self.stmts(&b.stmts, cx);
                        if !diverges(&b.stmts) {
                            self.diags
                                .error("E0001", b.close, "a block arm of a `match` expression must leave (`return`, `break`, `continue`, `panic`)")
                                .note("give the arm a value instead: `pattern => value`");
                        }
                        (out, None, true)
                    }
                    ExprKind::Return(_) | ExprKind::Break | ExprKind::Continue => (Vec::new(), Some(self.diverge(e, cx)), true),
                    _ => {
                        let h = self.expr(e, result_ty, cx);
                        let h = match result_ty {
                            Some(t) => self.fit(h, t, cx),
                            None => {
                                if h.ty == Ty::Unit {
                                    self.diags.error("E0001", h.span, "this arm produces no value");
                                } else if h.ty != Ty::Error {
                                    result_ty = Some(h.ty);
                                }
                                h
                            }
                        };
                        (Vec::new(), Some(self.consume(h, cx)), false)
                    }
                },
            };
            self.pop_scope(cx);
            ends.push((std::mem::take(&mut cx.moved), div));
            harms.push(HArm { pat, body, value });
        }
        // A branch that cannot fall through does not affect the code after the `match`.
        let live: Vec<&Moved> = ends.iter().filter(|(_, d)| !d).map(|(m, _)| m).collect();
        let merged: Vec<&Moved> = if live.is_empty() { ends.iter().map(|(m, _)| m).collect() } else { live };
        cx.moved = match merged.split_first() {
            Some((first, rest)) => rest.iter().fold((*first).clone(), |acc, m| union(&acc, m)),
            None => before,
        };
        if !all_ok {
            return None;
        }
        for i in 0..harms.len() {
            let rows: Vec<Vec<&HPat>> = harms[..i].iter().map(|a| vec![&a.pat]).collect();
            if pats_ok[i] && !self.useful(&rows, &[&harms[i].pat], &[mty]) {
                self.diags
                    .warn("W0002", arms[i].span, "this arm can never run: earlier arms already handle its cases")
                    .note("remove it");
            }
        }
        let rows: Vec<Vec<&HPat>> = harms.iter().map(|a| vec![&a.pat]).collect();
        if self.useful(&rows, &[&WILD], &[mty]) {
            let missing = self.missing_case(&rows, mty);
            self.diags
                .error("E0602", span, format!("this `match` does not handle `{missing}`"))
                .note(format!("add a `{missing} => ...` arm, or `_ => ...` for every remaining case"));
            return None;
        }
        Some((scrut, owned, harms, result_ty.unwrap_or(Ty::Unit)))
    }

    /// Checks a pattern against a value of type `ty` and declares its bindings.
    fn check_pat(&mut self, p: &Pattern, ty: Ty, pc: &PatCx, ok: &mut bool, cx: &mut FnCx) -> HPat {
        if ty == Ty::Error {
            *ok = false;
            self.error_binds(p, pc, cx);
            return HPat::Wild;
        }
        let mut mismatch = |this: &mut Self, what: &str, span: Span, cx: &mut FnCx| {
            this.diags.error("E0001", span, format!("the pattern `{what}` cannot match a value of type `{ty}`"));
            *ok = false;
            this.error_binds(p, pc, cx);
            HPat::Wild
        };
        match p {
            Pattern::Wild => HPat::Wild,
            Pattern::Bind { name, span } => {
                if ty == Ty::Unit {
                    return mismatch(self, name, *span, cx);
                }
                let by_ref = !pc.owned && !ty.is_copy();
                if by_ref && ty.contains_ref_mut() {
                    self.diags.error("U0001", *span, format!("binding a stored `{ty}` is not supported yet"));
                    *ok = false;
                    return HPat::Wild;
                }
                let bty = if by_ref { Ty::Ref(ref_of(ty)) } else { ty };
                let x = self.declare(cx, name, *span, bty, pc.kind);
                if pc.in_expr {
                    // Uses later in this same statement count as later uses (see `Pre`).
                    cx.locals[x].decl = cx.cur.saturating_sub(1);
                }
                if let Some(end) = pc.scope_end {
                    cx.locals[x].scope_end = end;
                }
                if !pc.loans.is_empty() || bty.carries_borrow() {
                    self.store_loans(cx, x, pc.loans.to_vec(), *span, true);
                }
                HPat::Bind { local: x, by_ref }
            }
            Pattern::None | Pattern::Some(_) => {
                let Ty::Opt(o) = ty else {
                    let what = if matches!(p, Pattern::None) { "none" } else { "some(...)" };
                    return mismatch(self, what, pc.span, cx);
                };
                match p {
                    Pattern::Some(inner) => HPat::Opt(Some(Box::new(self.check_pat(inner, opt_inner(o), pc, ok, cx)))),
                    _ => HPat::Opt(None),
                }
            }
            Pattern::Ok(inner) | Pattern::Err(inner) => {
                let is_ok = matches!(p, Pattern::Ok(_));
                let Ty::Res(r) = ty else {
                    let what = if is_ok { "ok(...)" } else { "err(...)" };
                    return mismatch(self, what, pc.span, cx);
                };
                let (okt, errt) = res_parts(r);
                let inner = self.check_pat(inner, if is_ok { okt } else { errt }, pc, ok, cx);
                HPat::Res { ok: is_ok, inner: Box::new(inner) }
            }
            Pattern::Case { ty: tname, name, span, args } => {
                let Ty::Enum(id) = ty else {
                    return mismatch(self, &format!(".{name}"), *span, cx);
                };
                let ei = id as usize;
                if let Some((tn, tsp)) = tname {
                    if *tn != self.structs[ei].name {
                        self.diags.error("E0001", *tsp, format!("expected a case of `{ty}`, found a case of `{tn}`"));
                        *ok = false;
                        self.error_binds(p, pc, cx);
                        return HPat::Wild;
                    }
                }
                let Some(vi) = self.structs[ei].variants.iter().position(|v| v.name == *name) else {
                    let names: Vec<String> = self.structs[ei].variants.iter().map(|v| v.name.clone()).collect();
                    self.diags
                        .error("E0010", *span, format!("enum `{ty}` has no case `{name}`"))
                        .note(format!("its cases are: {}", names.join(", ")));
                    *ok = false;
                    self.error_binds(p, pc, cx);
                    return HPat::Wild;
                };
                let ftys: Vec<Ty> = self.structs[ei].variants[vi].fields.iter().map(|f| f.1).collect();
                if args.len() != ftys.len() {
                    let fields: Vec<String> = self.structs[ei].variants[vi].fields.iter().map(|f| f.0.clone()).collect();
                    let d = self.diags.error(
                        "E0012",
                        *span,
                        format!("case `{name}` has {}, but the pattern lists {}", plural(ftys.len(), "field"), args.len()),
                    );
                    if !fields.is_empty() {
                        d.note(format!("write `.{name}({})`, using `_` for fields you do not need", fields.join(", ")));
                    }
                    *ok = false;
                    self.error_binds(p, pc, cx);
                    return HPat::Wild;
                }
                let fields = args.iter().zip(ftys).map(|(a, t)| self.check_pat(a, t, pc, ok, cx)).collect();
                HPat::Case { variant: vi, fields }
            }
        }
    }

    /// After an error in a pattern, its names are still declared (with an error type) so the arm
    /// does not report them as unknown.
    fn error_binds(&mut self, p: &Pattern, pc: &PatCx, cx: &mut FnCx) {
        match p {
            Pattern::Bind { name, span } => {
                let x = self.declare(cx, name, *span, Ty::Error, pc.kind);
                cx.locals[x].used = true;
            }
            Pattern::Some(q) | Pattern::Ok(q) | Pattern::Err(q) => self.error_binds(q, pc, cx),
            Pattern::Case { args, .. } => {
                for a in args {
                    self.error_binds(a, pc, cx);
                }
            }
            Pattern::Wild | Pattern::None => {}
        }
    }

    /// The constructors of a type that patterns can tell apart, with each one's field types:
    /// `none`/`some`, `ok`/`err`, or the cases of an enum.
    fn ctors_of(&self, t: Ty) -> Option<Vec<Vec<Ty>>> {
        match t {
            Ty::Opt(o) => Some(vec![Vec::new(), vec![opt_inner(o)]]),
            Ty::Res(r) => {
                let (a, b) = res_parts(r);
                Some(vec![vec![a], vec![b]])
            }
            Ty::Enum(id) => Some(self.structs[id as usize].variants.iter().map(|v| v.fields.iter().map(|f| f.1).collect()).collect()),
            _ => None,
        }
    }

    /// Whether some value matching `q` is matched by none of `rows` (the usefulness check of
    /// pattern-match compilation; one column per type in `tys`).
    fn useful(&self, rows: &[Vec<&HPat>], q: &[&HPat], tys: &[Ty]) -> bool {
        if q.is_empty() {
            return rows.is_empty();
        }
        let (rest_q, rest_t) = (&q[1..], &tys[1..]);
        let specialize = |c: usize, arity: usize| -> Vec<Vec<&HPat>> {
            rows.iter()
                .filter_map(|r| match pat_ctor(r[0]) {
                    None => Some(std::iter::repeat_n(&WILD, arity).chain(r[1..].iter().copied()).collect()),
                    Some((d, subs)) if d == c => Some(subs.into_iter().chain(r[1..].iter().copied()).collect()),
                    _ => None,
                })
                .collect()
        };
        let ctors = self.ctors_of(tys[0]);
        match pat_ctor(q[0]) {
            Some((c, subs)) => {
                let sub_t = ctors.as_ref().and_then(|cs| cs.get(c).cloned()).unwrap_or_default();
                let arity = subs.len();
                let q2: Vec<&HPat> = subs.into_iter().chain(rest_q.iter().copied()).collect();
                let t2: Vec<Ty> = sub_t.into_iter().chain(rest_t.iter().copied()).collect();
                self.useful(&specialize(c, arity), &q2, &t2)
            }
            None => {
                let complete = ctors.as_ref().is_some_and(|cs| {
                    !cs.is_empty() && (0..cs.len()).all(|c| rows.iter().any(|r| pat_ctor(r[0]).is_some_and(|(d, _)| d == c)))
                });
                if complete {
                    let cs = ctors.unwrap();
                    (0..cs.len()).any(|c| {
                        let arity = cs[c].len();
                        let q2: Vec<&HPat> = std::iter::repeat_n(&WILD, arity).chain(rest_q.iter().copied()).collect();
                        let t2: Vec<Ty> = cs[c].iter().copied().chain(rest_t.iter().copied()).collect();
                        self.useful(&specialize(c, arity), &q2, &t2)
                    })
                } else {
                    let def: Vec<Vec<&HPat>> =
                        rows.iter().filter(|r| pat_ctor(r[0]).is_none()).map(|r| r[1..].to_vec()).collect();
                    self.useful(&def, rest_q, rest_t)
                }
            }
        }
    }

    /// A case that `rows` do not cover, written as a pattern for the diagnostic.
    fn missing_case(&self, rows: &[Vec<&HPat>], t: Ty) -> String {
        let Some(cs) = self.ctors_of(t) else { return "_".into() };
        for (c, fields) in cs.iter().enumerate() {
            let wilds = || fields.iter().map(|_| HPat::Wild).collect::<Vec<_>>();
            let p = match t {
                Ty::Opt(_) if c == 0 => HPat::Opt(None),
                Ty::Opt(_) => HPat::Opt(Some(Box::new(HPat::Wild))),
                Ty::Res(_) => HPat::Res { ok: c == 0, inner: Box::new(HPat::Wild) },
                _ => HPat::Case { variant: c, fields: wilds() },
            };
            if self.useful(rows, &[&p], &[t]) {
                return match t {
                    Ty::Opt(_) if c == 0 => "none".into(),
                    Ty::Opt(_) => "some(_)".into(),
                    Ty::Res(_) if c == 0 => "ok(_)".into(),
                    Ty::Res(_) => "err(_)".into(),
                    Ty::Enum(id) => {
                        let v = &self.structs[id as usize].variants[c];
                        if v.fields.is_empty() {
                            format!(".{}", v.name)
                        } else {
                            format!(".{}({})", v.name, v.fields.iter().map(|_| "_").collect::<Vec<_>>().join(", "))
                        }
                    }
                    _ => "_".into(),
                };
            }
        }
        "_".into()
    }

    /// `panic(message)` and `assert(cond)` / `assert(cond, message)`.
    fn panic_call(&mut self, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let (cond, msg) = match (name, args) {
            ("panic", [m]) => (None, Some(m)),
            ("assert", [c]) => (Some(c), None),
            ("assert", [c, m]) => (Some(c), Some(m)),
            _ => {
                let want = if name == "panic" { "exactly one argument (the message)" } else { "a condition and an optional message" };
                self.diags.error("E0012", span, format!("`{name}` takes {want}"));
                return err_expr(span);
            }
        };
        let cond = cond.map(|c| Box::new(self.cond(c, cx)));
        let parts = match msg {
            Some(m) => self.print_parts(m, cx),
            None => Vec::new(),
        };
        HExpr { kind: HExprKind::Panic { cond, parts }, ty: Ty::Unit, span, at }
    }

    fn conversion_inner(&mut self, to: IntTy, checked: bool, args: &[Expr], at: Span, span: Span, cx: &mut FnCx) -> HExpr {
        self.conversion_body(to, checked, args, at, span, cx)
    }

    /// `min(a, b)` / `max(a, b)` on two integers of the same type.
    #[allow(clippy::too_many_arguments)]
    fn min_max(&mut self, max: bool, args: &[Expr], expected: Option<Ty>, span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let name = if max { "max" } else { "min" };
        let [a, b] = args else {
            self.diags.error("E0012", span, format!("`{name}` takes exactly two arguments"));
            return err_expr(span);
        };
        let (ha, hb) = self.pair(a, b, expected, cx);
        match (ha.ty, hb.ty) {
            (Ty::Error, _) | (_, Ty::Error) => err_expr(span),
            (Ty::Int(x), Ty::Int(y)) if x == y => {
                HExpr { kind: HExprKind::MinMax { max, a: Box::new(ha), b: Box::new(hb) }, ty: Ty::Int(x), span, at }
            }
            (x, y) => {
                self.diags
                    .error("E0001", span, format!("`{name}` needs two integers of the same type, found `{x}` and `{y}`"))
                    .note("convert one side explicitly, e.g. `int(...)`");
                err_expr(span)
            }
        }
    }

    /// `io.readAll()`, `io.readLine()`, `fs.readText(path)`, `fs.writeText(path, text)`,
    /// `os.args()` (stdlib contract §5). `None` for other module members.
    fn module_call(&mut self, m: &str, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> Option<HExpr> {
        let lib = |this: &Self, e: &str| -> Ty { this.structs[this.struct_index[e]].ty };
        let (op, nargs, ty) = match (m, name) {
            ("os", "args") => {
                if !args.is_empty() {
                    self.diags.error("E0012", span, "`os.args` takes no arguments");
                    return Some(err_expr(span));
                }
                return Some(HExpr { kind: HExprKind::OsArgs, ty: Ty::Slice(slice_of(Ty::Str)), span, at });
            }
            ("io", "readAll") => (IoOp::ReadAll, 0, Ty::Res(res_of(Ty::String, lib(self, "io.Error")))),
            ("io", "readLine") => (IoOp::ReadLine, 0, Ty::Res(res_of(Ty::Opt(opt_of(Ty::String)), lib(self, "io.Error")))),
            ("fs", "readText") => (IoOp::ReadText, 1, Ty::Res(res_of(Ty::String, lib(self, "fs.Error")))),
            ("fs", "writeText") => (IoOp::WriteText, 2, Ty::Res(res_of(Ty::Unit, lib(self, "fs.Error")))),
            _ => return None,
        };
        if args.len() != nargs {
            let what = ["no arguments", "one argument (the path)", "two arguments (the path and the text)"][nargs];
            self.diags.error("E0012", span, format!("`{m}.{name}` takes {what}"));
            return Some(err_expr(span));
        }
        let mut hargs = Vec::new();
        for a in args {
            let h = self.expr(a, Some(Ty::Str), cx);
            let h = self.deref(h);
            let h = if h.ty == Ty::String {
                let (span, at) = (h.span, h.at);
                HExpr { kind: HExprKind::StrView(Box::new(h)), ty: Ty::Str, span, at }
            } else {
                h
            };
            self.expect_ty(&h, Ty::Str);
            hargs.push(h);
        }
        Some(HExpr { kind: HExprKind::Io { op, args: hargs }, ty, span, at })
    }

    /// `v.sort()`: a stable sort of a writable vector whose elements are ordered (K1).
    fn sort_call(&mut self, base: &Expr, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let Some(place) = self.mutable_place(base, "sort", cx) else { return err_expr(span) };
        if !args.is_empty() {
            self.diags
                .error("E0012", span, "`sort` takes no arguments")
                .note("to sort by a custom order use `sortBy((a, b) => ...)`");
            return err_expr(span);
        }
        match place.ty {
            Ty::Vec(v) => match vec_elem(v) {
                Ty::Int(_) | Ty::Str | Ty::String => HExpr { kind: HExprKind::Sort(place), ty: Ty::Unit, span, at },
                Ty::Error => err_expr(span),
                e if self.protocol(e).is_some_and(|(cmp, _)| cmp.is_some()) => {
                    let func = self.protocol(e).and_then(|(cmp, _)| cmp).expect("checked above");
                    let tramp = self.tramps.len();
                    self.tramps.push(Tramp { func, caps: Vec::new(), elem: e, by_cmp: true });
                    HExpr { kind: HExprKind::SortBy { place, tramp, caps: Vec::new() }, ty: Ty::Unit, span, at }
                }
                e => {
                    self.diags
                        .error("E0501", span, format!("values of type `{e}` cannot be ordered, so `sort` cannot be used"))
                        .note("sort by a key with `sortBy((a, b) => ...)`, or declare `Ord` with a `cmp` method");
                    err_expr(span)
                }
            },
            Ty::Error => err_expr(span),
            other => {
                self.diags.error("U0001", at, format!("method `sort` on `{other}` is not supported yet"));
                err_expr(span)
            }
        }
    }

    /// Text and sequence methods that take arguments or are new in the stdlib step:
    /// `contains`, `startsWith`, `endsWith`, `find`, `split`, `toLower`, `toUpper`, `parseInt`,
    /// and `contains`/`join` on vectors and slices. `None` when `name` is none of these.
    #[allow(clippy::too_many_arguments)]
    fn method2(&mut self, recv: HExpr, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> Result<HExpr, HExpr> {
        let int = Ty::Int(IntTy::I64);
        let text = matches!(recv.ty, Ty::Str | Ty::String);
        let (method, ty, arg_ty) = match (name, recv.ty) {
            ("contains", _) if text => (Method2::Contains, Ty::Bool, Some(Ty::Str)),
            ("startsWith", _) if text => (Method2::StartsWith, Ty::Bool, Some(Ty::Str)),
            ("endsWith", _) if text => (Method2::EndsWith, Ty::Bool, Some(Ty::Str)),
            ("find", _) if text => (Method2::Find, Ty::Opt(opt_of(int)), Some(Ty::Str)),
            ("split", _) if text => (Method2::Split, Ty::Vec(vec_of(Ty::Str)), Some(Ty::Str)),
            ("toLower", _) if text => (Method2::ToLower, Ty::String, None),
            ("toUpper", _) if text => (Method2::ToUpper, Ty::String, None),
            ("parseInt", _) if text => (Method2::ParseInt, Ty::Opt(opt_of(int)), None),
            ("contains", Ty::Vec(_) | Ty::Slice(_)) => {
                let e = seq_elem(recv.ty).expect("a sequence");
                if !matches!(e, Ty::Int(_) | Ty::Bool | Ty::Str | Ty::String) {
                    self.diags.error("E0501", span, format!("values of type `{e}` cannot be compared, so `contains` cannot be used"));
                    return Ok(err_expr(span));
                }
                // `String` elements are looked up with a `str` (like map keys, C6).
                (Method2::SeqContains, Ty::Bool, Some(if e == Ty::String { Ty::Str } else { e }))
            }
            ("join", Ty::Vec(_) | Ty::Slice(_)) => {
                let e = seq_elem(recv.ty).expect("a sequence");
                if !matches!(e, Ty::Str | Ty::String) {
                    self.diags.error("E0001", span, format!("`join` needs text elements (`str` or `String`), found `{e}`"));
                    return Ok(err_expr(span));
                }
                (Method2::Join, Ty::String, Some(Ty::Str))
            }
            _ => return Err(recv),
        };
        let want = usize::from(arg_ty.is_some());
        if args.len() != want {
            let what = if want == 0 { "no arguments".to_string() } else { "exactly one argument".to_string() };
            self.diags.error("E0012", span, format!("`{name}` takes {what}"));
            return Ok(err_expr(span));
        }
        let mut hargs = Vec::new();
        if let (Some(t), [a]) = (arg_ty, args) {
            let h = self.expr(a, Some(t), cx);
            let h = self.deref(h);
            let h = if t == Ty::Str && h.ty == Ty::String {
                let (span, at) = (h.span, h.at);
                HExpr { kind: HExprKind::StrView(Box::new(h)), ty: Ty::Str, span, at }
            } else {
                h
            };
            self.expect_ty(&h, t);
            hargs.push(h);
        }
        Ok(HExpr { kind: HExprKind::Call2 { method, recv: Box::new(recv), args: hargs }, ty, span, at })
    }

    fn conversion_body(&mut self, to: IntTy, checked: bool, args: &[Expr], at: Span, span: Span, cx: &mut FnCx) -> HExpr {
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
