use crate::diagnostic::Span;

pub struct Crate {
    pub items: Vec<Item>,
}

pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
}

pub struct Ident {
    pub name: String,
    pub span: Span,
}

pub enum ItemKind {
    Function(Function),
    Struct(Struct),
    Const(ConstItem),
    Impl(Impl),
}

pub struct Function {
    pub name: Ident,
    pub receiver: Option<Receiver>,
    pub params: Vec<Param>,
    pub return_ty: TyAst,
    pub body: BlockExpr,
    pub span: Span,
}

pub struct Receiver {
    pub is_ref: bool,
    pub is_mut: bool,
    pub span: Span,
}

pub struct Param {
    pub binding: Binding,
    pub ty: TyAst,
    pub span: Span,
}

pub struct Binding {
    pub is_mut: bool,
    pub name: Ident,
    pub span: Span,
}

pub struct Struct {
    pub name: Ident,
    pub derives: Vec<Derive>,
    pub fields: Vec<StructField>,
    pub span: Span,
}

pub struct Derive {
    pub kind: DeriveKind,
    pub span: Span,
}
pub enum DeriveKind {
    Copy,
    Clone,
    PartialEq,
    Eq,
}

pub struct StructField {
    pub name: Ident,
    pub ty: TyAst,
    pub span: Span,
}

pub struct ConstItem {
    pub name: Ident,
    pub ty: TyAst,
    pub value: ConstValue,

    pub span: Span,
}

pub struct ConstValue {
    pub kind: ConstValueKind,
    pub span: Span,
}

pub enum ConstValueKind {
    Int(IntLiteral),
    Bool(bool),
    Path(Path),
    Neg(Box<ConstValue>),
}

pub struct IntLiteral {
    pub raw: String, // raw does not contain suffix
    pub suffix: Option<IntSuffix>,
}

pub enum IntSuffix {
    I32,
    U32,
    ISize,
    USize,
}

pub struct Impl {
    pub ty: TyAst,
    pub items: Vec<AssociatedItem>,
    pub span: Span,
}

pub struct AssociatedItem {
    pub kind: AssociatedItemKind,
    pub span: Span,
}

pub enum AssociatedItemKind {
    ConstItem(ConstItem),
    Function(Function),
}

pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

pub enum StmtKind {
    Let(LetStmt),
    Expr(ExprStmt),
}

pub struct LetStmt {
    pub binding: Binding,
    pub ty: Option<TyAst>,
    pub expr: Expr,
    pub span: Span,
}

pub struct ExprStmt {
    pub expr: Expr,
    pub has_semi: bool,
    pub span: Span,
}

pub struct TyAst {
    pub kind: TyAstKind,
    pub span: Span,
}

pub enum TyAstKind {
    Path(Path),
    Unit,
    Ref(RefTy),
    Array(ArrayTy),
}

pub struct RefTy {
    pub is_mut: bool,
    pub ty: Box<TyAst>,
}

pub struct ArrayTy {
    pub ty: Box<TyAst>,
    pub len: ConstValue,
}

pub struct Path {
    pub segments: Vec<PathSegment>,
    pub span: Span,
}

pub struct PathSegment {
    pub name: PathIdent,
    pub generic_args: Vec<TyAst>,
    pub span: Span,
}

pub enum PathIdent {
    Ident(Ident),
    SelfValue, // self
    SelfType,  // Self
}

pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

pub enum ExprKind {
    Literal(LiteralExpr),
    Path(Path),
    Block(BlockExpr),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    Assign(AssignExpr),
    Cast(CastExpr),
    Unit,
    Array(ArrayExpr),
    Index(IndexExpr),
    Struct(StructExpr),
    Call(CallExpr),
    MethodCall(MethodCallExpr),
    Field(FieldExpr),
    Loop(LoopExpr),
    Break(Option<Box<Expr>>),
    Continue,
    If(IfExpr),
    Return(Option<Box<Expr>>),
}

pub enum LiteralExpr {
    Int(IntLiteral),
    Bool(bool),
}

pub struct BlockExpr {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

pub struct UnaryExpr {
    pub op: UnaryOp,
    pub operand: Box<Expr>,
}

pub enum UnaryOp {
    Neg,
    Not,
    Borrow,
    BorrowMut,
    Deref,
}

pub struct BinaryExpr {
    pub op: BinaryOp,
    pub lhs: Box<Expr>,
    pub rhs: Box<Expr>,
}

pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,

    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,

    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    And,
    Or,
}

pub struct AssignExpr {
    pub op: AssignOp,
    pub lhs: Box<Expr>,
    pub rhs: Box<Expr>,
}

pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    RemAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
    ShlAssign,
    ShrAssign,
}

pub struct CastExpr {
    pub expr: Box<Expr>,
    pub ty: TyAst,
}

pub enum ArrayExpr {
    Elements(Vec<Expr>),
    Repeat { value: Box<Expr>, len: ConstValue },
}

pub struct IndexExpr {
    pub base: Box<Expr>,
    pub index: Box<Expr>,
}

pub struct StructExpr {
    pub path: Path,
    pub fields: Vec<StructExprField>,
}
pub struct StructExprField {
    pub name: Ident,
    pub value: Expr,
}

pub struct CallExpr {
    pub callee: Box<Expr>,
    pub args: Vec<Expr>,
}

pub struct MethodCallExpr {
    pub receiver: Box<Expr>,
    pub method: PathSegment,
    pub args: Vec<Expr>,
}

pub struct FieldExpr {
    pub base: Box<Expr>,
    pub field: Ident,
}

pub enum LoopExpr {
    Loop {
        body: BlockExpr,
    },
    While {
        condition: Box<Expr>,
        body: BlockExpr,
    },
}

pub struct IfExpr {
    pub condition: Box<Expr>,
    pub then_branch: BlockExpr,
    pub else_branch: Option<ElseBranch>,
    pub span: Span,
}

pub enum ElseBranch {
    BlockExpr(BlockExpr),
    IfExpr(Box<IfExpr>),
}
