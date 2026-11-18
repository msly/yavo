use crate::diag::Span;

pub struct Program {
    pub fns: Vec<FnDecl>,
    pub structs: Vec<StructDecl>,
}

pub struct StructDecl {
    pub name: String,
    pub name_span: Span,
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
    pub name: String,
    pub name_span: Span,
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
    pub ty: TypeExpr,
}

pub struct TypeExpr {
    pub name: String,
    pub span: Span,
    /// Type arguments, e.g. `[int]` in `Vec[int]`.
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
    Assign { target: Expr, op: Option<BinOp>, op_span: Span, value: Expr },
    Expr(Expr),
    If { cond: Expr, then: Block, els: Option<Else> },
    While { cond: Expr, body: Block },
    Loop { body: Block },
    For { var: String, var_span: Span, start: Expr, end: Expr, body: Block },
    Break,
    Continue,
    Return(Option<Expr>),
    Block(Block),
}

pub enum Else {
    Block(Block),
    If(Box<Stmt>),
}

pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

pub enum ExprKind {
    Int(u128),
    Bool(bool),
    Str(Vec<StrSeg>),
    Ident(String),
    Call { callee: Box<Expr>, args: Vec<Expr> },
    Field { base: Box<Expr>, name: String },
    /// `Name{field: value, ...}`; fields are evaluated in the order written.
    StructLit { name: String, name_span: Span, fields: Vec<FieldInit> },
    /// `[a, b, c]`
    VecLit(Vec<Expr>),
    /// `base[index]`
    Index { base: Box<Expr>, index: Box<Expr> },
    Unary { op: UnOp, op_span: Span, expr: Box<Expr> },
    Binary { op: BinOp, op_span: Span, lhs: Box<Expr>, rhs: Box<Expr> },
    Paren(Box<Expr>),
}

pub enum StrSeg {
    Text(String),
    Expr { expr: Expr, spec: Option<String> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
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
