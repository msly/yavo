//! Name resolution and tye checking. Produces a typed IR (`H*`) for code generation.

use std::collections::HashMap;

use crate::ast::*;
use crate::diag::{Diags, Span};
use crate::types::{
    box_inner, box_of, finish_agg, map_of, map_parts, opt_inner, opt_of, ref_inner, ref_mut_inner, ref_mut_of, ref_of, register_enum,
    register_iface,
    register_struct, res_of, res_parts, set_user_drop, has_user_drop, slice_elem, slice_of, tuple_elems, tuple_of, vec_elem,
    vec_of, deque_of, heap_of, arena_of, FloatTy, IntTy, Ty, sync_of, sync_parts, ptr_inner, ptr_read_only, SyncKind, dyn_base, dyn_caps, dyn_with_caps,
};

pub type LocalId = usize;

/// A step of `HPlace::path` (and of `HStmtKind::Assign`'s path) that goes into the content of
/// a `Box` instead of a field (semantic table A7: fields project through `Box`).
pub const BOX_STEP: usize = usize::MAX;
/// The first step of an `HStmtKind::Assign` path that writes through the `&inout T` variable
/// itself (`*p = v`, A6) instead of rebinding it. Only appears at the start of a path.
pub const DEREF_STEP: usize = usize::MAX - 1;

pub struct HProgram {
    pub files: Vec<SourceFile>,
    pub fns: Vec<HFn>,
    /// Struct and enum definitions, ordered so that every type comes after the types it
    /// contains by value.
    pub structs: Vec<HStruct>,
    /// Comparison adapters for `sortBy`, one per call site.
    pub tramps: Vec<Tramp>,
    /// Interfaces, by interface id (`Ty::Dyn(id)`).
    pub ifaces: Vec<HIface>,
    /// One vtable per (type, interface) pair declared with `struct T: I`.
    pub impls: Vec<HImpl>,
}

/// An interface: the vtable layout of `Box[I]`.
pub struct HIface {
    pub ty: Ty,
    pub methods: Vec<HIfaceMethod>,
}

/// One method slot. Index 0 of `params`/`inouts`/`sinks` is the receiver (`self`,
/// `inout self` or `sink self`); its type is the interface value itself.
pub struct HIfaceMethod {
    pub params: Vec<Ty>,
    pub inouts: Vec<bool>,
    pub sinks: Vec<bool>,
    pub ret: Ty,
}

