use crate::diag::Span;

pub struct Program {
    pub files: Vec<SourceFile>,
    pub fns: Vec<FnDecl>,
    pub structs: Vec<StructDecl>,
    pub enums: Vec<EnumDecl>,
    pub interfaces: Vec<InterfaceDecl>,
    pub uses: Vec<Use>,
}

/// Original file boundaries in the combined compilation source.
#[derive(Clone)]
pub struct SourceFile {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub line: u32,
}

/// `interface Name { fn method(self, ...) -> T }`: method signatures without bodies. Each
/// method's `name` is `Name.method` and its body is empty.
pub struct InterfaceDecl {
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub methods: Vec<FnDecl>,
}

/// One module named by `use std/m` or `use std/{a, b}`.
pub struct Use {
    pub path: String,
    pub module: String,
    pub span: Span,
}

pub struct StructDecl {
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    /// `struct Wrapper[T] { ... }`: a generic struct; each `Wrapper[X]` is a struct of its own.
    pub type_params: Vec<TypeParam>,
    pub fields: Vec<FieldDecl>,
    /// `struct X: Drop + Named`: the interfaces it declares to implement.
    pub impls: Vec<(String, Span)>,
    /// Methods written in the body; moved into `Program::fns` by the parser.
    pub methods: Vec<FnDecl>,
}

/// `T` or `T: Ord + Copy` in `fn f[T: Ord + Copy]` / `struct S[T]`.
pub struct TypeParam {
    pub name: String,
    pub span: Span,
    pub bounds: Vec<(String, Span)>,
}

/// `enum Name { Case, Case(field: T, ...) }`
pub struct EnumDecl {
    pub public: bool,
    pub name: String,
    pub name_span: Span,
    pub variants: Vec<VariantDecl>,
    pub impls: Vec<(String, Span)>,
    pub methods: Vec<FnDecl>,
}

pub struct VariantDecl {
    pub name: String,
    pub span: Span,
    pub fields: Vec<FieldDecl>,
}

pub struct FieldDecl {
    pub name: String,
    pub span: Span,
    pub ty: TypeExpr,
}

pub struct FieldInit {
    pub name: String,
    pub span: Span,
    pub value: Expr,
}

pub struct FnDecl {
    pub public: bool,
    /// A method's name is `Type.method`.
    pub name: String,
    pub name_span: Span,
    /// For a method: the struct or enum it belongs to. Its receiver, if any, is the first
    /// parameter, named `self`.
    pub owner: Option<String>,
    /// `fn largest[T: Ord + Copy](...)`: a generic function, checked once per instantiation.
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    /// `-> T of a` / `of (a, b)` / `of static`: where a returned borrow may come from.
    pub ret_of: Option<Vec<(String, Span)>>,
    pub body: Block,
}

pub struct Param {
    pub name: String,
    pub span: Span,
    /// `x: sink T`: the argument is moved into the function. Otherwise parameters are read-only borrows.
    pub sink: bool,
    /// `x: inout T`: the caller passes `&place`; `x` is that place, readable and writable.
    pub inout: bool,
    pub ty: TypeExpr,
}

#[derive(Clone)]
pub struct TypeExpr {
    pub name: String,
    pub span: Span,
    /// Type arguments, e.g. `[int]` in `Vec[int]`. `?T` is `name: "?"` and `&T` is `name: "&"`,
    /// with `T` as the only argument; `T ! E` is `name: "!"` with arguments `T` and `E`; the
    /// unit type `()` is `name: "()"`; a tuple type `(A, B)` is `name: "(,)"` with its elements.
    pub args: Vec<TypeExpr>,
}

pub struct Block {
    pub stmts: Vec<Stmt>,
    pub close: Span,
}

pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

pub enum StmtKind {
    Let { mutable: bool, name: String, name_span: Span, ty: Option<TypeExpr>, init: Expr },
    /// `let (a, b) = value`: takes a tuple apart (`_` skips an element).
    LetTuple { mutable: bool, names: Vec<(String, Span)>, init: Expr },
    Assign { target: Expr, op: Option<BinOp>, op_span: Span, value: Expr },
    Expr(Expr),
    If { cond: Expr, then: Block, els: Option<Else> },
    /// `if let name = value { } else { }`: runs `then` when `value` (an optional) holds a value.
    IfLet { name: String, name_span: Span, value: Expr, then: Block, els: Option<Else> },
    /// `match value { pattern => body ... }`; a single-statement arm body is wrapped in a block.
    Match { value: Expr, arms: Vec<Arm> },
    While { cond: Expr, body: Block },
    Loop { body: Block },
    For { var: String, var_span: Span, start: Expr, end: Expr, body: Block },
    /// `for x in v`: copyable elements by value, others as `&T`; `for i, x in v` adds the index,
    /// `for k, v in m` walks a map; `in &v` iterates with `&inout` access.
    ForIn { var: String, var_span: Span, var2: Option<(String, Span)>, iter: Expr, body: Block },
    Break,
    Continue,
    Return(Option<Expr>),
    Block(Block),
    /// `with g = m.lock() { }`: `g` (a lock guard) is bound for the block and released at its
    /// end; borrows taken through it cannot leave the block (P4).
    With { name: String, name_span: Span, init: Expr, body: Block },
    /// `scope { }`: threads started with `spawn` inside are all joined before the block is left,
    /// however it is left (P3).
    Scope { body: Block },
}