/// `struct T: I`: the functions that fill `I`'s slots for `T`, in slot order.
pub struct HImpl {
    pub ty: Ty,
    pub iface: usize,
    pub fns: Vec<usize>,
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
    /// An owning environment stored by Heap.by, rather than a borrowed stack callback.
    pub saved: bool,
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
    /// `${x:.N}`: a float with exactly `N` digits after the decimal point.
    Fixed(HExpr, u8),
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
    /// A float literal of the float type in `ty`.
    Float(f64),
    Bool(bool),
    Str(String),
    Local(LocalId),
    /// Evaluate a returned mutable reference once, then use it as a receiver. The
    /// hidden descriptor has no ownership or destructor of its own.
    LetTemp { local: LocalId, init: Box<HExpr>, value: Box<HExpr> },
    Call { func: usize, args: Vec<HExpr> },
    Neg(Box<HExpr>),
    Not(Box<HExpr>),
    Binary { op: BinOp, lhs: Box<HExpr>, rhs: Box<HExpr> },
    Convert { to: IntTy, checked: bool, expr: Box<HExpr> },
    /// `f64(x)` / `f32(x)` from an integer or the other float type (`to` is in `ty`).
    ToFloat(Box<HExpr>),
    /// `int(x)` and friends from a float: truncates; NaN, infinities and out-of-range values
    /// panic (P-FLOAT-CONVERT).
    FloatToInt { to: IntTy, expr: Box<HExpr> },
    /// `math.sqrt(x)`, `math.pow(x, y)`, `math.floor(x)`, `math.ceil(x)`: the C function.
    Math { func: &'static str, args: Vec<HExpr> },
    /// Field values in the order they were written (`usize` is the field index).
    StructLit { fields: Vec<(usize, HExpr)> },
    Field { base: Box<HExpr>, field: usize },
    /// Moves a non-copyable local out; the slot is left empty.
    Move(LocalId),
    VecLit(Vec<HExpr>),
    DequeNew,
    ArenaNew,
    ArenaRead { op: MapRead, recv: Box<HExpr>, key: Box<HExpr> },
    ArenaWrite { op: ArenaWrite, place: HPlace, value: Box<HExpr> },
    HeapNew { tramp: usize, caps: Vec<HExpr> },
    DequePush { place: HPlace, value: Box<HExpr>, front: bool },
    DequePop { place: HPlace, front: bool },
    /// `(a, b, ...)`: element values in order.
    TupleLit(Vec<HExpr>),
    /// `base[index]` for a copyable element, or a byte of a string.
    Index { base: Box<HExpr>, index: Box<HExpr> },
    /// `base[start..end]`: a `[]T` view of a vector or slice, or a `str` view of text (S3).
    SliceRange { base: Box<HExpr>, start: Box<HExpr>, end: Box<HExpr> },
    Method { method: Builtin, recv: Box<HExpr> },
    /// `local.push(value)`; `value` is moved into a vector or appended to a `String`.
    Push { place: HPlace, value: Box<HExpr> },
    /// `place.reserve(n)` on a vector.
    Reserve { place: HPlace, n: Box<HExpr> },
    /// A `String` borrowed as a `str` (passing it to a `str` parameter).
    StrView(Box<HExpr>),
    /// Reading through a `&T`.
    Deref(Box<HExpr>),
    /// `Rc(v)`, `Arc(v)`, `Mutex(v)`: a new shared block holding `v`.
    SyncNew { kind: SyncKind, value: Box<HExpr> },
    /// `m.lock()` (a `Guard[T]`) or `m.tryLock()` (a `?Guard[T]`) on a `Mutex[T]` value.
    Lock { mutex: Box<HExpr>, try_: bool },
    /// `.clone()` of an `Rc`, `Arc` or `Channel`: one more handle to the same block.
    SyncClone(Box<HExpr>),
    /// `Channel[T]{cap: n}`.
    ChanNew(Box<HExpr>),
    /// `ch.send(v)`: `() ! SendError[T]`; a closed channel gives `v` back.
    ChanSend { chan: Box<HExpr>, value: Box<HExpr> },
    /// `ch.recv()`: `?T`, none once closed and empty.
    ChanRecv(Box<HExpr>),
    /// `ch.close()`.
    ChanClose(Box<HExpr>),
    /// The empty value of a `scope` block (its hidden local's initializer).
    ScopeNew,
    /// `spawn f(args)` in the scope whose hidden value is `scope`: the arguments are evaluated
    /// here, then the call runs on a new thread; the value is a `Join[T]`.
    Spawn { scope: LocalId, func: usize, args: Vec<HExpr> },
    /// `h.join()`: waits for the thread and takes its result (consumes the handle).
    JoinCall(Box<HExpr>),
    /// `String.fromUtf8(bytes)`: consumes a `Vec[byte]`; the result is `String ! Utf8Error`
    /// (stdlib contract §5).
    FromUtf8(Box<HExpr>),
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
    /// A `Box[T]` used where a `Box[I]` is expected: the box with the vtable of `impls[imp]`.
    ToDyn { value: Box<HExpr>, imp: usize },
    /// `x.m(args)` on a `Box[I]`: slot `method` of interface `iface` (`sig` is its checker
    /// signature). `args[0]` is the receiver: the value, an `InoutPlace`, or a moved value.
    DynCall { iface: usize, method: usize, sig: usize, args: Vec<HExpr> },
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
    /// `s.parseFloat()`: `?f64`.
    ParseFloat,
    /// `x.totalCmp(y)` on floats: an `Ordering` from the total order of bit patterns.
    TotalCmp,
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
    WriteNewText,
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
pub enum ArenaWrite { Add, Remove, GetMut }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Builtin {
    StrToString,
    StrLen,
    StringLen,
    StringClone,
    VecLen,
    VecClone,
    HeapLen,
    /// View-producing methods on `str`/`String`: the result points into the receiver's text.
    Trim,
    Fields,
    Lines,
    IsSome,
    IsNone,
    MapLen,
    MapClone,
    /// `s.toVec()` on a `[]T` with copyable elements: a new vector with the same elements.
    SliceToVec,
}

/// Stands for a function that was already reported as unusable here.
const NO_FN: usize = usize::MAX;
/// Signature numbers from here on are interface method slots (`Checker::dyn_sigs`); below it,
/// a signature number is also the function's index (user functions, then lifted functions).
const DYN_BASE: usize = 1 << 40;
const OUTPUT_FNS: [&str; 3] = ["println", "print", "eprintln"];
const UNSUPPORTED_TYPES: &[&str] = &[
    "Arena", "Id", "Atomic", "SendError", "Join",
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
    "len",
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

#[derive(Clone)]
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
    /// A lock guard bound by `with`: borrows taken through it cannot leave the block (P4).
    guard: bool,
    /// Holds borrows until the end of its scope whatever its last use (the hidden value of a
    /// `scope` block, which keeps its threads' borrows: P3).
    pinned: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Loan {
    /// Points into this variable (an owned `String`/`Vec`, or a parameter).
    Local(LocalId),
    /// Points into a temporary that is dropped at the end of the statement.
    Temp,
}

fn mentions(e: &Expr, name: &str) -> bool {
    match &e.kind {
        ExprKind::Ident(n) => n == name,
        ExprKind::Int(_) | ExprKind::Byte(_) | ExprKind::Bool(_) | ExprKind::None | ExprKind::Break | ExprKind::Continue | ExprKind::Unit | ExprKind::Generic { .. } | ExprKind::Float(_) => false,
        ExprKind::Return(v) => v.as_ref().is_some_and(|v| mentions(v, name)),
        ExprKind::Case { args, .. } => args.iter().flatten().any(|a| mentions(a, name)),
        ExprKind::Named { value, .. } | ExprKind::Err(value) | ExprKind::Try(value) | ExprKind::Spawn(value) => mentions(value, name),
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
        ExprKind::SliceRange { base, start, end } => mentions(base, name) || mentions(start, name) || mentions(end, name),
        ExprKind::Unary { expr, .. } | ExprKind::Paren(expr) | ExprKind::Borrow(expr) => mentions(expr, name),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Coalesce { lhs, rhs, .. } => {
            mentions(lhs, name) || mentions(rhs, name)
        }
    }
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
            // The guard is a binding of this statement that lives until the end of the body.
            StmtKind::With { init, body, .. } => {
                self.expr(init, idx, stack);
                self.block(&body.stmts, stack);
                self.bind_end.insert((idx, 0), self.next);
            }
            // The scope's hidden value lives until the end of the body.
            StmtKind::Scope { body } => {
                self.block(&body.stmts, stack);
                self.bind_end.insert((idx, 0), self.next);
            }
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
            ExprKind::Int(_) | ExprKind::Byte(_) | ExprKind::Bool(_) | ExprKind::None | ExprKind::Break | ExprKind::Continue | ExprKind::Unit | ExprKind::Generic { .. } | ExprKind::Float(_) => {}
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
            ExprKind::Named { value, .. } | ExprKind::Err(value) | ExprKind::Try(value) | ExprKind::Spawn(value) => self.expr(value, idx, stack),
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
            ExprKind::SliceRange { base, start, end } => {
                self.expr(base, idx, stack);
                self.expr(start, idx, stack);
                self.expr(end, idx, stack);
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
    /// The hidden values of the enclosing `scope` blocks, innermost last (`spawn` adds to it).
    thread_scopes: Vec<LocalId>,
    /// The expression (by span) that may be `x ?? return/break/continue`: the whole initializer
    /// of a `let` or right side of an assignment, or the right side of such a `??`.
    diverge_ok: Option<Span>,
    /// Checking the body of a `drop` method of a `T: Drop` type (V11).
    in_drop: bool,
    /// In a closure function: the parameters that stand for captured outer variables.
    captures: Vec<LocalId>,
    /// Arguments already evaluated by an enclosing call stay borrowed until the
    /// call starts. These also cover returned references with no named local.
    call_shared: Vec<LocalId>,
    call_mut: Vec<LocalId>,
    /// Values already computed for an indexed assignment survive until the target
    /// address and its indices have been evaluated. These are value loans, not
    /// loans of an owning container's descriptor (unlike read-only call arguments).
    assignment_shared: Vec<LocalId>,
    assignment_mut: Vec<LocalId>,
    /// Receivers reserved by an enclosing modifying call while its arguments run (V2,
    /// two-phase): the arguments may read them, but not modify or move them, and the
    /// argument values must not keep pointing into them. With a description of the call.
    call_reserved: Vec<(LocalId, String)>,
    /// Calls resolved to the unconditional builtins `panic` / `os.exit`. Source
    /// spelling alone is insufficient: a user function or local can shadow a name.
    exit_calls: std::collections::HashSet<(usize, usize)>,
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
            thread_scopes: Vec::new(),
            diverge_ok: None,
            in_drop: false,
            captures: Vec::new(),
            call_shared: Vec::new(),
            call_mut: Vec::new(),
            assignment_shared: Vec::new(),
            assignment_mut: Vec::new(),
            call_reserved: Vec::new(),
            exit_calls: std::collections::HashSet::new(),
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
    loan_breaks: Option<Loans>,
    loan_continues: Option<Loans>,
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

/// What each local points into at one program point. Like `Moved`, it is
/// restored before each alternative path and merged where paths join, so a
/// loan taken on one branch or a later loop iteration is not lost (E4, O2).
type Loans = Vec<Vec<Loan>>;

fn loans_snapshot(cx: &FnCx) -> Loans {
    cx.locals.iter().map(|l| l.loans.clone()).collect()
}

fn loans_restore(cx: &mut FnCx, state: &Loans) {
    for (local, loans) in cx.locals.iter_mut().zip(state) {
        local.loans = loans.clone();
    }
}

fn loans_union(a: &Loans, b: &Loans) -> Loans {
    (0..a.len().max(b.len()))
        .map(|i| {
            let mut out = a.get(i).cloned().unwrap_or_default();
            for l in b.get(i).into_iter().flatten() {
                if !out.contains(l) {
                    out.push(*l);
                }
            }
            out
        })
        .collect()
}

fn loans_union_into(slot: &mut Option<Loans>, state: &Loans) {
    *slot = Some(match slot.take() {
        Some(prev) => loans_union(&prev, state),
        None => state.clone(),
    });
}

/// Whether `after` adds a loan to one of the locals that existed in `before`.
fn loans_grew(before: &Loans, after: &Loans) -> bool {
    before.iter().zip(after).any(|(b, a)| a.iter().any(|l| !b.contains(l)))
}

/// Whether every borrow a value of this type may carry is a `str` (text), so it
/// cannot point into the element storage of a container.
fn only_text_borrows(ty: Ty) -> bool {
    match ty {
        Ty::Ref(_) | Ty::RefMut(_) | Ty::Slice(_) | Ty::Sync(_) | Ty::Dyn(_) => false,
        Ty::Vec(id) | Ty::Deque(id) | Ty::Heap(id) | Ty::Arena(id) => only_text_borrows(vec_elem(id)),
        Ty::Opt(id) => only_text_borrows(opt_inner(id)),
        Ty::Box(id) => only_text_borrows(box_inner(id)),
        Ty::Tuple(id) => tuple_elems(id).iter().all(|t| only_text_borrows(*t)),
        Ty::Res(id) => {
            let (o, e) = res_parts(id);
            only_text_borrows(o) && only_text_borrows(e)
        }
        Ty::Map(id) | Ty::Set(id) => {
            let (k, v) = map_parts(id);
            only_text_borrows(k) && only_text_borrows(v)
        }
        _ => true,
    }
}

/// Merge the end states of alternative paths; paths that cannot fall through
/// do not reach the join unless every path diverges.
fn loans_join(ends: &[(Loans, bool)], before: &Loans) -> Loans {
    let live: Vec<&Loans> = ends.iter().filter(|(_, d)| !d).map(|(l, _)| l).collect();
    let merged: Vec<&Loans> = if live.is_empty() { ends.iter().map(|(l, _)| l).collect() } else { live };
    match merged.split_first() {
        Some((first, rest)) => rest.iter().fold((*first).clone(), |acc, l| loans_union(&acc, l)),
        None => before.clone(),
    }
}

/// Whether control can never fall off the end of these statements.
fn diverges(stmts: &[Stmt], cx: &FnCx) -> bool {
    terminates(stmts, cx) || stmts.iter().any(|s| matches!(s.kind, StmtKind::Break | StmtKind::Continue))
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

/// Every name used in `e` (statements of nested blocks included), with its line.
fn idents_expr(e: &Expr, out: &mut Vec<(String, u32)>) {
    let sub = |x: &Expr, out: &mut Vec<(String, u32)>| idents_expr(x, out);
    match &e.kind {
        ExprKind::Ident(n) => out.push((n.clone(), e.span.line)),
        ExprKind::Int(_) | ExprKind::Byte(_) | ExprKind::Bool(_) | ExprKind::None | ExprKind::Break | ExprKind::Continue | ExprKind::Unit | ExprKind::Generic { .. } | ExprKind::Float(_) => {}
        ExprKind::Return(v) => {
            if let Some(v) = v {
                sub(v, out);
            }
        }
        ExprKind::Case { args, .. } => args.iter().flatten().for_each(|a| sub(a, out)),
        ExprKind::Named { value, .. } | ExprKind::Err(value) | ExprKind::Try(value) | ExprKind::Spawn(value) => sub(value, out),
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
        ExprKind::SliceRange { base, start, end } => {
            sub(base, out);
            sub(start, out);
            sub(end, out);
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
            StmtKind::With { init, body, .. } => {
                idents_expr(init, out);
                idents_block(&body.stmts, out);
            }
            StmtKind::Scope { body } => idents_block(&body.stmts, out),
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
    imports: HashMap<(usize, String), (Span, bool)>,
    /// User comparison protocols by struct/enum id (K2): the `cmp` function of a `T: Ord`, the
    /// `eq` function of a `T: Eq`.
    protocols: HashMap<u32, (Option<usize>, Option<usize>)>,
    /// Number of user functions; closure functions are numbered after them.
    n_fns: usize,
    /// Functions made from closures (O5), in order.
    lifted: Vec<HFn>,
    tramps: Vec<Tramp>,
    /// Interfaces by id, and by name.
    ifaces: Vec<IfaceDef>,
    iface_index: HashMap<String, usize>,
    /// Signatures (indexes into `sigs`, after the user functions) that stand for interface
    /// method slots: (interface, slot).
    dyn_slots: HashMap<usize, (usize, usize)>,
    /// Declared implementations, and their index by (type, interface).
    impls: Vec<HImpl>,
    impl_index: HashMap<(Ty, usize), usize>,
    /// (type, interface) pairs declared with `struct T: I`, valid or not.
    declared_impls: std::collections::HashSet<(Ty, usize)>,
    /// Signatures of interface method slots, numbered from `DYN_BASE`.
    dyn_sigs: Vec<Sig>,
    /// The program, for instantiating generic functions and structs from their declarations.
    prog: &'d Program,
    /// Generic functions and structs by name (index into `prog.fns` / `prog.structs`).
    gen_fns: HashMap<String, usize>,
    gen_structs: HashMap<String, usize>,
    /// Instances made so far: generic functions (and methods of generic structs) by declaration
    /// index and type arguments, as function indexes; generic structs as types.
    fn_insts: HashMap<(usize, Vec<Ty>), usize>,
    struct_insts: HashMap<(usize, Vec<Ty>), Ty>,
    /// The generic struct and type arguments of each struct instance.
    inst_of: HashMap<Ty, (usize, Vec<Ty>)>,
    /// What type parameters (and, inside a generic struct's methods, its own name and `Self`)
    /// stand for while an instance is resolved or checked.
    subst: HashMap<String, Ty>,
    /// Function instances whose bodies still have to be checked: (function index, declaration
    /// index, substitution, C-safe name).
    pending: Vec<(usize, usize, HashMap<String, Ty>, String)>,
    /// `collect_structs` has finished: struct instances made from now on are finished at once
    /// and listed in `late_structs` (after the ordered structs, which they only depend on).
    structs_done: bool,
    late_structs: Vec<usize>,
}

struct IfaceDef {
    name: String,
    /// Method name, signature index, and the span of its name.
    methods: Vec<(String, usize, Span)>,
}

struct StructDef {
    name: String,
    span: Span,
    ty: Ty,
    fields: Vec<(String, Ty, Span)>,
    /// The constant default of each field, if any (`field: T = value`).
    defaults: Vec<Option<(FieldDefault, Span)>>,
    /// For an enum: its cases and their payload fields.
    variants: Vec<VariantDef>,
}

struct VariantDef {
    name: String,
    fields: Vec<(String, Ty, Span)>,
}

/// A field default (`field: T = value`). Only constants are allowed, so a literal that
/// leaves the field out has no hidden side effects or evaluation order to worry about.
#[derive(Clone)]
enum FieldDefault {
    /// An integer literal; `true`: negated.
    Int(u128, bool),
    Float(f64, bool),
    Byte(u8),
    Bool(bool),
    /// A string literal without interpolation; it initializes a `String` field.
    Str(String),
    None,
    EmptyVec,
    EmptyMap,
}

impl FieldDefault {
    fn of(e: &Expr) -> Option<FieldDefault> {
        Some(match &e.kind {
            ExprKind::Paren(inner) => return FieldDefault::of(inner),
            ExprKind::Int(n) => FieldDefault::Int(*n, false),
            ExprKind::Float(f) => FieldDefault::Float(*f, false),
            ExprKind::Byte(b) => FieldDefault::Byte(*b),
            ExprKind::Bool(b) => FieldDefault::Bool(*b),
            ExprKind::Unary { op: UnOp::Neg, expr, .. } => match expr.kind {
                ExprKind::Int(n) => FieldDefault::Int(n, true),
                ExprKind::Float(f) => FieldDefault::Float(f, true),
                _ => return None,
            },
            ExprKind::Str(segs) => {
                let mut text = String::new();
                for s in segs {
                    match s {
                        StrSeg::Text(t) => text.push_str(t),
                        StrSeg::Expr { .. } => return None,
                    }
                }
                FieldDefault::Str(text)
            }
            ExprKind::None => FieldDefault::None,
            ExprKind::VecLit(items) if items.is_empty() => FieldDefault::EmptyVec,
            ExprKind::MapLit(items) if items.is_empty() => FieldDefault::EmptyMap,
            _ => return None,
        })
    }

    /// Whether this constant can initialize a field of type `ty`.
    fn fits(&self, ty: Ty) -> bool {
        let inner = match ty {
            Ty::Opt(o) => opt_inner(o),
            t => t,
        };
        match self {
            FieldDefault::None => matches!(ty, Ty::Opt(_)),
            FieldDefault::Int(..) => matches!(inner, Ty::Int(_) | Ty::Float(_)),
            FieldDefault::Float(..) => matches!(inner, Ty::Float(_)),
            FieldDefault::Byte(_) => matches!(inner, Ty::Int(_)),
            FieldDefault::Bool(_) => inner == Ty::Bool,
            FieldDefault::Str(_) => inner == Ty::String,
            FieldDefault::EmptyVec => matches!(inner, Ty::Vec(_) | Ty::Set(_)),
            FieldDefault::EmptyMap => matches!(inner, Ty::Map(_)),
        }
    }

    /// The source expression for a literal that leaves the field out.
    fn expr(&self, span: Span) -> Expr {
        let e = |kind| Expr { kind, span };
        match self {
            FieldDefault::Int(..) | FieldDefault::Float(..) => {
                let (lit, neg) = match self {
                    FieldDefault::Int(n, neg) => (e(ExprKind::Int(*n)), *neg),
                    FieldDefault::Float(f, neg) => (e(ExprKind::Float(*f)), *neg),
                    _ => unreachable!(),
                };
                if neg { e(ExprKind::Unary { op: UnOp::Neg, op_span: span, expr: Box::new(lit) }) } else { lit }
            }
            FieldDefault::Byte(b) => e(ExprKind::Byte(*b)),
            FieldDefault::Bool(b) => e(ExprKind::Bool(*b)),
            FieldDefault::Str(t) => {
                let lit = e(ExprKind::Str(vec![StrSeg::Text(t.clone())]));
                let callee = e(ExprKind::Field { base: Box::new(lit), name: "toString".into() });
                e(ExprKind::Call { callee: Box::new(callee), args: Vec::new() })
            }
            FieldDefault::None => e(ExprKind::None),
            FieldDefault::EmptyVec => e(ExprKind::VecLit(Vec::new())),
            FieldDefault::EmptyMap => e(ExprKind::MapLit(Vec::new())),
        }
    }
}

/// Enums that standard library modules provide (stdlib contract §5).
const LIBRARY_ENUMS: &[(&str, &[(&str, &[&str])])] = &[
    ("io.Error", &[("Utf8", &["offset"]), ("Io", &["code"])]),
    ("fs.Error", &[("Utf8", &["offset"]), ("Io", &["code"]), ("InvalidPath", &[])]),
    // Prelude (K2): the result of `cmp`. Tags: Less 0, Equal 1, Greater 2.
    ("Ordering", &[("Less", &[]), ("Equal", &[]), ("Greater", &[])]),
];

/// Validate original spellings before a project's private symbols are renamed.
pub fn check_declaration_names(prog: &Program, diags: &mut Diags) {
    for f in &prog.fns {
        if f.owner.is_none() && (OUTPUT_FNS.contains(&f.name.as_str()) || is_type_name(&f.name)) {
            diags.error("E0011", f.name_span, format!("`{}` is a built-in name and cannot be redefined", f.name));
        }
    }
    for (name, span, interface) in prog.structs.iter().map(|s| (&s.name, s.name_span, false))
        .chain(prog.enums.iter().map(|e| (&e.name, e.name_span, false)))
        .chain(prog.interfaces.iter().map(|i| (&i.name, i.name_span, true))) {
        if is_type_name(name) || UNSUPPORTED_TYPES.contains(&name.as_str())
            || matches!(name.as_str(), "Ordering" | "Utf8Error")
            || (interface && matches!(name.as_str(), "Drop" | "Eq" | "Ord" | "Copy" | "Send" | "Sync")) {
            diags.error("E0011", span, format!("`{name}` is a built-in name and cannot be redefined"));
        }
    }
}

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
        ifaces: Vec::new(),
        iface_index: HashMap::new(),
        dyn_slots: HashMap::new(),
        impls: Vec::new(),
        impl_index: HashMap::new(),
        dyn_sigs: Vec::new(),
        prog,
        gen_fns: HashMap::new(),
        gen_structs: HashMap::new(),
        fn_insts: HashMap::new(),
        struct_insts: HashMap::new(),
        inst_of: HashMap::new(),
        subst: HashMap::new(),
        pending: Vec::new(),
        structs_done: false,
        late_structs: Vec::new(),
        declared_impls: Default::default(),
    };
    ck.collect_uses(prog);
    ck.collect_iface_names(prog);
    for (i, f) in prog.fns.iter().enumerate() {
        if !f.type_params.is_empty() {
            ck.gen_fns.insert(f.name.clone(), i);
        }
    }
    for (i, s) in prog.structs.iter().enumerate() {
        if !s.type_params.is_empty() {
            ck.gen_structs.insert(s.name.clone(), i);
        }
    }
    let order = ck.collect_structs(prog);
    ck.collect(prog);
    ck.collect_iface_sigs(prog);
    ck.check_impls(prog);
    ck.check_drops(prog);
    ck.check_protocols(prog);
    let mut fns: Vec<HFn> = prog.fns.iter().enumerate().map(|(i, f)| ck.function_or_template(f, i)).collect();
    // Generic functions are checked once per instantiation; checking one may instantiate more.
    while let Some((idx, di, subst, name)) = ck.pending.pop() {
        let saved = std::mem::replace(&mut ck.subst, subst);
        let mut h = ck.function(&prog.fns[di], idx);
        ck.subst = saved;
        h.name = name;
        ck.lifted[idx - ck.n_fns] = h;
    }
    fns.append(&mut ck.lifted);
    let mut unused: Vec<(Span, String)> =
        ck.imports.iter().filter(|(_, (_, used))| !used).map(|((_, m), (sp, _))| (*sp, m.clone())).collect();
    unused.sort_by_key(|(sp, _)| sp.start);
    for (sp, m) in unused {
        ck.diags.warn("W0003", sp, format!("unused import `std/{m}`")).note("remove it");
    }
    if ck.diags.has_errors() {
        None
    } else {
        let templates: Vec<usize> = ck.gen_structs.values().copied().collect();
        let structs = order
            .into_iter()
            .filter(|i| !templates.contains(i))
            .chain(std::mem::take(&mut ck.late_structs))
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
        let ifaces = ck
            .ifaces
            .iter()
            .enumerate()
            .map(|(i, d)| HIface {
                ty: Ty::Dyn(i as u32),
                methods: d
                    .methods
                    .iter()
                    .map(|(_, s, _)| {
                        let sig = &ck.sig(*s);
                        HIfaceMethod { params: sig.params.clone(), inouts: sig.inouts.clone(), sinks: sig.sinks.clone(), ret: sig.ret }
                    })
                    .collect(),
            })
            .collect();
        Some(HProgram { files: prog.files.clone(), fns, structs, tramps: std::mem::take(&mut ck.tramps), ifaces, impls: std::mem::take(&mut ck.impls) })
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
        ExprKind::Int(_) | ExprKind::Float(_) => true,
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

pub fn is_builtin_namespace(name: &str) -> bool {
    is_type_name(name) || UNSUPPORTED_TYPES.contains(&name)
        || matches!(name, "Ordering" | "Utf8Error") || MODULES.iter().any(|(m, _)| *m == name)
}

fn is_type_name(name: &str) -> bool {
    IntTy::from_name(name).is_some() || FloatTy::from_name(name).is_some() || matches!(name, "bool" | "str" | "String" | "Vec" | "Box" | "Set")
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

fn terminates(stmts: &[Stmt], cx: &FnCx) -> bool {
    stmts.iter().any(|s| stmt_terminates(s, cx))
}

fn stmt_terminates(s: &Stmt, cx: &FnCx) -> bool {
    match &s.kind {
        StmtKind::Return(_) => true,
        // `os.exit(...)` never returns (V8).
        StmtKind::Expr(e) => is_exit_call(e, cx),
        StmtKind::If { then, els: Some(els), .. } | StmtKind::IfLet { then, els: Some(els), .. } => {
            terminates(&then.stmts, cx)
                && match els {
                    Else::Block(b) => terminates(&b.stmts, cx),
                    Else::If(s) => stmt_terminates(s, cx),
                }
        }
        // A `match` that does not cover every case is an error, so its arms are all that runs.
        StmtKind::Match { arms, .. } => !arms.is_empty() && arms.iter().all(|a| terminates(&a.body.stmts, cx)),
        StmtKind::Loop { body } => !contains_break(&body.stmts),
        StmtKind::Block(b) => terminates(&b.stmts, cx),
        StmtKind::With { body, .. } => terminates(&body.stmts, cx),
        StmtKind::Scope { body } => terminates(&body.stmts, cx),
        _ => false,
    }
}

/// Consult name resolution from type checking, including its lexical scope.
fn is_exit_call(e: &Expr, cx: &FnCx) -> bool {
    matches!(e.kind, ExprKind::Call { .. }) && cx.exit_calls.contains(&(e.span.start, e.span.end))
}

/// Whether evaluating `e` can break out of the current loop. A catch or match
/// block can hide the exit inside any operand, argument or constructed value.
/// Closure bodies execute in another function and do not target this loop.
fn expr_breaks(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Break => true,
        ExprKind::Return(value) => value.as_ref().is_some_and(|value| expr_breaks(value)),
        ExprKind::Case { args, .. } => args.iter().flatten().any(expr_breaks),
        ExprKind::Named { value, .. } | ExprKind::Err(value) | ExprKind::Try(value)
        | ExprKind::Spawn(value) | ExprKind::Paren(value) | ExprKind::Borrow(value)
        | ExprKind::Unary { expr: value, .. } | ExprKind::Field { base: value, .. } => expr_breaks(value),
        ExprKind::Catch { value, handler, .. } => expr_breaks(value) || expr_breaks(handler),
        ExprKind::Match { value, arms } => expr_breaks(value) || arms.iter().any(|arm| expr_breaks(&arm.body)),
        ExprKind::Block(block) => contains_break(&block.stmts),
        ExprKind::Str(segments) => segments.iter().any(|segment| {
            matches!(segment, StrSeg::Expr { expr, .. } if expr_breaks(expr))
        }),
        ExprKind::Call { callee, args } => expr_breaks(callee) || args.iter().any(expr_breaks),
        ExprKind::StructLit { fields, .. } => fields.iter().any(|field| expr_breaks(&field.value)),
        ExprKind::VecLit(items) | ExprKind::Tuple(items) => items.iter().any(expr_breaks),
        ExprKind::MapLit(entries) => entries.iter().any(|(key, value)| expr_breaks(key) || expr_breaks(value)),
        ExprKind::Index { base, index } => expr_breaks(base) || expr_breaks(index),
        ExprKind::SliceRange { base, start, end } => expr_breaks(base) || expr_breaks(start) || expr_breaks(end),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Coalesce { lhs, rhs, .. } => expr_breaks(lhs) || expr_breaks(rhs),
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Byte(_) | ExprKind::Bool(_)
        | ExprKind::Ident(_) | ExprKind::Generic { .. } | ExprKind::None | ExprKind::Continue
        | ExprKind::Unit | ExprKind::Closure { .. } => false,
    }
}

/// Whether a `break` in these statements targets the enclosing loop.
fn contains_break(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Break => true,
        StmtKind::Let { init: e, .. } | StmtKind::LetTuple { init: e, .. }
        | StmtKind::Expr(e) | StmtKind::Return(Some(e)) => expr_breaks(e),
        StmtKind::Assign { target, value, .. } => expr_breaks(target) || expr_breaks(value),
        StmtKind::If { cond, then, els } | StmtKind::IfLet { value: cond, then, els, .. } => {
            expr_breaks(cond) || contains_break(&then.stmts)
                || match els {
                    Some(Else::Block(b)) => contains_break(&b.stmts),
                    Some(Else::If(s)) => contains_break(std::slice::from_ref(s.as_ref())),
                    None => false,
                }
        }
        StmtKind::Match { value, arms } => expr_breaks(value) || arms.iter().any(|a| contains_break(&a.body.stmts)),
        StmtKind::Block(b) => contains_break(&b.stmts),
        StmtKind::With { init, body, .. } => expr_breaks(init) || contains_break(&body.stmts),
        StmtKind::Scope { body } => contains_break(&body.stmts),
        // Loop inputs are checked before entering their body (see loop_body).
        // Only their expressions belong to this loop; inner body exits do not.
        StmtKind::While { cond, .. } => expr_breaks(cond),
        StmtKind::For { start, end, .. } => expr_breaks(start) || expr_breaks(end),
        StmtKind::ForIn { iter, .. } => expr_breaks(iter),
        StmtKind::Loop { .. } | StmtKind::Continue | StmtKind::Return(None) => false,
    })
}

impl Checker<'_> {
    /// `use std/m` (stdlib contract §1).
    fn collect_uses(&mut self, prog: &Program) {
        for u in &prog.uses {
            if !u.path.starts_with("std/") {
                self.diags.error("U0001", u.span, "user modules need project loading through check/build/run");
                continue;
            }
            let key = (crate::project::file_id(&prog.files, u.span), u.module.clone());
            if u.path != format!("std/{}", u.module) || !MODULES.iter().any(|(m, _)| *m == u.module) {
                let known: Vec<&str> = MODULES.iter().map(|(m, _)| *m).collect();
                self.diags
                    .error("E0010", u.span, format!("there is no standard library module `std/{}`", u.module))
                    .note(format!("the modules are: {}", known.join(", ")));
            } else if let Some((first, _)) = self.imports.get(&key) {
                let line = first.line;
                self.diags
                    .error("E0011", u.span, format!("`std/{}` is imported more than once", u.module))
                    .note(format!("first imported on line {line}"));
            } else {
                self.imports.insert(key, (u.span, false));
            }
        }
    }

    /// A use of a type that a module provides marks that module's import as used.
    fn mark_type_used(&mut self, name: &str, span: Span) {
        let file = crate::project::file_id(&self.prog.files, span);
        for (m, types) in MODULES {
            if types.contains(&name) {
                if let Some((_, used)) = self.imports.get_mut(&(file, m.to_string())) {
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
        let key = (crate::project::file_id(&self.prog.files, span), m.to_string());
        match self.imports.get_mut(&key) {
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
            if is_type_name(name) || UNSUPPORTED_TYPES.contains(&name) || matches!(name, "Ordering" | "Utf8Error") {
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
            self.structs.push(StructDef { name: name.to_string(), span, ty, fields: Vec::new(), variants: Vec::new(), defaults: Vec::new() });
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
            self.structs.push(StructDef { name: lname.to_string(), span, ty, fields: Vec::new(), variants, defaults: Vec::new() });
        }
        // Prelude (stdlib contract §5): `struct Utf8Error { offset: int }`.
        {
            let ty = Ty::Struct(register_struct("Utf8Error"));
            self.struct_index.insert("Utf8Error".to_string(), self.structs.len());
            let span = Span { start: 0, end: 0, line: 1, col: 1 };
            let fields = vec![("offset".to_string(), Ty::Int(IntTy::I64), span)];
            self.structs.push(StructDef { name: "Utf8Error".to_string(), span, ty, fields, variants: Vec::new(), defaults: Vec::new() });
        }
        for (i, s) in prog.structs.iter().enumerate() {
            // A generic struct's fields are resolved per instance.
            if !s.type_params.is_empty() {
                self.check_type_params(&s.type_params);
                if let Some((n, sp)) = s.impls.first() {
                    self.diags.error("U0001", *sp, format!("a generic struct cannot declare `{n}` yet"));
                }
                self.field_defaults(&s.fields, true);
                continue;
            }
            self.structs[i].fields = self.member_fields(&s.fields, &s.name);
            self.structs[i].defaults = self.field_defaults(&s.fields, true);
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
        self.structs_done = true;
        order
    }

    /// Resolves the fields of a struct or of an enum case. A view cannot be stored (rule 4).
    /// The defaults of `decls` (`field: T = value`). With `report`, a default that is not a
    /// constant, or does not fit the field's type, is an error (reported once per declaration).
    fn field_defaults(&mut self, decls: &[FieldDecl], report: bool) -> Vec<Option<(FieldDefault, Span)>> {
        let mut out = Vec::new();
        for f in decls {
            let Some(e) = &f.default else {
                out.push(None);
                continue;
            };
            match FieldDefault::of(e) {
                Some(d) => {
                    if report {
                        let ty = self.resolve_ty(&f.ty);
                        if ty != Ty::Error && !self.subst.contains_key(&f.ty.name) && !d.fits(ty) {
                            self.diags.error("E0001", e.span, format!("this default does not fit the type of field `{}` (`{ty}`)", f.name));
                        }
                    }
                    out.push(Some((d, e.span)));
                }
                None => {
                    if report {
                        self.diags
                            .error("E0001", e.span, format!("the default of field `{}` must be a constant", f.name))
                            .note("allowed: a number, `true`/`false`, a byte, a string literal, `none`, `[]` or `[:]`");
                    }
                    out.push(None);
                }
            }
        }
        out
    }

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

    fn struct_lit(&mut self, name: &str, name_span: Span, targs: &[TypeExpr], inits: &[FieldInit], span: Span, cx: &mut FnCx) -> HExpr {
        if name == "Arena" {
            let ty = self.resolve_ty(&TypeExpr { name: name.into(), span: name_span, args: targs.to_vec() });
            if let Some(init) = inits.first() {
                self.diags.error("E0001", init.value.span, "`Arena[T]{}` takes no fields; insert elements with `add`");
                return err_expr(span);
            }
            return HExpr { kind: HExprKind::ArenaNew, ty, span, at: span };
        }
        if name == "Id" {
            self.diags.error("E0001", name_span, "an Id is produced by `Arena.add`; it cannot be constructed directly");
            return err_expr(span);
        }
        if name == "String" && inits.is_empty() {
            return HExpr { kind: HExprKind::EmptyString, ty: Ty::String, span, at: span };
        }
        // The built-in empty constructor has the same element rules as a typed [].
        if name == "Vec" {
            let ty = self.resolve_ty(&TypeExpr { name: name.into(), span: name_span, args: targs.to_vec() });
            if let Some(init) = inits.first() {
                self.diags.error("E0001", init.value.span, "`Vec[T]{}` takes no fields; insert elements with `push`");
                return err_expr(span);
            }
            return self.vec_lit(&[], Some(ty), span, cx);
        }
        if name == "Deque" {
            let ty = self.resolve_ty(&TypeExpr { name: name.into(), span: name_span, args: targs.to_vec() });
            if let Some(init) = inits.first() {
                self.diags.error("E0001", init.value.span, "`Deque[T]{}` takes no fields; insert elements with `pushBack` or `pushFront`");
                return err_expr(span);
            }
            return HExpr { kind: HExprKind::DequeNew, ty, span, at: span };
        }
        // `Set[T]{}`: an empty set.
        if name == "Set" {
            let te = TypeExpr { name: name.to_string(), span: name_span, args: targs.to_vec() };
            let ty = self.resolve_ty(&te);
            if let Some(init) = inits.first() {
                self.diags.error("E0001", init.value.span, "`Set[T]{}` takes no fields; add elements with `add`");
                return err_expr(span);
            }
            if ty == Ty::Error {
                return err_expr(span);
            }
            return HExpr { kind: HExprKind::MapLit(Vec::new()), ty, span, at: span };
        }
        // `Wrapper[String]{...}`, or a generic struct's own name inside its methods.
        let generic = if !targs.is_empty() || self.subst.contains_key(name) || self.gen_structs.contains_key(name) {
            let te = TypeExpr { name: name.to_string(), span: name_span, args: targs.to_vec() };
            match self.resolve_ty(&te) {
                t @ Ty::Struct(_) => self.structs.iter().position(|s| s.ty == t),
                Ty::Sync(_) => {
                    let te = TypeExpr { name: name.to_string(), span: name_span, args: targs.to_vec() };
                    if let t @ Ty::Sync(id) = self.resolve_ty(&te) {
                        if sync_parts(id).0 == SyncKind::Channel {
                            return self.chan_new(t, inits, name_span, span, cx);
                        }
                    }
                    for init in inits {
                        self.expr(&init.value, None, cx);
                    }
                    self.diags.error("U0001", name_span, format!("constructing `{name}` with `{{...}}` is not supported yet"));
                    return err_expr(span);
                }
                Ty::Error => {
                    for init in inits {
                        self.expr(&init.value, Some(Ty::Error), cx);
                    }
                    return err_expr(span);
                }
                other => {
                    self.diags.error("E0001", name_span, format!("`{other}` is not a struct"));
                    return err_expr(span);
                }
            }
        } else {
            None
        };
        let Some(si) = generic.or_else(|| self.struct_index.get(name).copied()) else {
            if UNSUPPORTED_TYPES.contains(&name) {
                self.mark_type_used(name, name_span);
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
        // A field with a declared default takes it, evaluated afresh for each literal.
        for fi in 0..decl.len() {
            if seen[fi] {
                continue;
            }
            let Some((d, dspan)) = self.structs[si].defaults.get(fi).cloned().flatten() else { continue };
            seen[fi] = true;
            let fty = decl[fi].1;
            let e = d.expr(dspan);
            let h = self.expr(&e, Some(fty), cx);
            let h = self.fit(h, fty, cx);
            let h = self.consume(h, cx);
            out.push((fi, h));
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
                .note("give every field that has no declared default (`field: T = value`); optional fields default to `none`");
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

    /// Registers interface names, so that `Box[Named]` resolves in struct fields.
    fn collect_iface_names(&mut self, prog: &Program) {
        for d in &prog.interfaces {
            let name = d.name.as_str();
            if is_type_name(name)
                || UNSUPPORTED_TYPES.contains(&name)
                || matches!(name, "Ordering" | "Utf8Error" | "Drop" | "Eq" | "Ord" | "Copy" | "Send" | "Sync")
            {
                self.diags.error("E0011", d.name_span, format!("`{name}` is a built-in name and cannot be redefined"));
            } else if self.iface_index.contains_key(name) || prog.structs.iter().any(|s| s.name == name) || prog.enums.iter().any(|e| e.name == name) {
                self.diags.error("E0011", d.name_span, format!("type `{name}` is defined more than once"));
            } else {
                self.iface_index.insert(name.to_string(), self.ifaces.len());
            }
            // Every declaration gets an id, even a duplicate, so ids and indexes stay equal.
            register_iface(name);
            self.ifaces.push(IfaceDef { name: name.to_string(), methods: Vec::new() });
        }
    }

    /// Resolves interface method signatures. Each becomes a signature after the user
    /// functions; its receiver (parameter 0) has the interface value's type.
    fn collect_iface_sigs(&mut self, prog: &Program) {
        for (ii, d) in prog.interfaces.iter().enumerate() {
            for m in &d.methods {
                let short = m.name.rsplit('.').next().unwrap_or(&m.name).to_string();
                if self.ifaces[ii].methods.iter().any(|(n, _, _)| *n == short) {
                    self.diags.error("E0011", m.name_span, format!("method `{short}` is declared more than once in `{}`", d.name));
                    continue;
                }
                if m.params.first().is_none_or(|p| p.name != "self") {
                    self.diags
                        .error("E0001", m.name_span, format!("interface method `{}` must take `self` (or `inout self`) first", m.name))
                        .note("an interface describes what a value can do; functions without `self` belong to a type");
                    continue;
                }
                let mut params = vec![Ty::Dyn(ii as u32)];
                params.extend(m.params.iter().skip(1).map(|p| self.resolve_ty(&p.ty)));
                let ret = m.ret.as_ref().map(|t| self.local_ty(t)).unwrap_or(Ty::Unit);
                self.check_ret_of(m, ret);
                let ret_of = m.ret_of.as_ref().map(|names| {
                    names.iter().filter_map(|(n, _)| m.params.iter().position(|p| p.name == *n)).collect()
                });
                let sig = Sig {
                    params,
                    sinks: m.params.iter().map(|p| p.sink).collect(),
                    inouts: m.params.iter().map(|p| p.inout).collect(),
                    ret,
                    ret_of,
                    has_self: true,
                };
                let s = DYN_BASE + self.dyn_sigs.len();
                self.dyn_slots.insert(s, (ii, self.ifaces[ii].methods.len()));
                self.ifaces[ii].methods.push((short, s, m.name_span));
                self.dyn_sigs.push(sig);
            }
        }
    }

    /// `fn name(self, int, inout Vec[int]) -> String`, for diagnostics.
    fn sig_text(&self, name: &str, s: usize) -> String {
        let sig = &self.sig(s);
        let mut parts = Vec::new();
        for (i, t) in sig.params.iter().enumerate() {
            let mode = if sig.inouts[i] {
                "inout "
            } else if sig.sinks[i] {
                "sink "
            } else {
                ""
            };
            if i == 0 && sig.has_self {
                parts.push(format!("{mode}self"));
            } else {
                parts.push(format!("{mode}{t}"));
            }
        }
        let ret = if sig.ret == Ty::Unit { String::new() } else { format!(" -> {}", sig.ret) };
        format!("fn {name}({}){ret}", parts.join(", "))
    }

    /// `struct T: I`: `T` must have every method of `I`, with the same signature (receiver
    /// mode, parameter types and modes, return type).
    fn check_impls(&mut self, prog: &Program) {
        let decls = prog.structs.iter().map(|s| (&s.name, &s.impls)).chain(prog.enums.iter().map(|e| (&e.name, &e.impls)));
        for (owner, impls) in decls {
            let Some(&si) = self.struct_index.get(owner.as_str()) else { continue };
            let ty = self.structs[si].ty;
            for (iname, sp) in impls {
                match iname.as_str() {
                    "Drop" | "Eq" | "Ord" => continue,
                    "Copy" | "Send" | "Sync" => {
                        self.diags.error("U0001", *sp, format!("declaring `{owner}: {iname}` is not supported yet"));
                        continue;
                    }
                    _ => {}
                }
                let Some(&ii) = self.iface_index.get(iname.as_str()) else {
                    self.diags.error("E0010", *sp, format!("cannot find interface `{iname}`"));
                    continue;
                };
                self.declared_impls.insert((ty, ii));
                let mut fns = Vec::new();
                let methods = self.ifaces[ii].methods.clone();
                for (mname, s, _) in &methods {
                    let full = format!("{owner}.{mname}");
                    let want = self.sig_text(mname, *s);
                    let Some(&fi) = self.index.get(&full) else {
                        self.diags
                            .error("E0019", *sp, format!("`{owner}` declares `{iname}` but has no method `{mname}`"))
                            .note(format!("add `{want}` to `{owner}`'s body"));
                        continue;
                    };
                    let (a, b) = (&self.sig(fi), &self.sig(*s));
                    let same = a.has_self
                        && a.params.len() == b.params.len()
                        && a.params[1..] == b.params[1..]
                        && a.inouts == b.inouts
                        && a.sinks == b.sinks
                        && a.ret == b.ret;
                    if !same {
                        let span = prog.fns[fi].name_span;
                        self.diags
                            .error("E0019", span, format!("`{full}` does not match `{iname}.{mname}`"))
                            .note(format!("`{iname}` requires `{want}`"));
                        continue;
                    }
                    fns.push(fi);
                }
                if fns.len() == methods.len() {
                    self.impl_index.insert((ty, ii), self.impls.len());
                    self.impls.push(HImpl { ty, iface: ii, fns });
                }
            }
        }
    }

    /// A `Box[T]` used where a `Box[I]` is expected becomes an interface value when `T`
    /// implements `I` (section 8 whitelist); anything else is left for the type check.
    fn to_dyn(&mut self, h: HExpr, iface: u32, cx: &mut FnCx) -> HExpr {
        // An interface value with more capabilities can be used as one with fewer (P2).
        if let Ty::Dyn(j) = h.ty {
            if j != iface && dyn_base(j) == dyn_base(iface) {
                let ((s1, y1), (s2, y2)) = (dyn_caps(j), dyn_caps(iface));
                if (s1 || !s2) && (y1 || !y2) {
                    return HExpr { ty: Ty::Dyn(iface), ..h };
                }
            }
            return h;
        }
        let Ty::Box(b) = h.ty else { return h };
        let inner = box_inner(b);
        let Some(&imp) = self.impl_index.get(&(inner, dyn_base(iface) as usize)) else { return h };
        // Boxing as `Box[Named + Send]` checks the concrete type (P2).
        let (send, sync) = dyn_caps(iface);
        if (send || sync) && self.heap_thread_gap(inner, h.span) {
            return err_expr(h.span);
        }
        if send && !self.cap_ok(inner, false, &mut Vec::new()) {
            self.diags.error("E0701", h.span, format!("`{inner}` is not `Send`, so it cannot be boxed as `{}`", Ty::Dyn(iface)));
        } else if sync && !self.cap_ok(inner, true, &mut Vec::new()) {
            self.diags.error("E0702", h.span, format!("`{inner}` is not `Sync`, so it cannot be boxed as `{}`", Ty::Dyn(iface)));
        }
        let (span, at) = (h.span, h.at);
        let h = self.consume(h, cx);
        HExpr { kind: HExprKind::ToDyn { value: Box::new(h), imp }, ty: Ty::Dyn(iface), span, at }
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
            // A generic function, or a method of a generic struct, gets a signature per
            // instance; its own slot keeps a placeholder so that indexes stay aligned.
            let template = !f.type_params.is_empty() || f.owner.as_ref().is_some_and(|o| self.gen_structs.contains_key(o));
            if !f.type_params.is_empty() && f.owner.is_some() {
                self.diags.error("U0001", f.type_params[0].span, "generic methods are not supported yet; make the struct generic instead");
            }
            if !f.type_params.is_empty() {
                self.check_type_params(&f.type_params);
            }
            let sig = if template {
                Sig { params: Vec::new(), sinks: Vec::new(), inouts: Vec::new(), ret: Ty::Unit, ret_of: None, has_self: false }
            } else {
                self.make_sig(f)
            };
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
            self.sigs.push(sig);
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
                if !f.params.is_empty() || self.sig(i).ret != Ty::Unit || !f.type_params.is_empty() {
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
        self.resolve_ty_inner(t)
    }

    /// The signature of function `i` (a user function, a lifted function) or of an interface
    /// method slot (`DYN_BASE` and up).
    fn sig(&self, i: usize) -> &Sig {
        if i >= DYN_BASE {
            &self.dyn_sigs[i - DYN_BASE]
        } else {
            &self.sigs[i]
        }
    }

    /// Resolves a function's parameter and return types (under the current substitution).
    fn make_sig(&mut self, f: &FnDecl) -> Sig {
        let params = f.params.iter().map(|p| self.resolve_ty(&p.ty)).collect();
        let sinks = f.params.iter().map(|p| p.sink).collect();
        let inouts = f.params.iter().map(|p| p.inout).collect();
        let ret = f.ret.as_ref().map(|t| self.local_ty(t)).unwrap_or(Ty::Unit);
        let ret_of = f.ret_of.as_ref().map(|names| {
            names.iter().filter_map(|(n, _)| f.params.iter().position(|p| p.name == *n)).collect()
        });
        let has_self = f.owner.is_some() && f.params.first().is_some_and(|p| p.name == "self");
        Sig { params, sinks, inouts, ret, ret_of, has_self }
    }

    /// Type parameter names must be distinct, and bounds must be protocols or interfaces.
    fn check_type_params(&mut self, tps: &[TypeParam]) {
        for (i, tp) in tps.iter().enumerate() {
            if tps[..i].iter().any(|o| o.name == tp.name) {
                self.diags.error("E0011", tp.span, format!("type parameter `{}` is declared more than once", tp.name));
            }
            if is_type_name(&tp.name) || self.struct_index.contains_key(&tp.name) {
                self.diags.error("E0011", tp.span, format!("type parameter `{}` has the name of a type", tp.name));
            }
            for (b, sp) in &tp.bounds {
                match b.as_str() {
                    "Copy" | "Eq" | "Ord" => {}
                    "Send" | "Sync" => {
                        self.diags.error("U0001", *sp, format!("the bound `{b}` is not supported yet"));
                    }
                    _ if self.iface_index.contains_key(b) => {}
                    _ => {
                        self.diags.error("E0010", *sp, format!("cannot find the bound `{b}` (a protocol such as `Ord`, or an interface)"));
                    }
                }
            }
        }
    }

    /// A user function; a generic one (or a method of a generic struct) only has a placeholder
    /// here, since it is checked per instance.
    fn function_or_template(&mut self, f: &FnDecl, idx: usize) -> HFn {
        let template = !f.type_params.is_empty() || f.owner.as_ref().is_some_and(|o| self.gen_structs.contains_key(o));
        if template {
            let name = format!("template.{idx}");
            return HFn { name, params: Vec::new(), sinks: Vec::new(), ret: Ty::Unit, locals: Vec::new(), body: Vec::new() };
        }
        self.function(f, idx)
    }

    /// Whether `t` satisfies the bound `b` (K1, K2; an interface bound needs `struct T: I`).
    fn satisfies(&self, t: Ty, b: &str) -> bool {
        match b {
            "Copy" => t.is_copy(),
            "Eq" | "Ord" => {
                let ord = b == "Ord";
                match t {
                    Ty::Int(_) | Ty::Bool | Ty::Str | Ty::String | Ty::Id(_) | Ty::Error => true,
                    Ty::Tuple(id) => tuple_elems(id).iter().all(|e| self.satisfies(*e, b)),
                    Ty::Struct(_) | Ty::Enum(_) => match self.protocol(t) {
                        Some((cmp, eq)) => cmp.is_some() || (!ord && eq.is_some()),
                        // Enums whose cases carry no payload compare by case (`==` only).
                        None => !ord && self.ctors_of(t).is_some_and(|cs| cs.iter().all(|c| c.is_empty())) && matches!(t, Ty::Enum(_)),
                    },
                    _ => false,
                }
            }
            _ => match self.iface_index.get(b) {
                Some(&ii) => self.impl_index.contains_key(&(t, ii)),
                None => true, // reported at the declaration
            },
        }
    }

    /// Checks `args` against the bounds of `tps`; reports each failure at `span`.
    fn check_bounds(&mut self, what: &str, tps: &[TypeParam], args: &[Ty], span: Span) -> bool {
        let mut ok = true;
        for (tp, &t) in tps.iter().zip(args) {
            for (b, _) in &tp.bounds {
                if self.satisfies(t, b) {
                    continue;
                }
                ok = false;
                let code = if matches!(b.as_str(), "Eq" | "Ord") { "E0501" } else { "E0001" };
                let d = self.diags.error(code, span, format!("`{t}` does not satisfy `{}: {b}`, which `{what}` requires", tp.name));
                if b == "Copy" {
                    d.note("pass a copyable type, or remove the `Copy` bound and borrow the values instead");
                } else if self.iface_index.contains_key(b) {
                    d.note(format!("declare it as `struct {t}: {b}`"));
                }
            }
        }
        ok
    }

    /// `Name[args]` for a generic struct: its instance, made on first use. A view cannot be a
    /// type argument (E2: `Wrapper[str]` is rejected).
    fn instantiate_struct(&mut self, di: usize, args: Vec<Ty>, span: Span) -> Ty {
        let decl = &self.prog.structs[di];
        if args.contains(&Ty::Error) {
            return Ty::Error;
        }
        if args.len() != decl.type_params.len() {
            self.diags.error(
                "E0001",
                span,
                format!("`{}` takes {}, but {} were given", decl.name, plural(decl.type_params.len(), "type argument"), args.len()),
            );
            return Ty::Error;
        }
        if let Some(&t) = self.struct_insts.get(&(di, args.clone())) {
            return t;
        }
        let shown: Vec<String> = args.iter().map(|t| t.to_string()).collect();
        let inst_name = format!("{}[{}]", decl.name, shown.join(", "));
        if let Some(&t) = args.iter().find(|t| t.carries_borrow()) {
            self.diags
                .error("E0301", span, format!("`{inst_name}` cannot be made: a generic struct cannot hold a `{t}`, which is a view"))
                .note("use an owned type argument, e.g. `String` (call `.toString()`)");
            return Ty::Error;
        }
        if !self.check_bounds(&decl.name, &decl.type_params, &args, span) {
            return Ty::Error;
        }
        let ty = Ty::Struct(register_struct(&inst_name));
        let si = self.structs.len();
        self.structs.push(StructDef { name: inst_name.clone(), span: decl.name_span, ty, fields: Vec::new(), variants: Vec::new(), defaults: Vec::new() });
        // Registered before the fields are resolved, so `?Box[List[T]]` inside `List[T]` works.
        self.struct_insts.insert((di, args.clone()), ty);
        self.inst_of.insert(ty, (di, args.clone()));
        let mut subst: HashMap<String, Ty> = decl.type_params.iter().map(|tp| tp.name.clone()).zip(args.iter().copied()).collect();
        subst.insert(decl.name.clone(), ty);
        let saved = std::mem::replace(&mut self.subst, subst);
        let fields = self.member_fields(&decl.fields, &inst_name);
        self.subst = saved;
        self.structs[si].fields = fields;
        self.structs[si].defaults = self.field_defaults(&decl.fields, false);
        if self.structs_done {
            let (Ty::Struct(id) | Ty::Enum(id)) = ty else { unreachable!() };
            finish_agg(id, vec![self.structs[si].fields.iter().map(|f| f.1).collect()]);
            self.late_structs.push(si);
        }
        ty
    }

    /// The instance of generic function `di` (or of a method of a generic struct) for
    /// `args`: a function index, reserved now and checked later.
    fn instantiate_fn(&mut self, di: usize, args: Vec<Ty>, extra: Option<(String, Ty)>, span: Span) -> usize {
        let key = (di, args.clone());
        if let Some(&fi) = self.fn_insts.get(&key) {
            return fi;
        }
        let decl = &self.prog.fns[di];
        let mut subst: HashMap<String, Ty> = HashMap::new();
        let mut keys: Vec<String> = Vec::new();
        match &extra {
            // A method of a generic struct: the struct's parameters, its name and `Self`.
            Some((owner, inst)) => {
                let sdecl = &self.prog.structs[self.gen_structs[owner]];
                subst.extend(sdecl.type_params.iter().map(|tp| tp.name.clone()).zip(args.iter().copied()));
                subst.insert(owner.clone(), *inst);
                keys.push(inst.key());
            }
            None => {
                if !self.check_bounds(&decl.name, &decl.type_params, &args, span) {
                    return NO_FN;
                }
                subst.extend(decl.type_params.iter().map(|tp| tp.name.clone()).zip(args.iter().copied()));
                keys.extend(args.iter().map(|t| t.key()));
            }
        }
        let idx = self.n_fns + self.lifted.len();
        debug_assert_eq!(idx, self.sigs.len());
        let short = decl.name.rsplit('.').next().unwrap_or(&decl.name).to_string();
        let name = match &extra {
            Some(_) => format!("{}.{short}", keys.join("_")),
            None => format!("{}.{}", decl.name, keys.join("_")),
        };
        self.lifted.push(HFn { name: name.clone(), params: Vec::new(), sinks: Vec::new(), ret: Ty::Unit, locals: Vec::new(), body: Vec::new() });
        let saved = std::mem::replace(&mut self.subst, subst.clone());
        let sig = self.make_sig(decl);
        self.subst = saved;
        self.sigs.push(sig);
        self.fn_insts.insert(key, idx);
        self.pending.push((idx, di, subst, name));
        idx
    }

    /// A call of generic function `di`: explicit type arguments, or inferred from the
    /// arguments' types.
    fn generic_call(&mut self, di: usize, targs: Option<&[TypeExpr]>, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let name = self.prog.fns[di].name.clone();
        let Some(fi) = self.prepare_generic_fn(di, targs, args, at, cx) else {
            for a in args {
                self.expr(a, None, cx);
            }
            return err_expr(span);
        };
        self.call_fn(fi, &name, None, args, span, at, cx)
    }

    /// Resolve a generic signature without evaluating or borrowing its arguments.
    /// Address type preparation uses this before the assignment's right side; the
    /// normal call later checks the arguments exactly once, in execution order.
    fn prepare_generic_fn(&mut self, di: usize, targs: Option<&[TypeExpr]>, args: &[Expr], at: Span, cx: &FnCx) -> Option<usize> {
        let decl = &self.prog.fns[di];
        let name = decl.name.clone();
        let tys = match targs {
            Some(ts) => {
                let tys: Vec<Ty> = ts.iter().map(|t| self.resolve_ty(t)).collect();
                if tys.len() != decl.type_params.len() {
                    self.diags.error(
                        "E0001",
                        at,
                        format!("`{name}` takes {}, but {} were given", plural(decl.type_params.len(), "type argument"), tys.len()),
                    );
                    None
                } else if tys.contains(&Ty::Error) {
                    None
                } else {
                    Some(tys)
                }
            }
            None => self.infer(di, args, at, cx),
        };
        let tys = tys?;
        let fi = self.instantiate_fn(di, tys, None, at);
        (fi != NO_FN).then_some(fi)
    }

    /// Infers the type arguments of generic function `di` from its arguments' types.
    fn infer(&mut self, di: usize, args: &[Expr], at: Span, cx: &FnCx) -> Option<Vec<Ty>> {
        let decl = &self.prog.fns[di];
        let names: Vec<String> = decl.type_params.iter().map(|tp| tp.name.clone()).collect();
        let mut bound: Vec<Option<Ty>> = vec![None; names.len()];
        for (p, a) in decl.params.iter().zip(args) {
            let a = match (&a.kind, p.inout) {
                (ExprKind::Borrow(inner), true) => inner.as_ref(),
                _ => a,
            };
            let Some(t) = self.peek_ty(a, cx) else { continue };
            if let Err((n, a1, a2)) = self.unify(&p.ty, t, &names, &mut bound) {
                self.diags
                    .error("E0001", a.span, format!("`{}` is used as both `{a1}` and `{a2}` in this call of `{}`", n, decl.name))
                    .note(format!("write the type arguments, e.g. `{}[{a1}](...)`", decl.name));
                return None;
            }
        }
        if let Some(i) = bound.iter().position(|b| b.is_none()) {
            let shown = names.join(", ");
            self.diags
                .error("E0001", at, format!("cannot tell what `{}` is in this call of `{}`", names[i], decl.name))
                .note(format!("write the type arguments: `{}[{shown}](...)` with types in place of {shown}", decl.name));
            return None;
        }
        Some(bound.into_iter().map(|b| b.unwrap()).collect())
    }

    /// Matches a parameter type written with type parameters against an argument's type.
    fn unify(&self, te: &TypeExpr, t: Ty, names: &[String], bound: &mut [Option<Ty>]) -> Result<(), (String, Ty, Ty)> {
        if te.args.is_empty() {
            if let Some(i) = names.iter().position(|n| *n == te.name) {
                // A value read through a reference stands for its target.
                let t = match t {
                    Ty::Ref(r) => ref_inner(r),
                    Ty::RefMut(r) => ref_mut_inner(r),
                    t => t,
                };
                return match bound[i] {
                    None => {
                        bound[i] = Some(t);
                        Ok(())
                    }
                    Some(b) if b == t => Ok(()),
                    Some(b) => Err((te.name.clone(), b, t)),
                };
            }
            return Ok(());
        }
        let t = match t {
            Ty::Ref(r) if te.name != "&" => ref_inner(r),
            Ty::RefMut(r) => ref_mut_inner(r),
            t => t,
        };
        let sub = |s: &Self, i: usize, t: Ty, bound: &mut [Option<Ty>]| s.unify(&te.args[i], t, names, bound);
        match (te.name.as_str(), t) {
            ("[]" | "Vec", Ty::Vec(v)) => sub(self, 0, vec_elem(v), bound),
            ("Deque", Ty::Deque(v)) | ("Heap", Ty::Heap(v)) => sub(self, 0, vec_elem(v), bound),
            ("Arena", Ty::Arena(v)) | ("Id", Ty::Id(v)) => sub(self, 0, vec_elem(v), bound),
            ("[]", Ty::Slice(s)) => sub(self, 0, slice_elem(s), bound),
            ("?", Ty::Opt(o)) => sub(self, 0, opt_inner(o), bound),
            // A `T` passed where `?T` is expected is wrapped.
            ("?", t) => sub(self, 0, t, bound),
            ("&", Ty::Ref(r)) => sub(self, 0, ref_inner(r), bound),
            ("&", t) => sub(self, 0, t, bound),
            ("Box", Ty::Box(b)) => sub(self, 0, box_inner(b), bound),
            ("Map", Ty::Map(m)) => {
                let (k, v) = map_parts(m);
                sub(self, 0, k, bound)?;
                sub(self, 1, v, bound)
            }
            ("Set", Ty::Set(m)) => sub(self, 0, map_parts(m).0, bound),
            ("!", Ty::Res(r)) => {
                let (o, e) = res_parts(r);
                sub(self, 0, o, bound)?;
                sub(self, 1, e, bound)
            }
            ("(,)", Ty::Tuple(id)) => {
                let elems = tuple_elems(id);
                for (i, e) in elems.into_iter().enumerate().take(te.args.len()) {
                    sub(self, i, e, bound)?;
                }
                Ok(())
            }
            (n, Ty::Struct(_)) => match self.inst_of.get(&t) {
                Some((di, targs)) if self.prog.structs[*di].name == n => {
                    for (i, a) in targs.clone().into_iter().enumerate().take(te.args.len()) {
                        sub(self, i, a, bound)?;
                    }
                    Ok(())
                }
                _ => Ok(()),
            },
            _ => Ok(()),
        }
    }

    /// The type of `e`, worked out without checking it (no diagnostics, no moves), for type
    /// argument inference; `None` when that needs a full check.
    fn peek_ty(&self, e: &Expr, cx: &FnCx) -> Option<Ty> {
        Some(match &e.kind {
            ExprKind::Ident(n) => cx.locals[self.lookup(cx, n)?].ty,
            ExprKind::Paren(inner) => self.peek_ty(inner, cx)?,
            ExprKind::Int(_) => Ty::Int(IntTy::I64),
            ExprKind::Byte(_) => Ty::Int(IntTy::U8),
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Str(_) => Ty::Str,
            ExprKind::Unary { op: UnOp::Neg, expr, .. } => self.peek_ty(expr, cx)?,
            ExprKind::Borrow(inner) => Ty::Ref(ref_of(self.peek_ty(inner, cx)?)),
            ExprKind::Field { base, name } => {
                let mut t = self.peek_ty(base, cx)?;
                loop {
                    t = match t {
                        Ty::Ref(r) => ref_inner(r),
                        Ty::RefMut(r) => ref_mut_inner(r),
                        Ty::Box(b) => box_inner(b),
                        _ => break,
                    };
                }
                match t {
                    Ty::Tuple(id) => *tuple_elems(id).get(name.parse::<usize>().ok()?)?,
                    _ => self.struct_field(t, name)?.1,
                }
            }
            ExprKind::Index { base, .. } => match self.peek_ty(base, cx)? {
                Ty::Ref(r) => seq_elem(ref_inner(r))?,
                t => seq_elem(t)?,
            },
            ExprKind::VecLit(items) => Ty::Vec(vec_of(self.peek_ty(items.first()?, cx)?)),
            ExprKind::Tuple(items) => {
                let elems: Option<Vec<Ty>> = items.iter().map(|i| self.peek_ty(i, cx)).collect();
                Ty::Tuple(tuple_of(elems?))
            }
            ExprKind::StructLit { name, targs, .. } if targs.is_empty() => {
                if let Some(&t) = self.subst.get(name) {
                    t
                } else {
                    self.structs[*self.struct_index.get(name)?].ty
                }
            }
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Ident(n) if self.lookup(cx, n).is_none() && !self.gen_fns.contains_key(n) => match n.as_str() {
                    "Box" => return None,
                    _ => self.sig(*self.index.get(n)?).ret,
                },
                ExprKind::Field { base, name } => match (name.as_str(), self.peek_ty(base, cx)) {
                    ("toString" | "toLower" | "toUpper", _) => Ty::String,
                    ("len", _) => Ty::Int(IntTy::I64),
                    ("clone", Some(t)) => match t {
                        Ty::Ref(r) => ref_inner(r),
                        t => t,
                    },
                    ("trim", _) => Ty::Str,
                    _ => return None,
                },
                _ => return None,
            },
            _ => return None,
        })
    }

    fn resolve_ty_inner(&mut self, t: &TypeExpr) -> Ty {
        // A type parameter (or, in a generic struct's method, the struct's own name).
        if t.args.is_empty() {
            if let Some(&ty) = self.subst.get(&t.name) {
                return ty;
            }
        }
        if let Some(&di) = self.gen_structs.get(&t.name) {
            if t.args.is_empty() {
                let n = self.prog.structs[di].type_params.len();
                self.diags
                    .error("E0001", t.span, format!("`{}` is generic: write its type arguments, e.g. `{}[{}]`", t.name, t.name, vec!["int"; n].join(", ")));
                return Ty::Error;
            }
            let args: Vec<Ty> = t.args.iter().map(|a| self.resolve_ty(a)).collect();
            return self.instantiate_struct(di, args, t.span);
        }
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
        if t.name == "&inout" {
            self.diags
                .error("U0001", t.span, "`&inout T` is supported for local variables and return values; explicit parameter and nested types are not supported yet")
                .note("use an `inout` parameter to modify the caller's value");
            return Ty::Error;
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
                    .note("keys can be integers, bool, str, String or Id");
                return Ty::Error;
            }
            return Ty::Map(map_of(k_ty, v_ty));
        }
        if t.name == "Set" {
            let [k] = t.args.as_slice() else {
                self.diags.error("E0001", t.span, "`Set` takes one type argument, e.g. `Set[String]`");
                return Ty::Error;
            };
            let k_ty = self.resolve_ty(k);
            if k_ty == Ty::Error {
                return Ty::Error;
            }
            if !k_ty.is_hashable() {
                self.diags
                    .error("E0501", k.span, format!("`{k_ty}` cannot be a Set element"))
                    .note("elements can be integers, bool, str, String or Id");
                return Ty::Error;
            }
            return Ty::Set(map_of(k_ty, Ty::Bool));
        }
        let sync_kind = match t.name.as_str() {
            "Rc" => Some(SyncKind::Rc),
            "Arc" => Some(SyncKind::Arc),
            "Mutex" => Some(SyncKind::Mutex),
            "Guard" => Some(SyncKind::Guard),
            "Channel" => Some(SyncKind::Channel),
            _ => None,
        };
        if let Some(kind) = sync_kind {
            if matches!(kind, SyncKind::Mutex | SyncKind::Guard | SyncKind::Channel) && !self.need_sync(&t.name, t.span) {
                return Ty::Error;
            }
            let [a] = t.args.as_slice() else {
                self.diags.error("E0001", t.span, format!("`{}` takes one type argument, e.g. `{}[int]`", t.name, t.name));
                return Ty::Error;
            };
            let inner = self.resolve_ty(a);
            if inner == Ty::Error {
                return Ty::Error;
            }
            if inner.carries_borrow() {
                if kind == SyncKind::Channel {
                    self.diags
                        .error("E0704", t.span, format!("a channel cannot carry `{inner}`: its values point into data owned by the sender"))
                        .note("send owned values, e.g. `Vec[String]` (call `.toString()` on each view)");
                } else {
                    self.diags
                        .error("E0301", a.span, format!("a `{}` cannot hold a `{inner}`: a view cannot be stored in it", t.name))
                        .note("store owned values instead, e.g. `String`");
                }
                return Ty::Error;
            }
            return Ty::Sync(sync_of(kind, inner));
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
        if matches!(t.name.as_str(), "Arena" | "Id") {
            let [arg] = t.args.as_slice() else {
                self.diags.error("E0001", t.span, format!("`{}` takes one element type", t.name));
                return Ty::Error;
            };
            let elem = self.resolve_ty(arg);
            if matches!(elem, Ty::Error | Ty::Unit) {
                if elem == Ty::Unit { self.diags.error("E0001", arg.span, "an Arena element cannot be `()`"); }
                return Ty::Error;
            }
            return if t.name == "Arena" { Ty::Arena(arena_of(elem)) } else { Ty::Id(vec_of(elem)) };
        }
        if matches!(t.name.as_str(), "Heap" | "Deque") {
            if !self.module_ref("collections", t.span) {
                self.diags.error("E0010", t.span, format!("`{}` comes from `std/collections`, which is not imported", t.name))
                    .note("add `use std/collections`");
                return Ty::Error;
            }
            let [arg] = t.args.as_slice() else {
                self.diags.error("E0001", t.span, format!("`{}` takes one element type", t.name));
                return Ty::Error;
            };
            let elem = self.resolve_ty(arg);
            if matches!(elem, Ty::Error | Ty::Unit) {
                if elem == Ty::Unit { self.diags.error("E0001", arg.span, "a collection element cannot be `()`"); }
                return Ty::Error;
            }
            return if t.name == "Heap" { Ty::Heap(heap_of(elem)) } else { Ty::Deque(deque_of(elem)) };
        }
        if t.name == "Box" {
            let [inner] = t.args.as_slice() else {
                self.diags.error("E0001", t.span, "`Box` takes exactly one type argument, e.g. `Box[Node]`");
                return Ty::Error;
            };
            // `Box[Named]`: a boxed value of any type that implements the interface.
            // `Box[Named + Send]`: one that may also be moved to (or shared with) other threads.
            if inner.name == "+" {
                let base = &inner.args[0];
                let ii = if base.args.is_empty() { self.iface_index.get(&base.name).copied() } else { None };
                let Some(ii) = ii else {
                    self.diags.error("E0001", base.span, "capabilities such as `+ Send` can only follow an interface name");
                    return Ty::Error;
                };
                let (mut send, mut sync) = (false, false);
                for c in &inner.args[1..] {
                    match c.name.as_str() {
                        "Send" => send = true,
                        "Sync" => sync = true,
                        other => {
                            self.diags.error("E0001", c.span, format!("unknown capability `{other}`; the capabilities are `Send` and `Sync`"));
                            return Ty::Error;
                        }
                    }
                }
                return Ty::Dyn(dyn_with_caps(ii as u32, send, sync));
            }
            if inner.args.is_empty() {
                if let Some(&ii) = self.iface_index.get(&inner.name) {
                    return Ty::Dyn(ii as u32);
                }
            }
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
                self.mark_type_used(&t.name, t.span);
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
        if let Some(ft) = FloatTy::from_name(&t.name) {
            return Ty::Float(ft);
        }
        if let Some(&i) = self.struct_index.get(&t.name) {
            return self.structs[i].ty;
        }
        if self.iface_index.contains_key(&t.name) {
            self.diags
                .error("E0001", t.span, format!("`{}` is an interface, not a type of its own", t.name))
                .note(format!("a value of some type that implements it is written `Box[{}]`", t.name));
            return Ty::Error;
        }
        match t.name.as_str() {
            "bool" => Ty::Bool,
            "str" => Ty::Str,
            "String" => Ty::String,
            name if UNSUPPORTED_TYPES.contains(&name) => {
                self.mark_type_used(name, t.span);
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
        let ret = self.sig(idx).ret;
        let param_tys = self.sig(idx).params.clone();
        let sinks = self.sig(idx).sinks.clone();
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
        if ret != Ty::Unit && ret != Ty::Error && !unit_res && !terminates(&f.body.stmts, &cx) {
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
    fn lift_closure(&mut self, e: &Expr, param_tys: &[Ty], ret: Ty, what: &str, saved: bool, cx: &mut FnCx) -> Option<(usize, Vec<HExpr>)> {
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
        let names = crate::closure_free::free_variables(params, body);
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
            cx.locals[id].used = true;
            self.check_moved(cx, id, e.span);
            if saved && (ty.carries_borrow() || matches!(ty, Ty::Sync(s) if matches!(sync_parts(s).0, SyncKind::Join | SyncKind::Scope))) {
                self.diags.error("E0307", e.span, format!("the stored `{what}` closure cannot capture `{name}`: `{ty}` carries a borrow"))
                    .note("capture owned data instead; convert views to owned values before constructing the closure");
                return None;
            }
            if ty.contains_ref_mut() {
                self.diags.error("U0001", e.span, format!("a closure cannot use the `&inout` value `{name}` yet"));
                return None;
            }
            let pid = self.declare(&mut ccx, &name, e.span, ty, LocalKind::Param);
            ccx.locals[pid].used = true;
            ccx.captures.push(pid);
            if ty.carries_borrow() || !ty.is_copy() {
                ccx.locals[pid].loans = vec![Loan::Local(pid)];
            }
            ids.push(pid);
            let cap = HExpr { kind: HExprKind::Local(id), ty, span: e.span, at: e.span };
            cap_exprs.push(if saved { self.consume(cap, cx) } else { cap });
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
                if ret != Ty::Unit && !terminates(&b.stmts, &ccx) {
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
        // Keeps signature numbers equal to function indexes (generic instances are numbered
        // among the lifted functions too).
        let cparams: Vec<Ty> = ids.iter().map(|&i| ccx.locals[i].ty).collect();
        self.sigs.push(Sig { params: cparams, sinks: vec![false; n], inouts: vec![false; n], ret, ret_of: None, has_self: false });
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
        let Some((func, caps)) = self.lift_closure(arg, &[elem, elem], Ty::Bool, "sortBy", false, cx) else { return err_expr(span) };
        if caps.iter().any(|c| matches!(c.kind, HExprKind::Local(id) if id == place.local)) {
            let n = cx.locals[place.local].name.clone();
            self.diags
                .error("E0102", arg.span, format!("`{n}` is being sorted, so the closure cannot use it"))
                .note("copy what the closure needs into a variable first");
            return err_expr(span);
        }
        let tramp = self.tramps.len();
        self.tramps.push(Tramp { func, caps: caps.iter().map(|c| c.ty).collect(), elem, by_cmp: false, saved: false });
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
        cx.call_reserved.push((id, format!("{local}.update(...)")));
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
        let Some((func, caps)) = self.lift_closure(fa, &[v], v, "update", false, cx) else { return err_expr(span) };
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
            guard: false,
            pinned: false,
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
        if let (Ty::Box(b), Ty::Dyn(i)) = (e.ty, expected) {
            let (inner, iname) = (box_inner(b), &self.ifaces[dyn_base(i) as usize].name);
            if self.declared_impls.contains(&(inner, dyn_base(i) as usize)) {
                d.note(format!("`{inner}` declares `{iname}`, but its methods do not match the interface (reported above)"));
            } else {
                d.note(format!("`{inner}` does not implement `{iname}`; declare it as `struct {inner}: {iname}` and write its methods"));
            }
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
        // A boxed value used where an interface value is expected (section 8 whitelist).
        if let Ty::Dyn(i) = expected {
            return self.to_dyn(h, i, cx);
        }
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
        let h = if let Ty::Dyn(i) = inner { self.to_dyn(h, i, cx) } else { h };
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
        if cx.call_shared.contains(&id) || cx.call_mut.contains(&id)
            || cx.locals[id].loans.iter().any(|l| matches!(l, Loan::Local(s) if cx.call_shared.contains(s) || cx.call_mut.contains(s)))
        {
            self.diags.error("E0102", span, format!("`{name}` is already borrowed by an earlier argument of this call"));
            return err_expr(span);
        }
        // An `&inout T` (a writable binding, `getMut`'s result) passes its target on: `&n`.
        if let Ty::RefMut(r) = cx.locals[id].ty {
            let target = ref_mut_inner(r);
            if exp.is_none_or(|t| t == target) {
                if !self.ref_mut_writable(cx, id, span) { return err_expr(span); }
                if borrowed.contains(&id) || mut_borrowed.contains(&id) {
                    self.diags
                        .error("E0102", span, format!("`{name}` is already borrowed by an earlier argument of this call"))
                        .note("a variable passed as `inout` cannot be used by any other argument of the same call");
                }
                self.check_not_borrowed(cx, id, inner.span, "borrow mutably");
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

    /// `with g = m.lock() { body }`: `g` lives until the end of `body`, where the lock is
    /// released; borrows through it cannot leave the block (P4, `E0703`).
    fn with_stmt(&mut self, name: &str, name_span: Span, init: &Expr, body: &Block, span: Span, cx: &mut FnCx) -> Option<HStmt> {
        let stmt_idx = cx.cur;
        let h = self.expr(init, None, cx);
        let is_guard = matches!(h.ty, Ty::Sync(id) if sync_parts(id).0 == SyncKind::Guard);
        if !is_guard && h.ty != Ty::Error {
            self.diags
                .error("E0001", h.span, format!("`with` binds a lock guard, but this value has type `{}`", h.ty))
                .note("write `with g = m.lock() { ... }`");
        }
        let h = self.consume(h, cx);
        let loans = if h.ty.carries_borrow() { self.loans_of(&h, cx) } else { Vec::new() };
        cx.scopes.push(Vec::new());
        let local = self.declare(cx, name, name_span, h.ty, LocalKind::Let);
        if let Some(&end) = cx.pre.bind_end.get(&(stmt_idx, 0)) {
            cx.locals[local].scope_end = end;
        }
        cx.locals[local].guard = true;
        if h.ty.carries_borrow() {
            self.store_loans(cx, local, loans, h.span, true);
        }
        let inner = self.block(body, cx);
        self.pop_scope(cx);
        let stmts = vec![HStmt { kind: HStmtKind::Let { local, init: h }, span }, HStmt { kind: HStmtKind::Block(inner), span }];
        Some(HStmt { kind: HStmtKind::Block(stmts), span })
    }

    /// `scope { body }`: a hidden local whose drop waits for the scope's threads is declared
    /// first, so it is dropped last; borrows taken by `spawn` are stored in it (P3).
    fn scope_stmt(&mut self, body: &Block, span: Span, cx: &mut FnCx) -> Option<HStmt> {
        self.scope_stmt_inner(body, span, cx)
    }

    /// `Channel[T]{cap: n}` (P5, stdlib contract §6): elements must be `Send` (`E0701`); a
    /// capacity that is not positive panics with `P-CHANNEL-CAP`.
    fn chan_new(&mut self, ty: Ty, inits: &[FieldInit], name_span: Span, span: Span, cx: &mut FnCx) -> HExpr {
        let Ty::Sync(id) = ty else { unreachable!("a channel type") };
        let elem = sync_parts(id).1;
        let int = Ty::Int(IntTy::I64);
        let mut cap = None;
        for init in inits {
            if init.name == "cap" && cap.is_none() {
                let h = self.expr(&init.value, Some(int), cx);
                cap = Some(self.fit(h, int, cx));
            } else {
                self.diags.error("E0001", init.value.span, format!("a channel has one field, `cap`; `{}` is not allowed here", init.name));
                self.expr(&init.value, None, cx);
            }
        }
        let Some(cap) = cap else {
            self.diags.error("E0001", name_span, "a channel needs its capacity: `Channel[T]{cap: n}`");
            return err_expr(span);
        };
        if self.heap_thread_gap(elem, name_span) {
            return err_expr(span);
        }
        if !self.cap_ok(elem, false, &mut Vec::new()) {
            let hint = self.send_hint(elem);
            self.diags.error("E0701", name_span, format!("a channel cannot carry `{elem}`: it is not `Send`")).note(hint);
            return err_expr(span);
        }
        HExpr { kind: HExprKind::ChanNew(Box::new(cap)), ty, span, at: name_span }
    }

    /// `SendError[T]` (stdlib contract §6): `enum { Closed(value: T) }`, made on first use.
    fn send_error_ty(&mut self, t: Ty) -> Ty {
        let name = format!("SendError[{t}]");
        if let Some(&i) = self.struct_index.get(&name) {
            return self.structs[i].ty;
        }
        let ty = Ty::Enum(register_enum(&name));
        let span = Span { start: 0, end: 0, line: 1, col: 1 };
        let si = self.structs.len();
        self.struct_index.insert(name.clone(), si);
        let variants = vec![VariantDef { name: "Closed".to_string(), fields: vec![("value".to_string(), t, span)] }];
        self.structs.push(StructDef { name, span, ty, fields: Vec::new(), variants, defaults: Vec::new() });
        let Ty::Enum(id) = ty else { unreachable!() };
        finish_agg(id, vec![vec![t]]);
        self.late_structs.push(si);
        ty
    }

    fn scope_stmt_inner(&mut self, body: &Block, span: Span, cx: &mut FnCx) -> Option<HStmt> {
        let stmt_idx = cx.cur;
        cx.scopes.push(Vec::new());
        let ty = Ty::Sync(sync_of(SyncKind::Scope, Ty::Unit));
        let local = self.declare(cx, "_", span, ty, LocalKind::Let);
        if let Some(&end) = cx.pre.bind_end.get(&(stmt_idx, 0)) {
            cx.locals[local].scope_end = end;
        }
        cx.locals[local].pinned = true;
        cx.locals[local].used = true;
        cx.thread_scopes.push(local);
        let mut stmts = vec![HStmt { kind: HStmtKind::Let { local, init: HExpr { kind: HExprKind::ScopeNew, ty, span, at: span } }, span }];
        stmts.extend(self.stmts(&body.stmts, cx));
        cx.thread_scopes.pop();
        self.pop_scope(cx);
        Some(HStmt { kind: HStmtKind::Block(stmts), span })
    }

    /// `spawn f(args)` (P3, P7, stdlib contract §6): moved arguments must be `Send` (`E0701`),
    /// borrowed ones `Sync` (`E0702`); borrowed data must exist before the scope (`E0705`); the
    /// result must be `Send` and own its data.
    fn spawn_expr(&mut self, call: &Expr, span: Span, cx: &mut FnCx) -> HExpr {
        let Some(&scope) = cx.thread_scopes.last() else {
            self.diags
                .error("E0001", span, "`spawn` can only be used inside a `scope { ... }` block")
                .note("the scope waits for the thread, so data the thread borrows stays alive");
            self.expr(call, None, cx);
            return err_expr(span);
        };
        let ExprKind::Call { callee, .. } = &call.kind else {
            self.diags.error("U0001", call.span, "`spawn` needs a call of a named function, e.g. `spawn work(x)`");
            self.expr(call, None, cx);
            return err_expr(span);
        };
        if !matches!(callee.kind, ExprKind::Ident(_)) {
            self.diags.error("U0001", call.span, "`spawn` of a method call is not supported yet; call a function");
            self.expr(call, None, cx);
            return err_expr(span);
        }
        let h = self.expr(call, None, cx);
        let HExprKind::Call { func, args } = h.kind else {
            if h.ty != Ty::Error {
                self.diags.error("U0001", call.span, "`spawn` can only run a function defined in this program");
            }
            return err_expr(span);
        };
        let (params, sinks, inouts) = {
            let s = self.sig(func);
            (s.params.clone(), s.sinks.clone(), s.inouts.clone())
        };
        let mut loans = Vec::new();
        let mut ok = true;
        for (i, a) in args.iter().enumerate() {
            let pty = params.get(i).copied().unwrap_or(a.ty);
            if inouts.get(i).copied().unwrap_or(false) || a.ty.contains_ref_mut() {
                // Generic arguments and tuples must not bypass the same restriction.
                // Scope loans currently pin shared access; exclusive thread loans need
                // their own access state before `inout` spawn can be implemented.
                self.diags.error("U0001", a.span, "passing a mutable borrow to a thread is not supported yet");
                ok = false;
                continue;
            }
            let sink = sinks.get(i).copied().unwrap_or(false);
            if self.heap_thread_gap(if sink { a.ty } else { pty }, a.span) {
                ok = false;
                continue;
            }
            if sink {
                if !self.cap_ok(a.ty, false, &mut Vec::new()) {
                    let hint = self.send_hint(a.ty);
                    self.diags
                        .error("E0701", a.span, format!("`{}` cannot be moved to another thread: it is not `Send`", a.ty))
                        .note(hint);
                    ok = false;
                    continue;
                }
            } else if !self.cap_ok(pty, true, &mut Vec::new()) {
                // A borrowed argument: the thread reads the caller's data, which must be `Sync`.
                self.diags
                    .error("E0702", a.span, format!("`{pty}` cannot be shared with another thread: it is not `Sync`"))
                    .note("move it to the thread with a `sink` parameter instead, or use a type that is `Sync` (e.g. `Box[Named + Send + Sync]`)");
                ok = false;
                continue;
            }
            // Moving an owning container transfers its own storage, not the data its
            // elements borrow. Keep those sources alive just like copied view descriptors.
            // Do not borrow the moved container's former slot: the worker now owns it.
            let mut ls = if sink || a.ty.is_copy() { self.loans_of(a, cx) } else { self.storage_loans(a, cx) };
            if a.ty.is_copy() && !a.ty.carries_borrow() {
                ls.clear();
            }
            for l in ls {
                match l {
                    Loan::Temp => {
                        self.diags
                            .error("E0705", a.span, "a thread cannot borrow a temporary: it is dropped at the end of this statement")
                            .note("store the value in a variable declared before the `scope`, then pass it");
                        ok = false;
                    }
                    Loan::Local(s) if cx.locals[s].decl > cx.locals[scope].decl => {
                        let sn = cx.locals[s].name.clone();
                        self.diags
                            .error("E0705", a.span, format!("a thread cannot borrow `{sn}`: it is declared inside the `scope` and dropped before the scope waits for its threads"))
                            .note(format!("declare `{sn}` before the `scope` block"));
                        ok = false;
                    }
                    l => {
                        if !loans.contains(&l) {
                            loans.push(l);
                        }
                    }
                }
            }
        }
        let ret = h.ty;
        if self.heap_thread_gap(ret, call.span) {
            ok = false;
        } else if ret != Ty::Error && (!self.cap_ok(ret, false, &mut Vec::new()) || ret.carries_borrow()) {
            self.diags
                .error("E0701", call.span, format!("a thread cannot return `{ret}`: the result must be `Send` and own its data"))
                .note("return owned values, e.g. `String` instead of `str`");
            ok = false;
        }
        if !ok {
            return err_expr(span);
        }
        self.store_loans(cx, scope, loans, span, false);
        let ty = Ty::Sync(sync_of(SyncKind::Join, ret));
        HExpr { kind: HExprKind::Spawn { scope, func, args }, ty, span, at: span }
    }

    /// Thread capability propagation for an erased Heap environment needs a public
    /// capability contract. Keep this implementation boundary distinct from a type that
    /// the language permanently declares !Send / !Sync, including nested containers.
    fn heap_thread_gap(&mut self, ty: Ty, span: Span) -> bool {
        fn contains(ck: &Checker<'_>, ty: Ty, seen: &mut Vec<Ty>) -> bool {
            if seen.contains(&ty) { return false; }
            seen.push(ty);
            match ty {
                Ty::Heap(_) => true,
                Ty::Vec(id) | Ty::Deque(id) | Ty::Arena(id) => contains(ck, vec_elem(id), seen),
                Ty::Slice(id) => contains(ck, slice_elem(id), seen),
                Ty::Ref(id) => contains(ck, ref_inner(id), seen),
                Ty::RefMut(id) => contains(ck, ref_mut_inner(id), seen),
                Ty::Opt(id) => contains(ck, opt_inner(id), seen),
                Ty::Box(id) => contains(ck, box_inner(id), seen),
                Ty::Sync(id) => contains(ck, sync_parts(id).1, seen),
                Ty::Map(id) | Ty::Set(id) => {
                    let (a, b) = map_parts(id);
                    contains(ck, a, seen) || contains(ck, b, seen)
                }
                Ty::Res(id) => {
                    let (a, b) = res_parts(id);
                    contains(ck, a, seen) || contains(ck, b, seen)
                }
                Ty::Tuple(id) => tuple_elems(id).iter().any(|t| contains(ck, *t, seen)),
                Ty::Struct(_) | Ty::Enum(_) => ck.structs.iter().find(|s| s.ty == ty).is_some_and(|s| {
                    s.fields.iter().map(|f| f.1).chain(s.variants.iter().flat_map(|v| v.fields.iter().map(|f| f.1)))
                        .any(|t| contains(ck, t, seen))
                }),
                _ => false,
            }
        }
        if !contains(self, ty, &mut Vec::new()) { return false; }
        self.diags.error("U0001", span, "thread capabilities of saved Heap comparators are not supported yet")
            .note("the comparator owns hidden captures; checking only the heap element type would be unsafe");
        true
    }

    /// A hint for a value that is not `Send`.
    fn send_hint(&self, t: Ty) -> String {
        match t {
            Ty::Dyn(_) => "an interface value crosses threads only if declared so: `Box[Named + Send]`".to_string(),
            Ty::Sync(id) if sync_parts(id).0 == SyncKind::Guard => {
                "a lock guard must be released on the thread that took the lock; pass the `Mutex` (e.g. in an `Arc`) instead".to_string()
            }
            _ => "`Rc` never crosses threads (use `Arc`), and `Arc[Mutex[T]]` needs `T: Send`".to_string(),
        }
    }

    /// Whether values of `ty` can be moved to another thread (`Send`, `sync == false`) or shared
    /// between threads (`Sync`, `sync == true`), derived from the type's structure (P6).
    fn cap_ok(&self, ty: Ty, sync: bool, seen: &mut Vec<(Ty, bool)>) -> bool {
        match ty {
            Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Unit | Ty::Str | Ty::String | Ty::Id(_) | Ty::Error => true,
            Ty::Vec(id) | Ty::Deque(id) | Ty::Arena(id) => self.cap_ok(vec_elem(id), sync, seen),
            // Heap's erased capture environment has no public capability parameter yet.
            // Never derive a capability from the element type alone.
            Ty::Heap(_) => false,
            // Views and read-only references share what they point to.
            Ty::Slice(id) => self.cap_ok(slice_elem(id), true, seen),
            Ty::Ref(id) => self.cap_ok(ref_inner(id), true, seen),
            Ty::RefMut(id) => self.cap_ok(ref_mut_inner(id), sync, seen),
            Ty::Opt(id) => self.cap_ok(opt_inner(id), sync, seen),
            Ty::Box(id) => self.cap_ok(box_inner(id), sync, seen),
            Ty::Map(id) | Ty::Set(id) => {
                let (k, v) = map_parts(id);
                self.cap_ok(k, sync, seen) && self.cap_ok(v, sync, seen)
            }
            Ty::Res(id) => {
                let (o, e) = res_parts(id);
                self.cap_ok(o, sync, seen) && self.cap_ok(e, sync, seen)
            }
            Ty::Tuple(id) => tuple_elems(id).iter().all(|t| self.cap_ok(*t, sync, seen)),
            Ty::Struct(_) | Ty::Enum(_) => {
                if seen.contains(&(ty, sync)) {
                    return true;
                }
                // A recursive Send proof cannot stand in for a separate Sync proof.
                seen.push((ty, sync));
                let Some(s) = self.structs.iter().find(|s| s.ty == ty) else { return true };
                let mut tys: Vec<Ty> = s.fields.iter().map(|f| f.1).collect();
                tys.extend(s.variants.iter().flat_map(|v| v.fields.iter().map(|f| f.1)));
                tys.into_iter().all(|t| self.cap_ok(t, sync, seen))
            }
            Ty::Dyn(id) => {
                let (send, syn) = dyn_caps(id);
                if sync { syn } else { send }
            }
            Ty::Sync(id) => match sync_parts(id) {
                (SyncKind::Rc | SyncKind::Guard | SyncKind::Join | SyncKind::Scope, _) => false,
                (SyncKind::Arc, t) => self.cap_ok(t, false, seen) && self.cap_ok(t, true, seen),
                (SyncKind::Mutex | SyncKind::Channel, t) => self.cap_ok(t, false, seen),
            },
        }
    }

    /// `Mutex`, `Guard` and `Channel` come from `std/sync` (stdlib contract §1).
    fn need_sync(&mut self, name: &str, span: Span) -> bool {
        let key = (crate::project::file_id(&self.prog.files, span), "sync".to_string());
        match self.imports.get_mut(&key) {
            Some((_, used)) => {
                *used = true;
                true
            }
            None => {
                self.diags.error("E0010", span, format!("`{name}` comes from `std/sync`, which is not imported")).note("add `use std/sync`");
                false
            }
        }
    }

    /// `Rc(v)`, `Arc(v)`, `Mutex(v)`: moves `v` into a new shared block (stdlib contract §6).
    fn sync_new(&mut self, kind: SyncKind, args: &[Expr], expected: Option<Ty>, span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let kname = kind.name();
        if kind == SyncKind::Mutex && !self.need_sync(kname, at) {
            for a in args {
                self.expr(a, None, cx);
            }
            return err_expr(span);
        }
        let [arg] = args else {
            self.diags.error("E0012", span, format!("`{kname}` takes exactly one argument, the value to share"));
            for a in args {
                self.expr(a, None, cx);
            }
            return err_expr(span);
        };
        let want = match expected {
            Some(Ty::Sync(id)) if sync_parts(id).0 == kind => Some(sync_parts(id).1),
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
                self.diags.error("E0001", h.span, format!("`()` cannot be put in a `{kname}`"));
                return err_expr(span);
            }
            t if t.carries_borrow() => {
                self.diags
                    .error("E0301", h.span, format!("a `{kname}` cannot hold a `{t}`: a view cannot be stored in it"))
                    .note("store owned values instead, e.g. `String`");
                return err_expr(span);
            }
            _ => {}
        }
        let h = self.consume(h, cx);
        let ty = Ty::Sync(sync_of(kind, h.ty));
        HExpr { kind: HExprKind::SyncNew { kind, value: Box::new(h) }, ty, span, at }
    }

    /// `&place` where an `&inout T` is expected (A7): the place must be writable, and nothing
    /// else may use it while the new reference is alive (A3). `&p` for an `&inout T` variable
    /// `p` reborrows its target (A4).
    fn borrow_mut(&mut self, inner: &Expr, r: u32, span: Span, cx: &mut FnCx) -> HExpr {
        let mut h = self.mutable_address(inner, cx);
        self.expect_ty(&h, Ty::RefMut(r));
        h.span = span;
        h
    }

    /// A checked address to a writable position, including indexed/nested positions.
    /// Building it never consumes the owner; every result retains its storage loans.
    fn mutable_address(&mut self, inner: &Expr, cx: &mut FnCx) -> HExpr {
        let span = inner.span;
        match &inner.kind {
            ExprKind::Paren(e) => self.mutable_address(e, cx),
            ExprKind::Ident(name) => {
                let before = self.diags.count(crate::diag::Severity::Error);
                let Some(place) = self.mutable_place(inner, "borrow mutably", cx) else { return err_expr(span) };
                let id = place.local;
                if matches!(cx.locals[id].ty, Ty::RefMut(_)) && cx.locals[id].kind == LocalKind::Param {
                    self.diags.error("E0101", span, format!("cannot modify through `{name}`: a default parameter is read-only"));
                    return err_expr(span);
                }
                if self.diags.count(crate::diag::Severity::Error) == before {
                    self.check_exclusive(cx, id, span);
                }
                let ty = Ty::RefMut(ref_mut_of(place.ty));
                HExpr { kind: HExprKind::InoutPlace(place), ty, span, at: span }
            }
            ExprKind::Field { base, name } => {
                let pointer = self.mutable_address(base, cx);
                if pointer.ty == Ty::Error { return pointer; }
                let b = self.deref(pointer);
                let b = self.mutable_projection(b);
                if b.ty == Ty::Error { return b; }
                let Some((field, ty)) = self.struct_field(b.ty, name) else {
                    self.diags.error("E0001", span, format!("a value of type `{}` has no field `{name}`", b.ty));
                    return err_expr(span);
                };
                let field = HExpr { kind: HExprKind::Field { base: Box::new(b), field }, ty, span, at: span };
                HExpr { kind: HExprKind::AddrOf(Box::new(field)), ty: Ty::RefMut(ref_mut_of(ty)), span, at: span }
            }
            ExprKind::Index { base, index } => {
                let pointer = self.mutable_address(base, cx);
                if pointer.ty == Ty::Error { return pointer; }
                let loans = self.loans_of(&pointer, cx);
                let b = self.deref(pointer);
                let b = self.mutable_projection(b);
                if b.ty == Ty::Error { return b; }
                let Ty::Vec(v) = b.ty else {
                    self.diags.error("E0101", span, format!("mutable element borrowing requires a Vec, found `{}`", b.ty));
                    return err_expr(span);
                };
                // The index runs before the element is borrowed (V2): it may read the vector
                // (`v[v.len() - 1] = x`), but not modify it.
                let mark = cx.call_reserved.len();
                for l in &loans {
                    if let Loan::Local(id) = *l {
                        let call = format!("{}[...] = ...", cx.locals[id].name);
                        cx.call_reserved.push((id, call));
                    }
                }
                let i = self.expr(index, None, cx);
                cx.call_reserved.truncate(mark);
                if !matches!(i.ty, Ty::Int(_) | Ty::Error) {
                    self.diags.error("E0001", i.span, format!("an index must be an integer, found `{}`", i.ty));
                }
                HExpr { kind: HExprKind::Index { base: Box::new(b), index: Box::new(i) },
                    ty: Ty::RefMut(ref_mut_of(vec_elem(v))), span, at: span }
            }
            ExprKind::Unary { op: UnOp::Deref, expr, .. } => {
                let h = self.expr(expr, None, cx);
                if let HExprKind::Local(id) = h.kind {
                    if !self.ref_mut_writable(cx, id, span) { return err_expr(span); }
                }
                if matches!(h.ty, Ty::RefMut(_)) { h } else {
                    if h.ty != Ty::Error {
                        self.diags.error("E0101", span, "a mutable reborrow requires an `&inout` reference");
                    }
                    err_expr(span)
                }
            }
            _ => {
                let h = self.expr(inner, None, cx);
                if matches!(h.ty, Ty::RefMut(_)) { h } else {
                    if h.ty != Ty::Error {
                        self.diags.error("E0014", span, "cannot mutably borrow an owned temporary; bind it with `var` first");
                    }
                    err_expr(span)
                }
            }
        }
    }

    fn mutable_projection(&mut self, mut h: HExpr) -> HExpr {
        while let Some(ty) = ptr_inner(h.ty) {
            if ptr_read_only(h.ty) {
                self.read_only_ptr(h.ty, h.span);
                return err_expr(h.span);
            }
            let (span, at) = (h.span, h.at);
            h = HExpr { kind: HExprKind::Unbox(Box::new(h)), ty, span, at };
        }
        h
    }

    /// A mutable receiver is acquired before its arguments (V2). Check aliases,
    /// including all sources preserved through a returned/reborrowed descriptor.
    fn borrowed_argument_conflict(&mut self, args: &[Expr], loans: &[Loan], cx: &FnCx, operation: &str) -> bool {
        for arg in args {
            let mut names = Vec::new();
            idents_expr(arg, &mut names);
            for (name, _) in names {
                let Some(id) = self.lookup(cx, &name) else { continue };
                if loans.contains(&Loan::Local(id)) || cx.locals[id].loans.iter().any(|l| loans.contains(l)) {
                    self.diags.error("E0403", arg.span,
                        format!("`{name}` is already borrowed by the receiver of `{operation}`, so the argument cannot use it"))
                        .note("evaluate the argument in a separate statement before acquiring the mutable receiver");
                    return true;
                }
            }
        }
        false
    }

    /// Runs a modifying built-in call; the receiver it reserves (V2) is released afterwards.
    fn with_reservation(&mut self, cx: &mut FnCx, f: impl FnOnce(&mut Self, &mut FnCx) -> HExpr) -> HExpr {
        let mark = cx.call_reserved.len();
        let h = f(self, cx);
        cx.call_reserved.truncate(mark);
        h
    }

    /// V2 (two-phase): the value of an argument of a call that modifies `recv` must not keep
    /// pointing into `recv` once the call starts. `by_ref`: the argument stays borrowed during
    /// the call (a read-only parameter) instead of being moved or copied in.
    fn reserved_conflict(&mut self, h: &HExpr, recv: LocalId, by_ref: bool, call: &str, cx: &FnCx) -> bool {
        let mut loans = if h.ty.carries_borrow() { self.loans_of(h, cx) } else { Vec::new() };
        if by_ref && !h.ty.is_copy() {
            loans.extend(self.storage_loans(h, cx));
        }
        let hit = loans.iter().any(|l| match *l {
            Loan::Local(s) => s == recv || cx.locals[s].loans.contains(&Loan::Local(recv)),
            Loan::Temp => false,
        });
        if hit {
            let name = cx.locals[recv].name.clone();
            self.diags
                .error("E0403", h.span, format!("this argument still points into `{name}`, which `{call}` is about to modify"))
                .note("pass an owned copy (`.clone()`) or a copyable value read from it, computed before the call");
        }
        hit
    }

    /// The declared type of a local variable. Unlike other positions, a local may be an
    /// `&inout T` (A1, A7).
    fn local_ty(&mut self, t: &TypeExpr) -> Ty {
        if t.name == "&inout" {
            return match t.args.first().map(|a| self.resolve_ty(a)) {
                Some(Ty::Error) | None => Ty::Error,
                Some(inner) => Ty::RefMut(ref_mut_of(inner)),
            };
        }
        self.resolve_ty(t)
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
        let ret = h.ty;
        let recv_loans = |r: &HExpr| -> Vec<Loan> {
            match (&r.kind, r.ty) {
                (HExprKind::Local(id), Ty::String) => vec![Loan::Local(*id)],
                (HExprKind::InoutArg(id), _) => {
                    let mut out = vec![Loan::Local(*id)];
                    out.extend(cx.locals[*id].loans.iter().copied());
                    out
                }
                (HExprKind::InoutPlace(_), _) => self.loans_of(r, cx),
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
                // Other owning containers (`Vec[String]`, `Map`, `Box`, ...): a returned view may
                // point into their storage. Only a vector of Copy elements cannot hold text that a
                // `str` could view, so a result whose borrows are all `str` keeps just its elements'
                // loans (`fn first(ws: Vec[str]) -> str of ws`).
                (kind, ty) if !ty.is_copy() && ty.needs_drop() => {
                    let text_only = matches!(ty, Ty::Vec(id) if vec_elem(id).is_copy()) && only_text_borrows(ret);
                    if text_only {
                        self.loans_of(r, cx)
                    } else if matches!(kind, HExprKind::Local(_) | HExprKind::Field { .. } | HExprKind::Deref(_) | HExprKind::Unbox(_) | HExprKind::Index { .. }) {
                        self.storage_loans(r, cx)
                    } else {
                        let mut out = vec![Loan::Temp];
                        out.extend(self.loans_of(r, cx));
                        out
                    }
                }
                _ => self.loans_of(r, cx),
            }
        };
        match &h.kind {
            HExprKind::Local(id) | HExprKind::Move(id) => cx.locals[*id].loans.clone(),
            HExprKind::LetTemp { value, .. } => self.loans_of(value, cx),
            // A guard is `of` its mutex (stdlib contract §6).
            HExprKind::Lock { mutex, .. } => self.storage_loans(mutex, cx),
            // `&x` / `&x.f` (and a reborrow `&p`): points into the root variable and whatever it
            // already points into.
            HExprKind::InoutPlace(p) => {
                let mut out = if cx.locals[p.local].name.starts_with("$receiver") { Vec::new() } else { vec![Loan::Local(p.local)] };
                out.extend(cx.locals[p.local].loans.iter().copied());
                out
            }
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
            HExprKind::Pop(p) | HExprKind::DequePop { place: p, .. } => cx.locals[p.local].loans.clone(),
            HExprKind::Take(p) | HExprKind::Replace(p, _) => cx.locals[p.local].loans.clone(),
            // `get` borrows the map's slot; `getCopy` copies a value that keeps the elements' loans.
            HExprKind::MapRead { op: MapRead::Get, recv, .. } => self.storage_loans(recv, cx),
            HExprKind::ArenaRead { op: MapRead::Get, recv, .. } => self.storage_loans(recv, cx),
            HExprKind::ArenaRead { recv, .. } => self.loans_of(recv, cx),
            HExprKind::MapRead { recv, .. } => self.loans_of(recv, cx),
            HExprKind::MapWrite { op: MapWrite::GetMut, place, .. }
            | HExprKind::ArenaWrite { op: ArenaWrite::GetMut, place, .. } => {
                // A temporary receiver is only a pointer descriptor. The slot
                // returned by getMut belongs to its actual Map/Guard sources.
                let mut out = if cx.locals[place.local].name.starts_with("$receiver") { Vec::new() } else { vec![Loan::Local(place.local)] };
                out.extend(cx.locals[place.local].loans.iter().copied());
                out
            }
            HExprKind::MapWrite { place, .. } => cx.locals[place.local].loans.clone(),
            HExprKind::ArenaWrite { place, .. } => cx.locals[place.local].loans.clone(),
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
            HExprKind::Index { base, .. } if matches!(h.ty, Ty::Ref(_) | Ty::RefMut(_)) => self.storage_loans(base, cx),
            // A view of a view points where it points; a view of owned data borrows its storage.
            HExprKind::SliceRange { base, .. } => match (base.ty, &base.kind) {
                (Ty::Str | Ty::Slice(_), _) => self.loans_of(base, cx),
                (_, HExprKind::Local(_) | HExprKind::Field { .. } | HExprKind::Deref(_) | HExprKind::Unbox(_) | HExprKind::Index { .. }) => {
                    self.storage_loans(base, cx)
                }
                _ => {
                    let mut out = vec![Loan::Temp];
                    out.extend(self.loans_of(base, cx));
                    out
                }
            },
            HExprKind::StrView(inner) => recv_loans(inner),
            HExprKind::Method { method: Builtin::Trim | Builtin::Fields | Builtin::Lines, recv } => recv_loans(recv),
            // A cloned `Vec[str]` owns new storage, but its elements still point into the same text.
            HExprKind::Method { method: Builtin::VecClone | Builtin::SliceToVec, recv } if h.ty.carries_borrow() => {
                self.loans_of(recv, cx)
            }
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
            HExprKind::Call { func: s, args } | HExprKind::DynCall { sig: s, args, .. } if h.ty.carries_borrow() => {
                let picked: Vec<usize> = match &self.sig(*s).ret_of {
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
                    if cx.locals[s].guard {
                        self.diags
                            .error("E0703", span, format!("`{vn}` would keep a borrow taken through the lock guard `{sn}` after the `with` block ends"))
                            .note("outside the block the data is no longer protected by the lock; use the value inside the block, or `.clone()` an owned copy there");
                    } else {
                        self.diags
                            .error("E0305", span, format!("`{sn}` does not live long enough: `{vn}` would still point into it after it is dropped"))
                            .note(format!("declare `{sn}` before `{vn}`, in the same or an enclosing block"));
                    }
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
        // An assignment value is already materialized while the target runs.
        // Changing an owning Vec[str] descriptor does not change the strings it
        // points into; only writable aliases carry this conflict transitively.
        let alias = matches!(cx.locals[s].ty, Ty::Ref(_) | Ty::RefMut(_))
            || cx.locals[s].ty.contains_ref_mut()
            || matches!(cx.locals[s].ty, Ty::Sync(id) if sync_parts(id).0 == SyncKind::Guard);
        if cx.assignment_shared.contains(&s) || cx.assignment_mut.contains(&s)
            || alias && cx.locals[s].loans.iter().any(|l| matches!(l, Loan::Local(id)
                if cx.assignment_shared.contains(id) || cx.assignment_mut.contains(id)))
        {
            self.diags.error("E0102", span, format!("cannot {what} `{}`: the pending assignment value still borrows it", cx.locals[s].name));
            return;
        }
        if let Some((_, call)) = cx.call_reserved.iter().find(|(r, _)| *r == s) {
            let name = cx.locals[s].name.clone();
            self.diags
                .error("E0403", span, format!("cannot {what} `{name}` in an argument of `{call}`: the call is about to modify it"))
                .note(format!("an argument may read `{name}`, but not modify or move it; do that in a separate statement first"));
            return;
        }
        if cx.call_shared.contains(&s) || cx.call_mut.contains(&s)
            || cx.locals[s].loans.iter().any(|l| matches!(l, Loan::Local(id) if cx.call_shared.contains(id) || cx.call_mut.contains(id)))
        {
            self.diags.error("E0102", span, format!("cannot {what} `{}`: an earlier call argument still borrows it", cx.locals[s].name));
            return;
        }
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
                let d = self.diags
                    .error("E0102", span, format!("cannot {what} `{sn}` while `{vn}` still points into it"))
                    .note(format!("`{vn}` is used again on line {line}"));
                if !vn.starts_with('$') && matches!(lv.ty, Ty::Ref(_)) {
                    d.note(format!(
                        "`{vn}` is a `{}` borrow: reading a non-copyable element (`let {vn} = {sn}[i]`) borrows it; if `{vn}` should not depend on `{sn}`, copy it with `.clone()` where it is created",
                        lv.ty
                    ));
                }
                return;
            }
            // Threads of an enclosing `scope` keep their borrows until the scope ends (P3).
            if lv.pinned {
                let sn = cx.locals[s].name.clone();
                self.diags
                    .error("E0102", span, format!("cannot {what} `{sn}` while threads of the enclosing `scope` may still use it"))
                    .note("a thread started with `spawn` keeps its borrows until the end of the `scope` block, even after `join()`");
                return;
            }
        }
    }

    /// Any use of `s` while an `&inout` into it is still used later is an error (A3): that
    /// reference has exclusive access.
    fn check_exclusive(&mut self, cx: &FnCx, s: LocalId, span: Span) {
        let name = cx.locals[s].name.clone();
        if cx.assignment_mut.contains(&s)
            || cx.locals[s].loans.iter().any(|l| matches!(l, Loan::Local(id) if cx.assignment_mut.contains(id)))
        {
            self.diags.error("E0102", span, format!("cannot use `{name}`: the pending assignment value has exclusive access to it"));
            return;
        }
        if cx.call_mut.contains(&s)
            || cx.locals[s].loans.iter().any(|l| matches!(l, Loan::Local(id) if cx.call_mut.contains(id)))
        {
            self.diags.error("E0102", span, format!("cannot use `{name}`: an earlier call argument has exclusive access to it"));
            return;
        }
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
        // A join handle stays in its scope (P3): bound by `let h = spawn ...`, used by `h.join()`.
        if matches!(h.ty, Ty::Sync(s) if sync_parts(s).0 == SyncKind::Join) && !matches!(h.kind, HExprKind::Spawn { .. }) {
            self.diags
                .error("U0001", h.span, "a `Join` handle cannot be moved yet; bind it with `let h = spawn ...` and call `h.join()`")
                .note("a handle must not leave its `scope`");
            return err_expr(h.span);
        }
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
                    // A reborrow depends on the descriptor for local permission,
                    // but its returned pointer does not point into that slot.
                    // Transitive pointee sources are present in the same loan set.
                    if !is_param && matches!(local.ty, Ty::RefMut(_)) && !local.loans.is_empty() {
                        continue;
                    }
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
        let loans_before = loans_snapshot(cx);
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
        // Loans written late in the body reach the next iteration's earlier statements:
        // replay the body until the loan state at its start stops growing.
        let loan_end = |cx: &FnCx, flow: &LoopFlow| {
            let mut end = loans_snapshot(cx);
            if let Some(c) = &flow.loan_continues {
                end = loans_union(&end, c);
            }
            let mut next = loans_union(&loans_before, &end);
            next.truncate(loans_before.len());
            next
        };
        let mut loans_next = loan_end(cx, &flow);
        if grew || loans_grew(&loans_before, &loans_next) {
            let saved = std::mem::take(&mut *self.diags);
            let end_cur = cx.cur;
            let mut kept: Vec<crate::diag::Diagnostic> = Vec::new();
            let mut rounds = 0;
            loop {
                cx.cur = start_cur;
                cx.moved = next.clone();
                loans_restore(cx, &loans_next);
                cx.loops.push(LoopFlow::default());
                cx.loop_depth += 1;
                let _ = self.block(body, cx);
                cx.loop_depth -= 1;
                let replay = cx.loops.pop().unwrap_or_default();
                if let Some(b) = &replay.breaks {
                    union_into(&mut flow.breaks, b);
                }
                if let Some(b) = &replay.loan_breaks {
                    loans_union_into(&mut flow.loan_breaks, b);
                }
                for d in std::mem::take(&mut self.diags.items) {
                    let kind = matches!(d.code, "E0402" | "E0303" | "E0305" | "E0306" | "E0102" | "E0703");
                    if kind && !kept.iter().any(|x| x.code == d.code && x.span == d.span) {
                        kept.push(d);
                    }
                }
                let widened = loans_union(&loans_next, &loan_end(cx, &replay));
                rounds += 1;
                if !loans_grew(&loans_next, &widened) || rounds >= 256 {
                    break;
                }
                loans_next = widened;
            }
            cx.cur = end_cur;
            *self.diags = saved;
            for d in kept {
                if !self.diags.items.iter().any(|x| x.code == d.code && x.span == d.span) {
                    self.diags.items.push(d);
                }
            }
        }
        cx.moved = match (infinite, flow.breaks) {
            (true, Some(b)) => b,
            (false, Some(b)) => union(&next, &b),
            (_, None) => next,
        };
        let after = match &flow.loan_breaks {
            Some(b) => loans_union(&loans_next, b),
            None => loans_next,
        };
        loans_restore(cx, &after);
        out
    }

    fn stmts(&mut self, stmts: &[Stmt], cx: &mut FnCx) -> Vec<HStmt> {
        stmts.iter().filter_map(|s| self.stmt(s, cx)).collect()
    }

    /// The type behind the transparent projections used by fields and indices.
    /// Permissions and moved-state checks are deliberately left to the real address check.
    fn address_projection_ty(mut ty: Ty) -> Ty {
        loop {
            ty = match ty {
                Ty::Ref(r) => ref_inner(r),
                Ty::RefMut(r) => ref_mut_inner(r),
                t => match ptr_inner(t) { Some(inner) => inner, None => return t },
            };
        }
    }

    /// Add only a pattern's names and types to a disposable type environment.
    /// The real pattern check still owns all diagnostics, permissions and loans.
    fn assignment_pattern_types(&self, pat: &Pattern, ty: Ty, cx: &mut FnCx) {
        match (pat, ty) {
            (Pattern::Bind { name, span }, ty) => {
                let id = cx.locals.len();
                cx.locals.push(LocalState { name: name.clone(), ty, kind: LocalKind::Let,
                    used: true, span: *span, decl: 0, scope_end: 0, depth: cx.scopes.len(),
                    loans: Vec::new(), map_key: false, guard: false, pinned: false });
                cx.scopes.last_mut().unwrap().push(id);
            }
            (Pattern::Some(inner), Ty::Opt(o)) => self.assignment_pattern_types(inner, opt_inner(o), cx),
            (Pattern::Ok(inner), Ty::Res(r)) => self.assignment_pattern_types(inner, res_parts(r).0, cx),
            (Pattern::Err(inner), Ty::Res(r)) => self.assignment_pattern_types(inner, res_parts(r).1, cx),
            (Pattern::Case { name, args, .. }, Ty::Enum(id)) => {
                if let Some(case) = self.structs[id as usize].variants.iter().find(|v| v.name == *name) {
                    for (p, (_, ty, _)) in args.iter().zip(&case.fields) { self.assignment_pattern_types(p, *ty, cx); }
                }
            }
            _ => {}
        }
    }

    /// Prepare the contextual type of an assignment target without evaluating it.
    /// Unlike `peek_ty`, this may instantiate a function signature, preserving all
    /// diagnostics, but cannot change locals, loans or move state (`cx` is read-only).
    fn assignment_target_ty(&mut self, e: &Expr, cx: &FnCx) -> Option<Ty> {
        match &e.kind {
            ExprKind::Paren(inner) => self.assignment_target_ty(inner, cx),
            ExprKind::Field { base, name } => {
                let ty = Self::address_projection_ty(self.assignment_target_ty(base, cx)?);
                self.struct_field(ty, name).map(|(_, ty)| ty)
            }
            ExprKind::Index { base, .. } => {
                let ty = Self::address_projection_ty(self.assignment_target_ty(base, cx)?);
                seq_elem(ty)
            }
            ExprKind::Unary { op: UnOp::Deref, expr, .. } => match self.assignment_target_ty(expr, cx)? {
                Ty::Ref(r) => Some(ref_inner(r)),
                Ty::RefMut(r) => Some(ref_mut_inner(r)),
                _ => None,
            },
            ExprKind::Coalesce { lhs, .. } => {
                let ty = self.assignment_target_ty(lhs, cx)?;
                let ty = match ty { Ty::Ref(r) => ref_inner(r), Ty::RefMut(r) => ref_mut_inner(r), t => t };
                match ty { Ty::Opt(o) => Some(opt_inner(o)), _ => None }
            }
            ExprKind::Try(value) | ExprKind::Catch { value, .. } => match self.assignment_target_ty(value, cx)? {
                Ty::Res(r) => Some(res_parts(r).0),
                _ => None,
            },
            ExprKind::Match { value, arms } => {
                let ty = self.assignment_target_ty(value, cx)?;
                let ty = match ty { Ty::Ref(r) => ref_inner(r), t => t };
                for arm in arms {
                    // This environment is used only by type/signature resolution,
                    // never by expr(), mutable_address() or a speculative check.
                    let mut types = FnCx::new(cx.ret, Pre::default());
                    types.locals = cx.locals.clone();
                    types.scopes = cx.scopes.clone();
                    types.scopes.push(Vec::new());
                    self.assignment_pattern_types(&arm.pat, ty, &mut types);
                    if let Some(t) = self.assignment_target_ty(&arm.body, &types) { return Some(t); }
                }
                None
            }
            ExprKind::Call { callee, args } => {
                match &callee.kind {
                    ExprKind::Ident(name) if self.lookup(cx, name).is_none() => {
                        if let Some(&di) = self.gen_fns.get(name) {
                            return Some(match self.prepare_generic_fn(di, None, args, callee.span, cx) {
                                Some(fi) => self.sig(fi).ret,
                                None => Ty::Error,
                            });
                        }
                        if let Some(&fi) = self.index.get(name) { return Some(self.sig(fi).ret); }
                    }
                    ExprKind::Generic { name, targs } if self.lookup(cx, name).is_none() => {
                        if let Some(&di) = self.gen_fns.get(name) {
                            return Some(match self.prepare_generic_fn(di, Some(targs), args, callee.span, cx) {
                                Some(fi) => self.sig(fi).ret,
                                None => Ty::Error,
                            });
                        }
                    }
                    ExprKind::Field { base, name } => {
                        if let ExprKind::Ident(owner) = &base.kind {
                            if self.lookup(cx, owner).is_none() {
                                if let Some(&fi) = self.index.get(&format!("{owner}.{name}")) {
                                    return Some(self.sig(fi).ret);
                                }
                            }
                        }
                        let ty = Self::address_projection_ty(self.assignment_target_ty(base, cx)?);
                        if let Some((fi, _)) = self.user_method(ty, name, callee.span) {
                            return Some(if fi == NO_FN { Ty::Error } else { self.sig(fi).ret });
                        }
                        if name == "getMut" {
                            let elem = match ty {
                                Ty::Map(m) => map_parts(m).1,
                                Ty::Arena(a) => vec_elem(a),
                                _ => return None,
                            };
                            return Some(Ty::Opt(opt_of(Ty::RefMut(ref_mut_of(elem)))));
                        }
                    }
                    _ => {}
                }
                self.peek_ty(e, cx)
            }
            _ => self.peek_ty(e, cx),
        }
    }

    /// `place[i](.field)* (op)= value` (C2, V5). The new value is computed first, so it may
    /// read the container (`dp[i] = dp[i - 1] + dp[i - 2]`); then the element is located and
    /// bounds-checked, the old value is dropped and the new one written, through a temporary
    /// `&inout` to the element. Lowered to `{ let $value = v; let $slot = &place; *$slot... = $value }`.
    #[allow(clippy::too_many_arguments)]
    fn index_assign(
        &mut self,
        target: &Expr,
        place: &Expr,
        fields_rev: &[(&str, Span)],
        op: Option<BinOp>,
        op_span: Span,
        value: &Expr,
        span: Span,
        cx: &mut FnCx,
    ) -> Option<HStmt> {
        let errors = self.diags.count(crate::diag::Severity::Error);
        let Some(expected) = self.assignment_target_ty(target, cx) else {
            // Retain normal diagnostics for invalid targets. For an as-yet
            // uninferrable complex target, reject rather than reorder its effects.
            self.expr(value, None, cx);
            self.mutable_address(place, cx);
            if self.diags.count(crate::diag::Severity::Error) == errors {
                self.diags.error("U0001", target.span, "cannot determine this indexed assignment's target type before evaluating it")
                    .note("bind the container reference to a variable, then assign through that variable");
            }
            return None;
        };
        cx.scopes.push(Vec::new());
        if op.is_none() {
            cx.diverge_ok = Some(value.span);
        }
        let v = self.expr(value, Some(expected), cx);
        cx.diverge_ok = None;
        if expected == Ty::Error {
            self.pop_scope(cx);
            return None;
        }
        let v = if op.is_none() { self.fit(v, expected, cx) } else { self.coerce(v, expected, cx) };
        let v = self.consume(v, cx);
        let vloans = if v.ty.carries_borrow() { self.loans_of(&v, cx) } else { Vec::new() };
        let shared_mark = cx.assignment_shared.len();
        let mutable_mark = cx.assignment_mut.len();
        let held = if v.ty.contains_ref_mut() { &mut cx.assignment_mut } else { &mut cx.assignment_shared };
        for loan in &vloans {
            if let Loan::Local(id) = *loan { held.push(id); }
        }
        let slot_h = self.mutable_address(place, cx);
        cx.assignment_shared.truncate(shared_mark);
        cx.assignment_mut.truncate(mutable_mark);
        let Ty::RefMut(r) = slot_h.ty else {
            self.pop_scope(cx);
            return None;
        };
        let Some((mut path, lty)) = self.field_path(ref_mut_inner(r), fields_rev) else {
            self.pop_scope(cx);
            return None;
        };
        path.insert(0, DEREF_STEP);
        // The full address check validates the type prepared without effects above.
        // No conversions may be postponed until this point: they can move or borrow.
        if lty != expected {
            self.diags.error("U0001", target.span, "the indexed assignment's contextual type could not be resolved consistently");
            self.pop_scope(cx);
            return None;
        }
        if lty.carries_borrow() && !vloans.is_empty() && !self.index_store_loans(place, &vloans, v.span, cx) {
            self.pop_scope(cx);
            return None;
        }
        let tv_name = format!("$value{}", cx.locals.len());
        let tv = self.declare(cx, &tv_name, value.span, v.ty, LocalKind::Let);
        cx.locals[tv].used = true;
        cx.locals[tv].loans = vloans;
        let mut stmts = vec![HStmt { kind: HStmtKind::Let { local: tv, init: v }, span }];
        let slot_loans = self.loans_of(&slot_h, cx);
        let slot_name = format!("$slot{}", cx.locals.len());
        let slot = self.declare(cx, &slot_name, place.span, slot_h.ty, LocalKind::Let);
        cx.locals[slot].used = true;
        cx.locals[slot].loans = slot_loans;
        stmts.push(HStmt { kind: HStmtKind::Let { local: slot, init: slot_h }, span });
        let tv_expr = HExpr { kind: HExprKind::Local(tv), ty: cx.locals[tv].ty, span: value.span, at: value.span };
        let new = match op {
            None => self.consume(tv_expr, cx),
            Some(op) => {
                let slot_expr = HExpr { kind: HExprKind::Local(slot), ty: cx.locals[slot].ty, span: place.span, at: place.span };
                let mut lhs = self.deref(slot_expr);
                for &fi in &path[1..] {
                    if fi == BOX_STEP {
                        lhs = self.unbox(lhs);
                        continue;
                    }
                    let Ty::Struct(sid) = lhs.ty else { unreachable!() };
                    let ty = self.structs[sid as usize].fields[fi].1;
                    lhs = HExpr { kind: HExprKind::Field { base: Box::new(lhs), field: fi }, ty, span: target.span, at: target.span };
                }
                self.arith(op, lhs, tv_expr, op_span, span)
            }
        };
        stmts.push(HStmt { kind: HStmtKind::Assign { local: slot, path, value: new }, span });
        self.pop_scope(cx);
        Some(HStmt { kind: HStmtKind::Block(stmts), span })
    }

    /// A borrowed value stored into an element: the container (a local variable) now also
    /// holds the value's sources, as `push` does (O4). Through a reference or into a caller's
    /// `inout` container this would add a new external source, which is not allowed.
    fn index_store_loans(&mut self, place: &Expr, loans: &[Loan], span: Span, cx: &mut FnCx) -> bool {
        let mut root = place;
        loop {
            match &root.kind {
                ExprKind::Index { base, .. } | ExprKind::Field { base, .. } => root = base,
                ExprKind::Paren(inner) => root = inner,
                _ => break,
            }
        }
        let ExprKind::Ident(name) = &root.kind else {
            self.diags.error("U0001", span, "storing a borrowed value through an `&inout` reference is not supported yet");
            return false;
        };
        let Some(id) = self.lookup(cx, name) else { return false };
        if cx.locals[id].kind == LocalKind::InoutParam {
            self.diags
                .error("E0306", span, format!("cannot store a borrow into the caller's `{name}`"))
                .note("return it instead, e.g. `-> Vec[str] of text`");
            return false;
        }
        if matches!(cx.locals[id].ty, Ty::Ref(_) | Ty::RefMut(_)) {
            self.diags.error("U0001", span, "storing a borrowed value through an `&inout` reference is not supported yet");
            return false;
        }
        self.store_loans(cx, id, loans.to_vec(), span, false);
        true
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
                let declared = ty.as_ref().map(|t| self.local_ty(t));
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
                    // Explicit discard consumes an owning value and drops it now (§6).
                    // `_name` is the distinct form that retains it until scope exit.
                    HStmtKind::Discard(self.consume(init, cx))
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
                // `*p = v` / `(*p).f = v`: write through the reference `p` (A6).
                let mut star: Option<Span> = None;
                loop {
                    match &root.kind {
                        ExprKind::Field { base, name } => {
                            fields_rev.push((name.as_str(), root.span));
                            root = base;
                        }
                        ExprKind::Paren(inner) => root = inner,
                        ExprKind::Unary { op: UnOp::Deref, expr, .. } if star.is_none() => {
                            star = Some(root.span);
                            root = expr;
                        }
                        _ => break,
                    }
                }
                // `v[i] = x`, `v[i].f += x`, `m[i][j] = x` (C2): written through a temporary
                // `&inout` to the element, after the new value has been computed (V5).
                if star.is_none() && matches!(root.kind, ExprKind::Index { .. }) {
                    return self.index_assign(target, root, &fields_rev, *op, *op_span, value, s.span, cx);
                }
                let ExprKind::Ident(name) = &root.kind else {
                    if star.is_some() {
                        self.diags.error("U0001", target.span, "only `*name` (a variable holding a reference) can be written through for now");
                    } else {
                        self.diags.error("U0001", target.span, "only variables and their struct fields can be assigned in P0");
                    }
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
                if let Some(star_span) = star {
                    cx.locals[id].used = true;
                    match root_ty {
                        Ty::RefMut(_) => {}
                        Ty::Ref(_) => {
                            self.diags
                                .error("E0101", star_span, format!("cannot write through `{name}`: it is a read-only reference (`{root_ty}`)"))
                                .note("a `var` binding does not make `&T` writable; borrow with `&inout T` instead");
                            self.expr(value, None, cx);
                            return None;
                        }
                        Ty::Error => {
                            self.expr(value, None, cx);
                            return None;
                        }
                        // `for x in xs` and `if let`/`match` bindings of a read-only place stand for
                        // a read-only reference even when the element is copied (A2).
                        _ if matches!(cx.locals[id].kind, LocalKind::LoopVar | LocalKind::Bind | LocalKind::Param) => {
                            self.diags
                                .error("E0101", star_span, format!("cannot write through `{name}`: it is read-only"))
                                .note(format!("to modify the elements, loop with `for {name} in &...`"));
                            self.expr(value, None, cx);
                            return None;
                        }
                        _ => {
                            self.diags
                                .error("E0001", star_span, format!("`*{name}` needs a reference, but `{name}` has type `{root_ty}`"))
                                .note(format!("assign to the variable directly: `{name} = ...`"));
                            self.expr(value, None, cx);
                            return None;
                        }
                    }
                }
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
                // Through a guard (writable) or an `Rc`/`Arc` (read-only) as well (P6).
                let shared = matches!(root_ty, Ty::Sync(_)) && ptr_inner(root_ty).is_some();
                let through = star.is_some() || !fields_rev.is_empty() && (matches!(root_ty, Ty::Ref(_) | Ty::RefMut(_)) || shared);
                if through && !self.ref_mut_writable(cx, id, target.span) {
                    self.expr(value, None, cx);
                    return None;
                }
                if cx.locals[id].map_key {
                    self.diags
                        .error("E0103", target.span, format!("cannot modify the Map key `{name}`"))
                        .note("remove the entry and insert it again under the new key");
                    self.expr(value, None, cx);
                    return None;
                }
                if through {
                    cx.locals[id].used = true;
                    if ptr_read_only(root_ty) {
                        self.read_only_ptr(root_ty, target.span);
                        self.expr(value, None, cx);
                        return None;
                    }
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
                let Some((mut path, lty)) = self.field_path(lty, &fields_rev) else {
                    self.expr(value, None, cx);
                    return None;
                };
                if star.is_some() {
                    path.insert(0, DEREF_STEP);
                }
                let value = match op {
                    None => {
                        self.check_not_borrowed(cx, id, target.span, if star.is_some() { "write through" } else { "assign to" });
                        cx.diverge_ok = Some(value.span);
                        let v = self.expr(value, Some(lty), cx);
                        cx.diverge_ok = None;
                        let v = self.fit(v, lty, cx);
                        let v = self.consume(v, cx);
                        if cx.locals[id].kind == LocalKind::InoutParam && lty.carries_borrow() && !self.loans_of(&v, cx).is_empty() {
                            self.diags
                                .error("E0306", v.span, format!("cannot store a borrow into the caller's `{name}`"))
                                .note("return it instead, e.g. `-> Vec[str] of text`");
                        } else if star.is_some() && lty.carries_borrow() && !self.loans_of(&v, cx).is_empty() {
                            self.diags.error("U0001", v.span, "storing a borrowed value through an `&inout` reference is not supported yet");
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
                        self.check_not_borrowed(cx, id, target.span, if star.is_some() { "write through" } else { "assign to" });
                        let mut lhs =
                            HExpr { kind: HExprKind::Local(id), ty: cx.locals[id].ty, span: root.span, at: root.span };
                        if through {
                            lhs = self.deref(lhs);
                        }
                        let mut ty = lhs.ty;
                        for &fi in &path {
                            if fi == DEREF_STEP {
                                continue;
                            }
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
                if matches!(e.kind, ExprKind::Try(_) | ExprKind::Catch { .. } | ExprKind::Match { .. } | ExprKind::Spawn(_)) {
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
                let loans_before = loans_snapshot(cx);
                let then_h = self.block(then, cx);
                let after_then = std::mem::replace(&mut cx.moved, before);
                let loans_then = loans_snapshot(cx);
                loans_restore(cx, &loans_before);
                let els_h = match els {
                    None => Vec::new(),
                    Some(Else::Block(b)) => self.block(b, cx),
                    Some(Else::If(s)) => self.stmt(s, cx).into_iter().collect(),
                };
                let after_else = std::mem::take(&mut cx.moved);
                let loans_else = loans_snapshot(cx);
                // A branch that cannot fall through does not affect the code after the `if`.
                let then_div = diverges(&then.stmts, cx);
                let else_div = match els {
                    Some(Else::Block(b)) => diverges(&b.stmts, cx),
                    Some(Else::If(s)) => diverges(std::slice::from_ref(s.as_ref()), cx),
                    None => false,
                };
                cx.moved = match (then_div, else_div) {
                    (true, false) => after_else,
                    (false, true) => after_then,
                    _ => union(&after_then, &after_else),
                };
                let joined = loans_join(&[(loans_then, then_div), (loans_else, else_div)], &loans_before);
                loans_restore(cx, &joined);
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
                let loans = loans_snapshot(cx);
                if let Some(flow) = cx.loops.last_mut() {
                    union_into(if is_break { &mut flow.breaks } else { &mut flow.continues }, &state);
                    loans_union_into(if is_break { &mut flow.loan_breaks } else { &mut flow.loan_continues }, &loans);
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
            StmtKind::With { name, name_span, init, body } => return self.with_stmt(name, *name_span, init, body, s.span, cx),
            StmtKind::Scope { body } => return self.scope_stmt(body, s.span, cx),
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
        let iterable = |t: Ty| matches!(t, Ty::Vec(_) | Ty::Map(_) | Ty::Set(_) | Ty::Slice(_));
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
            // `for x in s`: the elements, read-only (A5); the map's `bool` value is a hidden local.
            Ty::Set(mid) => {
                if let Some((_, s2)) = second {
                    self.diags.error("E0001", *s2, "a `Set` is iterated with one variable: `for x in s`");
                    self.pop_scope(cx);
                    return None;
                }
                if mutable {
                    self.diags
                        .error("E0103", first.1, "the elements of a `Set` cannot be modified")
                        .note("remove the element and add the new one");
                    self.pop_scope(cx);
                    return None;
                }
                let k = map_parts(mid).0;
                let kty = if k.is_copy() { k } else { Ty::Ref(ref_of(k)) };
                let key = bind(self, cx, first, kty);
                cx.locals[key].map_key = true;
                let val = self.declare(cx, "_", first.1, Ty::Bool, LocalKind::LoopVar);
                cx.locals[val].used = true;
                Shape::Map { key, key_ref: !k.is_copy(), val, val_ref: false }
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
        let loans_before = loans_snapshot(cx);
        let mut local = None;
        let (mut then_h, mut els_h) = (Vec::new(), Vec::new());
        let mut both_h = None;
        let mut ends: Vec<(Moved, bool)> = Vec::new();
        let mut loan_ends: Vec<(Loans, bool)> = Vec::new();
        for (i, (arm, role)) in arms.iter().zip(&roles).enumerate() {
            cx.moved = before.clone();
            loans_restore(cx, &loans_before);
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
                Body::Block(b) => diverges(&b.stmts, cx),
                Body::Stmt(s) => diverges(std::slice::from_ref(s), cx),
                Body::Empty => false,
            };
            match role {
                Role::Some => then_h = out,
                Role::None => els_h = out,
                Role::Both => both_h = Some(out),
                Role::Unreachable => continue,
            }
            ends.push((std::mem::take(&mut cx.moved), div));
            loan_ends.push((loans_snapshot(cx), div));
        }
        // A branch that cannot fall through does not affect the code after the `match`.
        let live: Vec<&Moved> = ends.iter().filter(|(_, d)| !d).map(|(m, _)| m).collect();
        let merged: Vec<&Moved> = if live.is_empty() { ends.iter().map(|(m, _)| m).collect() } else { live };
        cx.moved = match merged.split_first() {
            Some((first, rest)) => rest.iter().fold((*first).clone(), |acc, m| union(&acc, m)),
            None => before,
        };
        let joined = loans_join(&loan_ends, &loans_before);
        loans_restore(cx, &joined);
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
        let shared_mark = cx.call_shared.len();
        let mutable_mark = cx.call_mut.len();
        let mut parts = Vec::new();
        match &arg.kind {
            ExprKind::Str(segs) => {
                for seg in segs {
                    match seg {
                        StrSeg::Text(t) => parts.push(PrintPart::Text(t.clone())),
                        StrSeg::Expr { expr, spec } => {
                            let h = self.expr(expr, None, cx);
                            let h = self.deref(h);
                            // Codegen evaluates every part before writing any of them. A
                            // String/str (or an owning enum's payload) therefore keeps its
                            // backing storage borrowed while later parts run. Copy scalars
                            // and fresh owned results need no loan of their source locals.
                            self.hold_call_argument(&h, false, false, cx);
                            if let Some(spec) = spec {
                                // `.N`: fixed decimals for a float (the only specifier v1 defines).
                                let digits = spec.strip_prefix('.').filter(|d| !d.is_empty() && d.len() <= 2 && d.bytes().all(|b| b.is_ascii_digit()));
                                match digits.map(|d| d.parse::<u8>().unwrap_or(99)) {
                                    Some(n) if n <= 17 => match h.ty {
                                        Ty::Float(_) => parts.push(PrintPart::Fixed(h, n)),
                                        Ty::Error => {}
                                        other => {
                                            self.diags
                                                .error("E0001", expr.span, format!("`:.{n}` formats a float, but this value has type `{other}`"))
                                                .note("convert it first, e.g. `${f64(n):.2}`");
                                        }
                                    },
                                    Some(_) => {
                                        self.diags.error("E0001", expr.span, "at most 17 decimal places can be printed (`:.0` to `:.17`)");
                                    }
                                    None => {
                                        self.diags.error("U0001", expr.span, format!("the format specifier `:{spec}` is not supported; only `:.N` (fixed decimals) is"));
                                    }
                                }
                                continue;
                            }
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
        cx.call_shared.truncate(shared_mark);
        cx.call_mut.truncate(mutable_mark);
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
        if let Ty::Struct(_) | Ty::Vec(_) | Ty::Map(_) | Ty::Set(_) | Ty::Slice(_) = h.ty {
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
        // An integer literal where a float is expected is that float (N2: the context decides
        // a literal's type).
        if let Some(Ty::Float(f)) = expected {
            let v = mag as f64;
            return HExpr { kind: HExprKind::Float(if negative { -v } else { v }), ty: Ty::Float(f), span, at: span };
        }
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
        let hint = |t: Ty, fallback: Option<Ty>| if matches!(t, Ty::Int(_) | Ty::Float(_) | Ty::Enum(_)) { Some(t) } else { fallback };
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
            // IEEE 754 arithmetic: no panics; `%` and the wrapping and bit operators are for integers.
            (Ty::Float(a), Ty::Float(b)) if a == b && matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div) => l.ty,
            (Ty::Float(a), Ty::Float(b)) if a == b => {
                self.diags.error("E0001", span, format!("operator `{}` is not defined for `{a}`", op.symbol()));
                Ty::Error
            }
            (Ty::Float(a), Ty::Float(b)) => {
                self.diags
                    .error("E0001", span, format!("mismatched operand types for `{}`: `{a}` and `{b}`", op.symbol()))
                    .note(format!("convert one side explicitly, e.g. `{a}(...)`"));
                Ty::Error
            }
            (Ty::Int(a), Ty::Float(b)) | (Ty::Float(b), Ty::Int(a)) => {
                self.diags
                    .error("E0001", span, format!("mismatched operand types for `{}`: `{a}` and `{b}`", op.symbol()))
                    .note(format!("Yavo has no implicit conversions; convert explicitly, e.g. `{b}(n)` or `{a}(x)`"));
                Ty::Error
            }
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
            ExprKind::Float(v) => {
                let ft = match expected {
                    Some(Ty::Float(f)) => f,
                    _ => FloatTy::F64,
                };
                let v = if ft == FloatTy::F32 { *v as f32 as f64 } else { *v };
                HExpr { kind: HExprKind::Float(v), ty: Ty::Float(ft), span, at: span }
            }
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
                    Ty::Float(t) => HExpr { kind: HExprKind::Neg(Box::new(h)), ty: Ty::Float(t), span, at: *op_span },
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
            // `*p` reads the place `p` points to: a copy for a `Copy` target, otherwise read-only
            // access (A6). Never moves the target out.
            ExprKind::Unary { op: UnOp::Deref, op_span, expr: inner } => {
                let h = self.expr(inner, None, cx);
                match h.ty {
                    Ty::Ref(_) | Ty::RefMut(_) => self.deref(h),
                    Ty::Error => err_expr(span),
                    // `for x in xs` and read-only `if let` bindings stand for `&T` even when the
                    // element is copied, so `*x` reads the same value (A2, A6).
                    _ if matches!(&h.kind, HExprKind::Local(id) if matches!(cx.locals[*id].kind, LocalKind::LoopVar | LocalKind::Bind)) => h,
                    other => {
                        self.diags.error("E0001", *op_span, format!("`*` needs a reference (`&T` or `&inout T`), found `{other}`"));
                        err_expr(span)
                    }
                }
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
                            if tn == "math" && matches!(name.as_str(), "pi" | "e") {
                                let v = if name == "pi" { std::f64::consts::PI } else { std::f64::consts::E };
                                return HExpr { kind: HExprKind::Float(v), ty: Ty::Float(FloatTy::F64), span, at: span };
                            }
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
            ExprKind::StructLit { name, name_span, targs, fields } => self.struct_lit(name, *name_span, targs, fields, span, cx),
            ExprKind::Generic { name, .. } => {
                self.diags
                    .error("E0001", span, format!("`{name}[...]` with type arguments must be called, e.g. `{name}[int](x)`"))
                    .note("functions cannot be used as values");
                err_expr(span)
            }
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
            ExprKind::SliceRange { base, start, end } => self.slice_range(base, start, end, span, cx),
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
            // `&place` where an `&inout T` is expected: an exclusive, writable reference (A7).
            ExprKind::Borrow(inner) if matches!(expected, Some(Ty::RefMut(_))) => {
                let Some(Ty::RefMut(r)) = expected else { unreachable!() };
                self.borrow_mut(inner, r, span, cx)
            }
            // A standalone `&x` has no permission context (A7): `let p = &x`.
            ExprKind::Borrow(inner) if expected.is_none() => {
                self.diags
                    .error("E0004", span, "a borrow needs a type that says whether it is read-only or writable")
                    .note("write the type, e.g. `let p: &int = &x` or `let p: &inout int = &x`");
                self.expr(inner, None, cx);
                err_expr(span)
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
            ExprKind::Spawn(call) => self.spawn_expr(call, span, cx),
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
                let loans_before = loans_snapshot(cx);
                let r = self.diverge(rhs, cx);
                cx.moved = before;
                loans_restore(cx, &loans_before);
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
                let loans = loans_snapshot(cx);
                if let Some(flow) = cx.loops.last_mut() {
                    union_into(if is_break { &mut flow.breaks } else { &mut flow.continues }, &state);
                    loans_union_into(if is_break { &mut flow.loan_breaks } else { &mut flow.loan_continues }, &loans);
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
        let shared = matches!(root_ty, Ty::Sync(_)) && ptr_inner(root_ty).is_some();
        let through = !fields_rev.is_empty() && (matches!(root_ty, Ty::Ref(_) | Ty::RefMut(_)) || shared);
        if through {
            if !self.ref_mut_writable(cx, id, root.span) { return None; }
            if ptr_read_only(root_ty) {
                self.read_only_ptr(root_ty, root.span);
                return None;
            }
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
            while let Some(inner) = ptr_inner(ty) {
                if ptr_read_only(ty) {
                    self.read_only_ptr(ty, *fspan);
                    return None;
                }
                path.push(BOX_STEP);
                ty = inner;
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

    /// Writing through an `Rc`/`Arc` (P6): they give shared read-only access.
    fn read_only_ptr(&mut self, ty: Ty, span: Span) {
        self.diags
            .error("E0101", span, format!("cannot modify through `{ty}`: it gives shared, read-only access"))
            .note("to share data that changes, put it in a `Mutex`, e.g. `Arc[Mutex[T]]`, and write through `lock()`");
    }

    /// The content of a `Box`, `Rc`, `Arc` or `Guard` (nested ones too) where a field, method or
    /// element is used (A7).
    fn unbox(&self, mut h: HExpr) -> HExpr {
        while let Some(inner) = ptr_inner(h.ty) {
            let (span, at) = (h.span, h.at);
            h = HExpr { kind: HExprKind::Unbox(Box::new(h)), ty: inner, span, at };
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
                // `String` and `str` compare by their text (K1): the owned side is viewed.
                let view = |h: HExpr| {
                    let (span, at) = (h.span, h.at);
                    HExpr { kind: HExprKind::StrView(Box::new(h)), ty: Ty::Str, span, at }
                };
                let (l, r) = match (l.ty, r.ty) {
                    (Ty::String, Ty::Str) => (view(l), r),
                    (Ty::Str, Ty::String) => (l, view(r)),
                    _ => (l, r),
                };
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
                    } else if (ordering && !matches!(l.ty, Ty::Int(_) | Ty::Float(_) | Ty::Str | Ty::String | Ty::Id(_)))
                        || !matches!(l.ty, Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Str | Ty::String | Ty::Id(_))
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
                if let Some(to) = FloatTy::from_name(name) {
                    return self.float_conversion(to, args, callee.span, span, cx);
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
                        "Rc" => return self.sync_new(SyncKind::Rc, args, expected, span, callee.span, cx),
                        "Arc" => return self.sync_new(SyncKind::Arc, args, expected, span, callee.span, cx),
                        "Mutex" => return self.sync_new(SyncKind::Mutex, args, expected, span, callee.span, cx),
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
                                Ty::Float(_) => {
                                    let ty = h.ty;
                                    HExpr { kind: HExprKind::Math { func: "fabs", args: vec![h] }, ty, span, at: callee.span }
                                }
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
                if let Some(&di) = self.gen_fns.get(name) {
                    return self.generic_call(di, None, args, span, callee.span, cx);
                }
                self.call_fn(fi, name, None, args, span, callee.span, cx)
            }
            ExprKind::Generic { name, targs } => match self.gen_fns.get(name) {
                Some(&di) if self.lookup(cx, name).is_none() => self.generic_call(di, Some(targs), args, span, callee.span, cx),
                _ => {
                    let what = if self.index.contains_key(name) { "is not generic" } else { "is not a generic function" };
                    self.diags.error("E0001", callee.span, format!("`{name}` {what}, so it takes no type arguments"));
                    for a in args {
                        self.expr(a, None, cx);
                    }
                    err_expr(span)
                }
            },
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
        let skip = usize::from(recv.is_some());
        let params = self.sig(fi).params.clone();
        let sinks = self.sig(fi).sinks.clone();
        let inouts = self.sig(fi).inouts.clone();
        let ret = self.sig(fi).ret;
        if args.len() + skip != params.len() {
            let what = if skip == 1 { "method" } else { "function" };
            self.diags.error("E0012", span,
                format!("{what} `{name}` takes {} but {} {} supplied",
                    plural(params.len().saturating_sub(skip), "argument"), args.len(),
                    if args.len() == 1 { "was" } else { "were" }));
        }
        let shared_mark = cx.call_shared.len();
        let mutable_mark = cx.call_mut.len();
        let mut hargs = Vec::new();
        let reserved_mark = cx.call_reserved.len();
        let mut reserved: Option<(LocalId, String)> = None;
        if let Some(r) = recv {
            let recv_inout = inouts.first().copied().unwrap_or(false);
            let root = match &r.kind {
                HExprKind::InoutArg(id) => Some(*id),
                HExprKind::InoutPlace(p) => Some(p.local),
                _ => None,
            };
            match root {
                // Two-phase (V2): an `inout self` receiver is reserved while the arguments run;
                // they may read it (`c.add(c.n)`), but not modify, move or keep borrowing it.
                Some(id) if recv_inout => {
                    let call = format!("{}.{}(...)", cx.locals[id].name, name.rsplit('.').next().unwrap_or(name));
                    cx.call_reserved.push((id, call.clone()));
                    reserved = Some((id, call));
                }
                _ => self.hold_call_argument(&r, recv_inout, sinks.first().copied().unwrap_or(false), cx),
            }
            hargs.push(r);
        }
        for (i, a) in args.iter().enumerate() {
            let i = i + skip;
            let exp = params.get(i).copied();
            let inout = inouts.get(i).copied().unwrap_or(false);
            let sink = sinks.get(i).copied().unwrap_or(false);
            if inout {
                let h = self.inout_arg(a, exp, &[], &mut Vec::new(), cx);
                self.hold_call_argument(&h, true, false, cx);
                hargs.push(h);
                continue;
            }
            let h = self.expr(a, exp, cx);
            let h = if matches!(h.ty, Ty::Ref(_) | Ty::RefMut(_)) && !matches!(exp, Some(Ty::Ref(_) | Ty::RefMut(_))) {
                self.deref(h)
            } else { h };
            let h = if exp == Some(Ty::Str) && h.ty == Ty::String {
                let (span, at) = (h.span, h.at);
                HExpr { kind: HExprKind::StrView(Box::new(h)), ty: Ty::Str, span, at }
            } else { h };
            let h = match (exp, h.ty) {
                (Some(Ty::Slice(s)), Ty::Vec(v)) if slice_elem(s) == vec_elem(v) => {
                    let (span, at) = (h.span, h.at);
                    HExpr { kind: HExprKind::SliceOf(Box::new(h)), ty: Ty::Slice(s), span, at }
                }
                _ => h,
            };
            let h = match exp { Some(t) => self.fit(h, t, cx), None => h };
            let h = if sink { self.consume(h, cx) } else { h };
            if let Some((rid, call)) = &reserved {
                self.reserved_conflict(&h, *rid, !sink, call, cx);
            }
            self.hold_call_argument(&h, false, sink, cx);
            hargs.push(h);
        }
        cx.call_reserved.truncate(reserved_mark);
        cx.call_shared.truncate(shared_mark);
        cx.call_mut.truncate(mutable_mark);
        let kind = match self.dyn_slots.get(&fi) {
            Some(&(iface, method)) => HExprKind::DynCall { iface, method, sig: fi, args: hargs },
            None => HExprKind::Call { func: fi, args: hargs },
        };
        HExpr { kind, ty: ret, span, at }
    }

    fn hold_call_argument(&self, h: &HExpr, inout: bool, sink: bool, cx: &mut FnCx) {
        if !inout && h.ty.is_copy() && !h.ty.carries_borrow() {
            return;
        }
        let mut loans = self.loans_of(h, cx);
        if inout || !sink && !h.ty.is_copy() {
            match &h.kind {
                HExprKind::InoutArg(id) => {
                    loans.push(Loan::Local(*id));
                    loans.extend(cx.locals[*id].loans.iter().copied());
                }
                HExprKind::InoutPlace(_) => {}
                _ => loans.extend(self.storage_loans(h, cx)),
            }
        }
        let dst = if inout || h.ty.contains_ref_mut() { &mut cx.call_mut } else { &mut cx.call_shared };
        for loan in loans {
            if let Loan::Local(id) = loan {
                dst.push(id);
            }
        }
    }

    /// First type for which the built-in recursive clone has no valid implementation.
    /// Copying a shared handle does not clone its payload; owning containers do.
    fn clone_blocker(&self, ty: Ty, seen: &mut Vec<Ty>) -> Option<Ty> {
        match ty {
            Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Unit | Ty::Str | Ty::String | Ty::Error
            | Ty::Ref(_) | Ty::Slice(_) | Ty::Id(_) => None,
            Ty::RefMut(_) => Some(ty),
            Ty::Dyn(id) => {
                // All implementations are registered before checking bodies in the
                // current single-unit compiler. Preserve vtable cloning only when
                // every possible concrete value has a valid recursive clone.
                // Separate compilation must carry this guarantee explicitly.
                let base = dyn_base(id);
                let key = Ty::Dyn(base);
                if seen.contains(&key) {
                    return None;
                }
                seen.push(key);
                self.impls.iter().filter(|imp| imp.iface == base as usize)
                    .find_map(|imp| self.clone_blocker(imp.ty, seen))
            }
            Ty::Sync(id) => match sync_parts(id).0 {
                SyncKind::Rc | SyncKind::Arc | SyncKind::Channel => None,
                SyncKind::Mutex | SyncKind::Guard | SyncKind::Join | SyncKind::Scope => Some(ty),
            },
            Ty::Vec(id) | Ty::Deque(id) => self.clone_blocker(vec_elem(id), seen),
            Ty::Heap(_) | Ty::Arena(_) => Some(ty),
            Ty::Box(id) => self.clone_blocker(box_inner(id), seen),
            Ty::Opt(id) => self.clone_blocker(opt_inner(id), seen),
            Ty::Map(id) | Ty::Set(id) => {
                let (k, v) = map_parts(id);
                self.clone_blocker(k, seen).or_else(|| self.clone_blocker(v, seen))
            }
            Ty::Res(id) => {
                let (o, e) = res_parts(id);
                self.clone_blocker(o, seen).or_else(|| self.clone_blocker(e, seen))
            }
            Ty::Tuple(id) => tuple_elems(id).into_iter().find_map(|t| self.clone_blocker(t, seen)),
            Ty::Struct(_) | Ty::Enum(_) => {
                // A user destructor can own a resource not represented by its fields.
                // Fieldwise cloning must not invent a second owner of that resource.
                if has_user_drop(ty) {
                    return Some(ty);
                }
                if seen.contains(&ty) {
                    return None;
                }
                seen.push(ty);
                let Some(s) = self.structs.iter().find(|s| s.ty == ty) else { return Some(ty) };
                s.fields.iter().map(|f| f.1)
                    .chain(s.variants.iter().flat_map(|v| v.fields.iter().map(|f| f.1)))
                    .find_map(|t| self.clone_blocker(t, seen))
            }
        }
    }

    fn method_call(&mut self, base: &Expr, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        if let ExprKind::Generic { name: tn, targs } = &base.kind {
            if tn == "Heap" && name == "by" && self.lookup(cx, tn).is_none() {
                let ty = self.resolve_ty(&TypeExpr { name: tn.clone(), span: base.span, args: targs.clone() });
                let Ty::Heap(id) = ty else { return err_expr(span) };
                let [callback] = args else {
                    self.diags.error("E0012", span, "`Heap[T].by` takes one comparison closure");
                    return err_expr(span);
                };
                let elem = vec_elem(id);
                let Some((func, caps)) = self.lift_closure(callback, &[elem, elem], Ty::Bool, "Heap.by", true, cx) else { return err_expr(span) };
                let tramp = self.tramps.len();
                self.tramps.push(Tramp { func, caps: caps.iter().map(|c| c.ty).collect(), elem, by_cmp: false, saved: true });
                return HExpr { kind: HExprKind::HeapNew { tramp, caps }, ty, span, at };
            }
        }
        // `v[i].push(x)`, `grid[r][c].f.set(k, x)`, `pts[i].moveBy(1)`: a modifying method
        // on an element. The element is located (index evaluated, bounds checked) and
        // borrowed mutably first, then the arguments run (V2): they may not use `v`.
        if Self::indexed_receiver(base) && self.mutating_method(base, name, cx) {
            let init = self.mutable_address(base, cx);
            if !matches!(init.ty, Ty::RefMut(_)) {
                for a in args {
                    self.expr(a, None, cx);
                }
                return err_expr(span);
            }
            let loans = self.loans_of(&init, cx);
            if self.borrowed_argument_conflict(args, &loans, cx, name) {
                return err_expr(span);
            }
            cx.scopes.push(Vec::new());
            let temp_name = format!("$receiver{}", cx.locals.len());
            let local = self.declare(cx, &temp_name, base.span, init.ty, LocalKind::Let);
            cx.locals[local].used = true;
            cx.locals[local].loans = loans;
            let receiver = Expr { kind: ExprKind::Ident(temp_name), span: base.span };
            let value = self.method_call_inner(&receiver, name, args, span, at, cx, None);
            self.pop_scope(cx);
            let ty = value.ty;
            return HExpr { kind: HExprKind::LetTemp { local, init: Box::new(init), value: Box::new(value) }, ty, span, at };
        }
        // Calls returning an exclusive descriptor can be receivers directly. Bind
        // the descriptor once; the callee's declared sources remain pinned while
        // the method arguments run. Owned temporary receivers keep the old rules.
        if self.peek_place_ty(base, cx).is_none()
            && matches!(base.kind, ExprKind::Call { .. } | ExprKind::Paren(_) | ExprKind::Field { .. })
        {
            let init = self.expr(base, None, cx);
            if matches!(init.ty, Ty::RefMut(_)) {
                let loans = self.loans_of(&init, cx);
                if self.borrowed_argument_conflict(args, &loans, cx, name) {
                    return err_expr(span);
                }
                cx.scopes.push(Vec::new());
                let temp_name = format!("$receiver{}", cx.locals.len());
                let local = self.declare(cx, &temp_name, base.span, init.ty, LocalKind::Let);
                cx.locals[local].used = true;
                cx.locals[local].loans = loans;
                let receiver = Expr { kind: ExprKind::Ident(temp_name), span: base.span };
                let value = self.method_call_inner(&receiver, name, args, span, at, cx, None);
                self.pop_scope(cx);
                let ty = value.ty;
                return HExpr { kind: HExprKind::LetTemp { local, init: Box::new(init), value: Box::new(value) }, ty, span, at };
            }
            return self.method_call_inner(base, name, args, span, at, cx, Some(init));
        }
        // Named exclusive references need the same transitive alias check as
        // temporary ones; checking only the descriptor spelling misses its owner.
        let mut root = base;
        loop {
            match &root.kind {
                ExprKind::Field { base, .. } | ExprKind::Paren(base) => root = base,
                _ => break,
            }
        }
        if let ExprKind::Ident(n) = &root.kind {
            if let Some(id) = self.lookup(cx, n) {
                if matches!(cx.locals[id].ty, Ty::RefMut(_)) {
                    let mut loans = vec![Loan::Local(id)];
                    loans.extend(cx.locals[id].loans.iter().copied());
                    if self.borrowed_argument_conflict(args, &loans, cx, name) {
                        return err_expr(span);
                    }
                }
            }
        }
        self.method_call_inner(base, name, args, span, at, cx, None)
    }

    /// `v[i]`, `v[i].f`, `(g[r][c]).f`: a receiver whose field chain passes through an index.
    fn indexed_receiver(base: &Expr) -> bool {
        let mut e = base;
        loop {
            match &e.kind {
                ExprKind::Field { base, .. } | ExprKind::Paren(base) => e = base,
                ExprKind::Index { .. } => return true,
                _ => return false,
            }
        }
    }

    /// Whether `base.name(...)` modifies its receiver: a built-in modifying method of the
    /// receiver's collection type, or a user method declared with `inout self`. Decided
    /// from types alone, without diagnostics or effects.
    fn mutating_method(&mut self, base: &Expr, name: &str, cx: &FnCx) -> bool {
        let Some(mut ty) = self.peek_ty(base, cx) else { return false };
        loop {
            ty = match ty {
                Ty::Ref(r) => ref_inner(r),
                Ty::RefMut(r) => ref_mut_inner(r),
                t => match ptr_inner(t) {
                    Some(inner) => inner,
                    None => break,
                },
            };
        }
        match ty {
            Ty::Vec(_) => matches!(name, "push" | "pop" | "reserve" | "sort" | "sortBy"),
            Ty::String => name == "push",
            Ty::Deque(_) => matches!(name, "pushBack" | "pushFront" | "popBack" | "popFront"),
            Ty::Heap(_) => matches!(name, "push" | "pop"),
            Ty::Map(_) => matches!(name, "set" | "remove" | "getMut" | "update"),
            Ty::Set(_) => matches!(name, "add" | "remove"),
            Ty::Arena(_) => matches!(name, "add" | "remove" | "getMut"),
            Ty::Struct(_) | Ty::Enum(_) => {
                if let Some((sdi, _)) = self.inst_of.get(&ty) {
                    let full = format!("{}.{name}", self.prog.structs[*sdi].name);
                    return self.prog.fns.iter().find(|f| f.name == full)
                        .and_then(|f| f.params.first())
                        .is_some_and(|p| p.name == "self" && p.inout);
                }
                let Some(s) = self.structs.iter().find(|s| s.ty == ty) else { return false };
                let full = format!("{}.{name}", s.name);
                self.index.get(&full).is_some_and(|&fi| {
                    let sig = self.sig(fi);
                    sig.has_self && sig.inouts.first().copied().unwrap_or(false)
                })
            }
            _ => false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn method_call_inner(&mut self, base: &Expr, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx, checked: Option<HExpr>) -> HExpr {
        // `Type.f(args)`: a function written inside a struct or enum, without `self`.
        if let ExprKind::Ident(tn) = &base.kind {
            if self.lookup(cx, tn).is_none() {
                let full = format!("{tn}.{name}");
                if let Some(&fi) = self.index.get(&full) {
                    if self.sig(fi).has_self {
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
            while let Some(inner) = ptr_inner(t) {
                t = inner;
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
                if tn == "String" && name == "fromUtf8" {
                    return self.from_utf8(args, span, at, cx);
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
        if matches!(name, "pushBack" | "pushFront" | "popBack" | "popFront") && matches!(self.peek_place_ty(base, cx), Some(Ty::Deque(_))) {
            return self.with_reservation(cx, |s, cx| s.deque_call(base, name, args, span, at, cx));
        }
        if name == "push" {
            return self.with_reservation(cx, |s, cx| s.push_call(base, args, span, at, cx));
        }
        if name == "pop" {
            return self.pop_call(base, args, span, at, cx);
        }
        if name == "reserve" {
            if let Some(Ty::Vec(_)) = self.peek_place_ty(base, cx) {
                let Some(place) = self.mutable_place(base, "reserve", cx) else { return err_expr(span) };
                let [arg] = args else {
                    self.diags.error("E0012", span, "`reserve` takes exactly one argument (the capacity)");
                    return err_expr(span);
                };
                let int = Ty::Int(IntTy::I64);
                let n = self.expr(arg, Some(int), cx);
                let n = self.fit(n, int, cx);
                return HExpr { kind: HExprKind::Reserve { place, n: Box::new(n) }, ty: Ty::Unit, span, at };
            }
        }
        if name == "sort" {
            return self.sort_call(base, args, span, at, cx);
        }
        if name == "sortBy" {
            return self.sort_by_call(base, args, span, at, cx);
        }
        if matches!(name, "set" | "remove" | "getMut" | "update") {
            if let Some(Ty::Map(mid)) = self.peek_place_ty(base, cx) {
                return self.with_reservation(cx, |s, cx| s.map_write_call(base, mid, name, args, span, at, cx));
            }
        }
        if matches!(name, "add" | "remove") {
            if let Some(Ty::Set(mid)) = self.peek_place_ty(base, cx) {
                return self.with_reservation(cx, |s, cx| s.set_write_call(base, mid, name, args, span, at, cx));
            }
        }
        if matches!(name, "add" | "remove" | "getMut") {
            if let Some(mut ty) = self.peek_place_ty(base, cx) {
                while let Some(inner) = ptr_inner(ty) { ty = inner; }
                if let Ty::Arena(id) = ty {
                    // Box and Guard project writable storage; mutable_place still rejects
                    // shared Rc/Arc pointers and read-only parameter/binding access.
                    return self.with_reservation(cx, |s, cx| s.arena_write_call(base, id, name, args, span, at, cx));
                }
            }
        }
        // `h.join()` consumes the handle (`Join.join(sink self)`): a second join is `E0402`.
        if name == "join" && args.is_empty() {
            if let ExprKind::Ident(n) = &base.kind {
                if let Some(id) = self.lookup(cx, n) {
                    if let Ty::Sync(sid) = cx.locals[id].ty {
                        if let (SyncKind::Join, ret) = sync_parts(sid) {
                            cx.locals[id].used = true;
                            self.check_moved(cx, id, base.span);
                            let recv = HExpr { kind: HExprKind::Local(id), ty: cx.locals[id].ty, span: base.span, at: base.span };
                            if id < cx.moved.len() {
                                cx.moved[id] = Some(base.span);
                            }
                            return HExpr { kind: HExprKind::JoinCall(Box::new(recv)), ty: ret, span, at };
                        }
                    }
                }
            }
        }
        let recv = checked.unwrap_or_else(|| self.expr(base, None, cx));
        let recv = self.deref(recv);
        // `.clone()` of a shared handle shares the same block (it does not clone the content).
        if name == "clone" && args.is_empty() {
            if let Ty::Sync(id) = recv.ty {
                if matches!(sync_parts(id).0, SyncKind::Rc | SyncKind::Arc | SyncKind::Channel) {
                    let ty = recv.ty;
                    return HExpr { kind: HExprKind::SyncClone(Box::new(recv)), ty, span, at };
                }
            }
        }
        let recv = self.unbox(recv);
        if let Ty::Sync(id) = recv.ty {
            let (kind, inner) = sync_parts(id);
            if kind == SyncKind::Channel && matches!(name, "send" | "recv" | "close") {
                let want = usize::from(name == "send");
                if args.len() != want {
                    let what = if want == 1 { "exactly one argument (the value)" } else { "no arguments" };
                    self.diags.error("E0012", span, format!("`{name}` takes {what}"));
                    return err_expr(span);
                }
                let chan = Box::new(recv);
                return match name {
                    "send" => {
                        let v = self.expr(&args[0], Some(inner), cx);
                        let v = self.fit(v, inner, cx);
                        let v = self.consume(v, cx);
                        let err = self.send_error_ty(inner);
                        HExpr { kind: HExprKind::ChanSend { chan, value: Box::new(v) }, ty: Ty::Res(res_of(Ty::Unit, err)), span, at }
                    }
                    "recv" => HExpr { kind: HExprKind::ChanRecv(chan), ty: Ty::Opt(opt_of(inner)), span, at },
                    _ => HExpr { kind: HExprKind::ChanClose(chan), ty: Ty::Unit, span, at },
                };
            }
            if kind == SyncKind::Mutex && matches!(name, "lock" | "tryLock") {
                if !args.is_empty() {
                    self.diags.error("E0012", span, format!("`{name}` takes no arguments"));
                    return err_expr(span);
                }
                let guard = Ty::Sync(sync_of(SyncKind::Guard, inner));
                let try_ = name == "tryLock";
                let ty = if try_ { Ty::Opt(opt_of(guard)) } else { guard };
                return HExpr { kind: HExprKind::Lock { mutex: Box::new(recv), try_ }, ty, span, at };
            }
        }
        if let Some((fi, full)) = self.user_method(recv.ty, name, at) {
            if fi == NO_FN {
                return err_expr(span);
            }
            if self.sig(fi).inouts[0] {
                self.diags
                    .error("E0014", base.span, format!("`{full}` takes `inout self`, so it must be called on a variable or a field of one"))
                    .note("store the value in a `var` first");
                return err_expr(span);
            }
            let recv = if self.sig(fi).sinks[0] { self.consume(recv, cx) } else { recv };
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
        if let Ty::Set(mid) = recv.ty {
            match name {
                "has" => return self.map_read(recv, mid, name, args, span, at, cx),
                "add" | "remove" => {
                    self.diags.error("U0001", at, format!("`{name}` is only supported on a variable for now"));
                    return err_expr(span);
                }
                _ => {}
            }
        }
        if let Ty::Arena(id) = recv.ty {
            match name {
                "get" | "getCopy" | "has" => return self.arena_read(recv, id, name, args, span, at, cx),
                "add" | "remove" | "getMut" => {
                    self.diags.error("E0014", at, format!("`{name}` requires a writable Arena position"));
                    return err_expr(span);
                }
                _ => {}
            }
        }
        if name == "clone" && matches!(recv.ty, Ty::Vec(_) | Ty::Deque(_) | Ty::Heap(_) | Ty::Arena(_) | Ty::Map(_) | Ty::Set(_)) {
            if let Some(blocker) = self.clone_blocker(recv.ty, &mut Vec::new()) {
                let code = if has_user_drop(blocker) { "U0001" } else { "E0001" };
                self.diags
                    .error(code, at, format!("cannot automatically clone `{}`: `{blocker}` has no built-in clone semantics", recv.ty))
                    .note("rebuild owned resources explicitly; `Rc`, `Arc` and `Channel` handles can be cloned without cloning their contents");
                return err_expr(span);
            }
        }
        let (method, ty) = match (recv.ty, name) {
            (Ty::Str, "toString") => (Builtin::StrToString, Ty::String),
            (Ty::Str, "len") => (Builtin::StrLen, Ty::Int(IntTy::I64)),
            (Ty::String, "len") => (Builtin::StringLen, Ty::Int(IntTy::I64)),
            (Ty::String, "clone") => (Builtin::StringClone, Ty::String),
            (Ty::Vec(_), "len") => (Builtin::VecLen, Ty::Int(IntTy::I64)),
            (Ty::Deque(_), "len") => (Builtin::VecLen, Ty::Int(IntTy::I64)),
            (Ty::Arena(_), "len") => (Builtin::VecLen, Ty::Int(IntTy::I64)),
            (Ty::Heap(_), "len") => (Builtin::HeapLen, Ty::Int(IntTy::I64)),
            (Ty::Slice(_), "len") => (Builtin::VecLen, Ty::Int(IntTy::I64)),
            (Ty::Vec(_), "clone") => (Builtin::VecClone, recv.ty),
            (Ty::Deque(_), "clone") => (Builtin::VecClone, recv.ty),
            (Ty::Slice(s), "toVec") => {
                let e = slice_elem(s);
                if !e.is_copy() {
                    self.diags
                        .error("E0001", at, format!("`toVec` copies the elements, and `{e}` is not copyable"))
                        .note("build the vector yourself, calling `.clone()` on each element");
                    return err_expr(span);
                }
                (Builtin::SliceToVec, Ty::Vec(vec_of(e)))
            }
            (Ty::Str | Ty::String, "trim") => (Builtin::Trim, Ty::Str),
            (Ty::Str | Ty::String, "fields") => (Builtin::Fields, Ty::Vec(vec_of(Ty::Str))),
            (Ty::Str | Ty::String, "lines") => (Builtin::Lines, Ty::Vec(vec_of(Ty::Str))),
            (Ty::Opt(_), "isSome") => (Builtin::IsSome, Ty::Bool),
            (Ty::Opt(_), "isNone") => (Builtin::IsNone, Ty::Bool),
            (Ty::Map(_), "len") => (Builtin::MapLen, Ty::Int(IntTy::I64)),
            (Ty::Map(_), "clone") => (Builtin::MapClone, recv.ty),
            (Ty::Set(_), "len") => (Builtin::MapLen, Ty::Int(IntTy::I64)),
            (Ty::Set(_), "clone") => (Builtin::MapClone, recv.ty),
            (Ty::Error, _) => return err_expr(span),
            (Ty::Dyn(i), m) => {
                let iname = self.ifaces[dyn_base(i) as usize].name.clone();
                self.diags
                    .error("E0010", at, format!("interface `{iname}` has no method `{m}`"))
                    .note("a value behind an interface can only use the interface's methods");
                for a in args {
                    self.expr(a, None, cx);
                }
                return err_expr(span);
            }
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
        if let Ty::Dyn(i) = t {
            let d = &self.ifaces[dyn_base(i) as usize];
            let (_, s, _) = d.methods.iter().find(|(n, _, _)| n == name)?;
            return Some((*s, format!("{}.{name}", d.name)));
        }
        // A method of a generic struct's instance: instantiated on first use.
        if let Some((sdi, targs)) = self.inst_of.get(&t).cloned() {
            let owner = self.prog.structs[sdi].name.clone();
            let full = format!("{owner}.{name}");
            let di = self.prog.fns.iter().position(|f| f.name == full)?;
            let shown = format!("{t}.{name}");
            if self.prog.fns[di].params.first().is_none_or(|p| p.name != "self") {
                self.diags.error("E0001", at, format!("`{shown}` has no `self` parameter: call it on the type"));
                return Some((NO_FN, shown));
            }
            let fi = self.instantiate_fn(di, targs, Some((owner, t)), at);
            return Some((fi, shown));
        }
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
        if !self.sig(fi).has_self {
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
        let recv = if self.sig(fi).inouts[0] {
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
            if self.sig(fi).sinks[0] {
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
        cx.exit_calls.insert((span.start, span.end));
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

    #[allow(clippy::too_many_arguments)]
    fn arena_read(&mut self, recv: HExpr, id: u32, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let [arg] = args else {
            self.diags.error("E0012", span, format!("`{name}` takes exactly one argument (the Id)"));
            return err_expr(span);
        };
        let shared_mark = cx.call_shared.len();
        let mutable_mark = cx.call_mut.len();
        self.hold_call_argument(&recv, false, false, cx);
        let key = self.expr(arg, Some(Ty::Id(id)), cx);
        let key = self.fit(key, Ty::Id(id), cx);
        cx.call_shared.truncate(shared_mark);
        cx.call_mut.truncate(mutable_mark);
        let elem = vec_elem(id);
        let (op, ty) = match name {
            "get" => (MapRead::Get, Ty::Opt(opt_of(Ty::Ref(ref_of(elem))))),
            "getCopy" => {
                if !elem.is_copy() {
                    self.diags.error("E0404", span, format!("`getCopy` needs a copyable element type, and `{elem}` is not"))
                        .note("borrow the element with `get`, or remove it to take ownership");
                    return err_expr(span);
                }
                (MapRead::GetCopy, Ty::Opt(opt_of(elem)))
            }
            _ => (MapRead::Has, Ty::Bool),
        };
        HExpr { kind: HExprKind::ArenaRead { op, recv: Box::new(recv), key: Box::new(key) }, ty, span, at }
    }

    #[allow(clippy::too_many_arguments)]
    fn arena_write_call(&mut self, base: &Expr, aid: u32, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let Some(place) = self.mutable_place(base, name, cx) else { return err_expr(span) };
        let local = place.local;
        let owner = cx.locals[local].name.clone();
        let [arg] = args else {
            self.diags.error("E0012", span, format!("`{name}` takes exactly one argument"));
            return err_expr(span);
        };
        let elem = vec_elem(aid);
        let wanted = if name == "add" { elem } else { Ty::Id(aid) };
        let mark = cx.call_mut.len();
        let shared_mark = cx.call_shared.len();
        cx.call_reserved.push((local, format!("{owner}.{name}(...)")));
        let source_loans = cx.locals[local].loans.clone();
        let exclusive = cx.locals[local].ty.contains_ref_mut() || cx.locals[local].guard;
        for loan in source_loans {
            if let Loan::Local(id) = loan {
                if exclusive { cx.call_mut.push(id); } else { cx.call_shared.push(id); }
            }
        }
        let value = self.expr(arg, Some(wanted), cx);
        let value = self.fit(value, wanted, cx);
        let value = if name == "add" { self.consume(value, cx) } else { value };
        cx.call_mut.truncate(mark);
        cx.call_shared.truncate(shared_mark);
        if name == "add" && elem.carries_borrow() {
            let loans = self.loans_of(&value, cx);
            if !loans.is_empty() {
                if matches!(cx.locals[local].ty, Ty::RefMut(_)) {
                    self.diags.error("U0001", span, "storing a borrowed value through an `&inout` reference is not supported yet");
                } else if cx.locals[local].kind == LocalKind::InoutParam {
                    self.diags.error("E0306", span, format!("cannot store a borrow into the caller's `{owner}`"))
                        .note("return the Arena with an explicit `of` source instead");
                } else {
                    self.store_loans(cx, local, loans, span, false);
                }
            }
        }
        let (op, ty) = match name {
            "add" => (ArenaWrite::Add, Ty::Id(aid)),
            "remove" => (ArenaWrite::Remove, Ty::Opt(opt_of(elem))),
            _ => (ArenaWrite::GetMut, Ty::Opt(opt_of(Ty::RefMut(ref_mut_of(elem))))),
        };
        HExpr { kind: HExprKind::ArenaWrite { op, place, value: Box::new(value) }, ty, span, at }
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
        // Two-phase (V2): the arguments may read the collection, but not modify or move it.
        let call = format!("{local}.{name}(...)");
        cx.call_reserved.push((id, call.clone()));
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
                if self.reserved_conflict(&kh, id, false, &call, cx) || self.reserved_conflict(&vh, id, false, &call, cx) {
                    return err_expr(span);
                }
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
            _ => {
                let key = self.map_query(&args[0], k, cx);
                if self.reserved_conflict(&key, id, true, &call, cx) {
                    return err_expr(span);
                }
                if name == "remove" {
                    (MapWrite::Remove, vec![key], Ty::Opt(opt_of(v)))
                } else {
                    (MapWrite::GetMut, vec![key], Ty::Opt(opt_of(Ty::RefMut(ref_mut_of(v)))))
                }
            }
        };
        HExpr { kind: HExprKind::MapWrite { op, place, args: hargs }, ty, span, at }
    }

    /// `s.add(x)` (consumes `x`; adding an element that is already there changes nothing) and
    /// `s.remove(x)` (`bool`: whether it was there) on a writable set (stdlib contract §3).
    #[allow(clippy::too_many_arguments)]
    fn set_write_call(&mut self, base: &Expr, mid: u32, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let Some(place) = self.mutable_place(base, name, cx) else { return err_expr(span) };
        let id = place.local;
        let local = cx.locals[id].name.clone();
        // Two-phase (V2): the arguments may read the collection, but not modify or move it.
        let call = format!("{local}.{name}(...)");
        cx.call_reserved.push((id, call.clone()));
        let [arg] = args else {
            self.diags.error("E0012", span, format!("`{name}` takes exactly one argument (the element)"));
            return err_expr(span);
        };
        let k = map_parts(mid).0;
        if name == "remove" {
            let key = self.map_query(arg, k, cx);
            if self.reserved_conflict(&key, id, true, &call, cx) {
                return err_expr(span);
            }
            let removed =
                HExpr { kind: HExprKind::MapWrite { op: MapWrite::Remove, place, args: vec![key] }, ty: Ty::Opt(opt_of(Ty::Bool)), span, at };
            return HExpr { kind: HExprKind::Method { method: Builtin::IsSome, recv: Box::new(removed) }, ty: Ty::Bool, span, at };
        }
        let kh = self.expr(arg, Some(k), cx);
        let kh = self.fit(kh, k, cx);
        let kh = self.consume(kh, cx);
        if self.reserved_conflict(&kh, id, false, &call, cx) {
            return err_expr(span);
        }
        if kh.ty.carries_borrow() {
            let loans = self.loans_of(&kh, cx);
            if !loans.is_empty() {
                if matches!(cx.locals[id].ty, Ty::RefMut(_)) {
                    self.diags.error("U0001", span, "storing a borrowed value through an `&inout` reference is not supported yet");
                } else if cx.locals[id].kind == LocalKind::InoutParam {
                    self.diags.error("E0306", span, format!("cannot store a borrow into the caller's `{local}`"));
                } else {
                    self.store_loans(cx, id, loans, span, false);
                }
            }
        }
        let yes = HExpr { kind: HExprKind::Bool(true), ty: Ty::Bool, span, at };
        HExpr { kind: HExprKind::MapWrite { op: MapWrite::Set, place, args: vec![kh, yes] }, ty: Ty::Unit, span, at }
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
                while let Some(inner) = ptr_inner(t) {
                    t = inner;
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
            if let ExprKind::Ident(n) = &base.kind {
                if let Some(id) = self.lookup(cx, n) {
                    let t = Self::receiver_ty(cx, id);
                    if ptr_read_only(t) {
                        cx.locals[id].used = true;
                        self.read_only_ptr(t, base.span);
                        return None;
                    }
                }
            }
            let id = self.mutable_receiver(base, method, cx)?;
            let (mut path, mut ty) = (Vec::new(), Self::receiver_ty(cx, id));
            // A method of the content of a box (`b.push(x)` on a `Box[Vec[int]]`) works in place.
            while let Some(inner) = ptr_inner(ty) {
                if ptr_read_only(ty) {
                    self.read_only_ptr(ty, base.span);
                    return None;
                }
                path.push(BOX_STEP);
                ty = inner;
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
            Ty::RefMut(_) => {
                if !self.ref_mut_writable(cx, id, root.span) { return None; }
                self.check_exclusive(cx, id, root.span);
            }
            // A locally owned guard grants exclusive access (P6); borrowing the guard as
            // a default parameter does not pass that exclusive capability to the callee.
            Ty::Sync(sid) if sync_parts(sid).0 == SyncKind::Guard => {
                if !self.ref_mut_writable(cx, id, root.span) { return None; }
            }
            t if ptr_read_only(t) => {
                self.read_only_ptr(t, root.span);
                return None;
            }
            _ => {
                if !self.writable(cx, id, root.span) {
                    return None;
                }
            }
        }
        let (mut path, mut ty) = self.field_path(Self::receiver_ty(cx, id), &fields_rev)?;
        while let Some(inner) = ptr_inner(ty) {
            if ptr_read_only(ty) {
                self.read_only_ptr(ty, base.span);
                return None;
            }
            path.push(BOX_STEP);
            ty = inner;
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
        if !self.ref_mut_writable(cx, id, base.span) { return None; }
        self.check_not_borrowed(cx, id, base.span, "modify");
        let guard = matches!(cx.locals[id].ty, Ty::Sync(sid) if sync_parts(sid).0 == SyncKind::Guard);
        if matches!(cx.locals[id].ty, Ty::RefMut(_)) || guard || self.writable(cx, id, base.span) {
            Some(id)
        } else {
            None
        }
    }

    fn ref_mut_writable(&mut self, cx: &FnCx, id: LocalId, span: Span) -> bool {
        let local = &cx.locals[id];
        let guard = matches!(local.ty, Ty::Sync(sid) if sync_parts(sid).0 == SyncKind::Guard);
        if (matches!(local.ty, Ty::RefMut(_)) || guard) && local.kind == LocalKind::Param {
            self.diags.error("E0101", span, format!("cannot modify through `{}`: a default parameter only grants read-only access", cx.locals[id].name));
            false
        } else if guard && matches!(local.kind, LocalKind::Bind | LocalKind::LoopVar) {
            self.diags.error("E0101", span, format!("cannot modify through `{}`: this binding only grants read-only access", local.name));
            false
        } else { true }
    }

    /// The type a receiver variable stands for: the target of an `&inout T`.
    fn receiver_ty(cx: &FnCx, id: LocalId) -> Ty {
        match cx.locals[id].ty {
            Ty::RefMut(r) => ref_mut_inner(r),
            t => t,
        }
    }

    fn deque_call(&mut self, base: &Expr, method: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let Some(place) = self.mutable_place(base, method, cx) else { return err_expr(span) };
        let Ty::Deque(did) = place.ty else { return err_expr(span) };
        let et = vec_elem(did);
        let front = method.ends_with("Front");
        if method.starts_with("pop") {
            if !args.is_empty() {
                self.diags.error("E0012", span, format!("`{method}` takes no arguments"));
                return err_expr(span);
            }
            return HExpr { kind: HExprKind::DequePop { place, front }, ty: Ty::Opt(opt_of(et)), span, at };
        }
        let [arg] = args else {
            self.diags.error("E0012", span, format!("`{method}` takes one value"));
            return err_expr(span);
        };
        let id = place.local;
        let name = cx.locals[id].name.clone();
        let call = format!("{name}.{method}(...)");
        cx.call_reserved.push((id, call.clone()));
        let h = self.expr(arg, Some(et), cx);
        let h = self.fit(h, et, cx);
        let h = self.consume(h, cx);
        if self.reserved_conflict(&h, id, false, &call, cx) {
            return err_expr(span);
        }
        if et.carries_borrow() {
            let loans = self.loans_of(&h, cx);
            if !loans.is_empty() && (matches!(cx.locals[id].ty, Ty::RefMut(_)) || cx.locals[id].kind == LocalKind::InoutParam) {
                self.diags.error("E0306", h.span, format!("cannot store a borrow into the caller's `{name}`"));
            } else {
                self.store_loans(cx, id, loans, h.span, false);
            }
        }
        HExpr { kind: HExprKind::DequePush { place, value: Box::new(h), front }, ty: Ty::Unit, span, at }
    }

    /// `v.pop()`: the last element as an owned `?T`.
    fn pop_call(&mut self, base: &Expr, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let Some(place) = self.mutable_place(base, "pop", cx) else { return err_expr(span) };
        if !args.is_empty() {
            self.diags.error("E0012", span, "`pop` takes no arguments");
            return err_expr(span);
        }
        match place.ty {
            Ty::Vec(vid) | Ty::Heap(vid) => HExpr { kind: HExprKind::Pop(place), ty: Ty::Opt(opt_of(vec_elem(vid))), span, at },
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
        // Two-phase (V2): the argument may read the vector, but not modify or move it.
        let call = format!("{name}.push(...)");
        cx.call_reserved.push((id, call.clone()));
        let rty = place.ty;
        let value = match rty {
            Ty::Vec(vid) | Ty::Heap(vid) => {
                let et = vec_elem(vid);
                let errors = self.diags.count(crate::diag::Severity::Error);
                let h = self.expr(arg, Some(et), cx);
                let h = self.fit(h, et, cx);
                let h = self.consume(h, cx);
                // One mistake, one error: a value already rejected is not reported again.
                if self.diags.count(crate::diag::Severity::Error) == errors && self.reserved_conflict(&h, id, false, &call, cx) {
                    return err_expr(span);
                }
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
                if self.reserved_conflict(&h, id, true, &call, cx) {
                    return err_expr(span);
                }
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
        // `var s: Set[String] = []`: an empty set.
        if let (Some(t @ Ty::Set(_)), true) = (expected, items.is_empty()) {
            return HExpr { kind: HExprKind::MapLit(Vec::new()), ty: t, span, at: span };
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
        if let (Some(t @ Ty::Set(_)), true) = (expected, entries.is_empty()) {
            return HExpr { kind: HExprKind::MapLit(Vec::new()), ty: t, span, at: span };
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
            self.diags.error("E0501", span, format!("`{kt}` cannot be a Map key")).note("keys can be integers, bool, str, String or Id");
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
                self.mark_type_used(n, span);
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

    /// `v[a..b]` / `s[a..b]`: a read-only view; out-of-range bounds panic (P-INDEX), and text
    /// bounds must fall on character boundaries (P-STR-BOUNDARY, S3).
    fn slice_range(&mut self, base: &Expr, start: &Expr, end: &Expr, span: Span, cx: &mut FnCx) -> HExpr {
        let b = self.expr(base, None, cx);
        let b = self.deref(b);
        let b = self.unbox(b);
        let int = Ty::Int(IntTy::I64);
        let mut bound = |this: &mut Self, e: &Expr| -> HExpr {
            let h = this.expr(e, Some(int), cx);
            let h = this.deref(h);
            match h.ty {
                Ty::Int(IntTy::I64) | Ty::Error => {}
                other => {
                    this.diags.error("E0001", h.span, format!("a slice bound must be an `int`, found `{other}`"));
                }
            }
            h
        };
        let s = bound(self, start);
        let e = bound(self, end);
        let ty = match b.ty {
            Ty::Error => return err_expr(span),
            Ty::Vec(v) => Ty::Slice(slice_of(vec_elem(v))),
            Ty::Slice(_) => b.ty,
            Ty::Str | Ty::String => Ty::Str,
            other => {
                self.diags.error("E0001", b.span, format!("a value of type `{other}` cannot be sliced"));
                return err_expr(span);
            }
        };
        if s.ty == Ty::Error || e.ty == Ty::Error {
            return err_expr(span);
        }
        HExpr { kind: HExprKind::SliceRange { base: Box::new(b), start: Box::new(s), end: Box::new(e) }, ty, span, at: span }
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
        let loans_before = loans_snapshot(cx);
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
            let div = diverges(&b.stmts, cx);
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
        let loans_after = loans_snapshot(cx);
        let joined = if div { loans_before } else { loans_union(&loans_before, &loans_after) };
        loans_restore(cx, &joined);
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
        let loans_before = loans_snapshot(cx);
        let mut harms: Vec<HArm> = Vec::new();
        let mut ends: Vec<(Moved, bool)> = Vec::new();
        let mut loan_ends: Vec<(Loans, bool)> = Vec::new();
        let mut all_ok = mty != Ty::Error;
        let mut pats_ok: Vec<bool> = Vec::new();
        let mut result_ty = expected.filter(|t| !matches!(t, Ty::Error | Ty::Unit));
        for (i, arm) in arms.iter().enumerate() {
            cx.moved = before.clone();
            loans_restore(cx, &loans_before);
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
                    (out, None, diverges(&b.stmts, cx))
                }
                GBody::Expr(e) => match &e.kind {
                    ExprKind::Block(b) => {
                        let out = self.stmts(&b.stmts, cx);
                        if !diverges(&b.stmts, cx) {
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
            loan_ends.push((loans_snapshot(cx), div));
            harms.push(HArm { pat, body, value });
        }
        // A branch that cannot fall through does not affect the code after the `match`.
        let live: Vec<&Moved> = ends.iter().filter(|(_, d)| !d).map(|(m, _)| m).collect();
        let merged: Vec<&Moved> = if live.is_empty() { ends.iter().map(|(m, _)| m).collect() } else { live };
        cx.moved = match merged.split_first() {
            Some((first, rest)) => rest.iter().fold((*first).clone(), |acc, m| union(&acc, m)),
            None => before,
        };
        let joined = loans_join(&loan_ends, &loans_before);
        loans_restore(cx, &joined);
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
                    // `ok(_u)` on a `() ! E`: nothing to bind; an ignored name is a wildcard.
                    if name.starts_with('_') {
                        return HPat::Wild;
                    }
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
        if cond.is_none() {
            cx.exit_calls.insert((span.start, span.end));
        }
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
            (Ty::Float(x), Ty::Float(y)) if x == y => {
                HExpr { kind: HExprKind::MinMax { max, a: Box::new(ha), b: Box::new(hb) }, ty: Ty::Float(x), span, at }
            }
            (x, y) => {
                self.diags
                    .error("E0001", span, format!("`{name}` needs two numbers of the same type, found `{x}` and `{y}`"))
                    .note("convert one side explicitly, e.g. `int(...)`");
                err_expr(span)
            }
        }
    }

    /// `String.fromUtf8(bytes)` (stdlib contract §5): consumes the bytes; on success the buffer
    /// becomes the `String`, on failure it is freed and the error carries the offset.
    fn from_utf8(&mut self, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> HExpr {
        let [arg] = args else {
            self.diags.error("E0012", span, "`String.fromUtf8` takes exactly one argument (a `Vec[byte]`)");
            return err_expr(span);
        };
        let bytes = Ty::Vec(vec_of(Ty::Int(IntTy::U8)));
        let h = self.expr(arg, Some(bytes), cx);
        let h = self.fit(h, bytes, cx);
        if h.ty == Ty::Error {
            return err_expr(span);
        }
        let h = self.consume(h, cx);
        let err = self.structs[self.struct_index["Utf8Error"]].ty;
        let ty = Ty::Res(res_of(Ty::String, err));
        HExpr { kind: HExprKind::FromUtf8(Box::new(h)), ty, span, at }
    }

    /// `io.readAll()`, `io.readLine()`, `fs.readText(path)`, `fs.writeText(path, text)`,
    /// `os.args()` (stdlib contract §5). `None` for other module members.
    fn module_call(&mut self, m: &str, name: &str, args: &[Expr], span: Span, at: Span, cx: &mut FnCx) -> Option<HExpr> {
        if m == "math" {
            let (func, n) = match name {
                "sqrt" => ("sqrt", 1),
                "floor" => ("floor", 1),
                "ceil" => ("ceil", 1),
                "round" => ("round", 1),
                "pow" => ("pow", 2),
                _ => return None,
            };
            if args.len() != n {
                self.diags.error("E0012", span, format!("`math.{name}` takes {}", plural(n, "argument")));
                return Some(err_expr(span));
            }
            let f64t = Ty::Float(FloatTy::F64);
            let mut hargs = Vec::new();
            for a in args {
                let h = self.expr(a, Some(f64t), cx);
                let h = self.fit(h, f64t, cx);
                hargs.push(h);
            }
            return Some(HExpr { kind: HExprKind::Math { func, args: hargs }, ty: f64t, span, at });
        }
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
            ("fs", "writeNewText") => (IoOp::WriteNewText, 2, Ty::Res(res_of(Ty::Unit, lib(self, "fs.Error")))),
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
                Ty::Int(_) | Ty::Str | Ty::String | Ty::Id(_) => HExpr { kind: HExprKind::Sort(place), ty: Ty::Unit, span, at },
                Ty::Error => err_expr(span),
                e if self.protocol(e).is_some_and(|(cmp, _)| cmp.is_some()) => {
                    let func = self.protocol(e).and_then(|(cmp, _)| cmp).expect("checked above");
                    let tramp = self.tramps.len();
                    self.tramps.push(Tramp { func, caps: Vec::new(), elem: e, by_cmp: true, saved: false });
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
            ("parseFloat", _) if text => (Method2::ParseFloat, Ty::Opt(opt_of(Ty::Float(FloatTy::F64))), None),
            ("totalCmp", Ty::Float(_)) => {
                let ord = self.structs[self.struct_index["Ordering"]].ty;
                (Method2::TotalCmp, ord, Some(recv.ty))
            }
            ("contains", Ty::Vec(_) | Ty::Slice(_)) => {
                let e = seq_elem(recv.ty).expect("a sequence");
                if !matches!(e, Ty::Int(_) | Ty::Bool | Ty::Str | Ty::String | Ty::Id(_)) {
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
            Ty::Float(_) if checked => HExpr { kind: HExprKind::FloatToInt { to, expr: Box::new(h) }, ty: Ty::Int(to), span, at },
            Ty::Error => err_expr(span),
            other => {
                let d = self.diags.error("E0001", arg.span, format!("cannot convert `{other}` to `{to}`"));
                if other == Ty::Bool {
                    d.note("use `if b { 1 } else { 0 }` logic to turn a bool into a number");
                } else if matches!(other, Ty::Float(_)) {
                    d.note(format!("`{to}.wrap` only takes integers; `{to}(x)` truncates a float (and panics when it does not fit)"));
                }
                err_expr(span)
            }
        }
    }

    /// `f64(x)` / `f32(x)`: from an integer (rounding to the nearest float) or the other float
    /// type.
    fn float_conversion(&mut self, to: FloatTy, args: &[Expr], at: Span, span: Span, cx: &mut FnCx) -> HExpr {
        let [arg] = args else {
            self.diags.error("E0012", span, format!("`{to}(x)` takes exactly one argument", to = to.name()));
            return err_expr(span);
        };
        let h = self.expr(arg, Some(Ty::Float(to)), cx);
        let h = self.deref(h);
        match h.ty {
            Ty::Float(from) if from == to => h,
            Ty::Int(_) | Ty::Float(_) => HExpr { kind: HExprKind::ToFloat(Box::new(h)), ty: Ty::Float(to), span, at },
            Ty::Error => err_expr(span),
            other => {
                self.diags.error("E0001", arg.span, format!("cannot convert `{other}` to `{}`", to.name()));
                err_expr(span)
            }
        }
    }
}