pub enum Else {
    Block(Block),
    If(Box<Stmt>),
}

pub struct Arm {
    pub pat: Pattern,
    pub span: Span,
    pub body: Block,
}

/// One arm of a `match` used as an expression: `pattern => value`.
pub struct ExprArm {
    pub pat: Pattern,
    pub span: Span,
    /// A value; or `return`/`break`/`continue`; or a block that must not fall through.
    pub body: Expr,
}

pub enum Pattern {
    /// A name that binds the matched value.
    Bind { name: String, span: Span },
    /// `some(p)`
    Some(Box<Pattern>),
    None,
    /// `_`
    Wild,
    /// `ok(p)`, `err(p)`
    Ok(Box<Pattern>),
    Err(Box<Pattern>),
    /// `.Case`, `.Case(p, ...)` or `Type.Case(...)`, with one pattern per payload field.
    Case { ty: Option<(String, Span)>, name: String, span: Span, args: Vec<Pattern> },
}

pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

pub enum ExprKind {
    Int(u128),
    /// `1.5`: an `f64` unless the context asks for `f32`.
    Float(f64),
    /// `'a'`: a `byte`.
    Byte(u8),
    Bool(bool),
    Str(Vec<StrSeg>),
    Ident(String),
    Call { callee: Box<Expr>, args: Vec<Expr> },
    /// `base.name`; a tuple element is `base.0` (`name` is the digits).
    Field { base: Box<Expr>, name: String },
    /// `Name{field: value, ...}`; fields are evaluated in the order written.
    /// `Name{...}`, or `Name[A, B]{...}` for a generic struct (`targs`).
    StructLit { name: String, name_span: Span, targs: Vec<TypeExpr>, fields: Vec<FieldInit> },
    /// `name[A, B]` before a call: a generic function with explicit type arguments.
    Generic { name: String, targs: Vec<TypeExpr> },
    /// `[a, b, c]`
    VecLit(Vec<Expr>),
    /// `[:]` or `[k: v, ...]`
    MapLit(Vec<(Expr, Expr)>),
    /// `base[index]`
    Index { base: Box<Expr>, index: Box<Expr> },
    /// `base[start..end]`: a view of a vector, slice or text.
    SliceRange { base: Box<Expr>, start: Box<Expr>, end: Box<Expr> },
    /// `&x`, for an `inout` argument.
    Borrow(Box<Expr>),
    Unary { op: UnOp, op_span: Span, expr: Box<Expr> },
    Binary { op: BinOp, op_span: Span, lhs: Box<Expr>, rhs: Box<Expr> },
    Paren(Box<Expr>),
    /// `none`
    None,
    /// `lhs ?? rhs`: the value inside `lhs`, or `rhs` (evaluated only when `lhs` is `none`).
    Coalesce { op_span: Span, lhs: Box<Expr>, rhs: Box<Expr> },
    /// `return [value]`, `break`, `continue`; only as the right side of `??`.
    Return(Option<Box<Expr>>),
    Break,
    Continue,
    /// `()`
    Unit,
    /// `(a, b, ...)`, at least two elements.
    Tuple(Vec<Expr>),
    /// `(a, b) => value` or `(a, b) => { block }`; parameters may have types (`a: int`).
    Closure { params: Vec<(String, Span, Option<TypeExpr>)>, body: Box<Expr> },
    /// `.Case` or `.Case(args)`: an enum case whose type comes from the context.
    Case { name: String, args: Option<Vec<Expr>> },
    /// `name: value`, a named argument of an enum case.
    Named { name: String, name_span: Span, value: Box<Expr> },
    /// `err(value)`
    Err(Box<Expr>),
    /// `try value`
    Try(Box<Expr>),
    /// `spawn f(args)`: runs a call on a new thread of the enclosing `scope`; a `Join[T]`.
    Spawn(Box<Expr>),
    /// `value catch name => handler` or `value catch name { block }` (`block`: the handler is an
    /// `ExprKind::Block` that must not fall through).
    Catch { value: Box<Expr>, name: String, name_span: Span, handler: Box<Expr>, block: bool },
    /// `match value { pattern => value ... }` used as an expression.
    Match { value: Box<Expr>, arms: Vec<ExprArm> },
    /// A block in an expression position (a `catch` handler or a `match` arm); it must leave
    /// with `return`, `break`, `continue`, `panic` or `os.exit`.
    Block(Block),
}

pub enum StrSeg {
    Text(String),
    Expr { expr: Expr, spec: Option<String> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    /// `*p`: the place a reference points to (A6).
    Deref,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    AddWrap,
    SubWrap,
    MulWrap,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::AddWrap => "+%",
            BinOp::SubWrap => "-%",
            BinOp::MulWrap => "*%",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}
