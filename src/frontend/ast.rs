use crate::diagnostic::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AstNodeId(u32);

impl From<u32> for AstNodeId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Debug)]
pub struct Crate {
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Debug)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Debug)]
pub enum ItemKind {
    Function(Function),
    Struct(Struct),
    Const(ConstItem),
    Impl(Impl),
}

#[derive(Debug)]
pub struct Function {
    pub id: AstNodeId,
    pub name: Ident,
    pub receiver: Option<Receiver>,
    pub params: Vec<Param>,
    pub return_ty: TyAst,
    pub body: BlockExpr,
    pub span: Span,
}

#[derive(Debug)]
pub struct Receiver {
    pub id: AstNodeId,
    pub is_ref: bool,
    pub is_mut: bool,
    pub span: Span,
}

#[derive(Debug)]
pub struct Param {
    pub id: AstNodeId,
    pub binding: Binding,
    pub ty: TyAst,
    pub span: Span,
}

#[derive(Debug)]
pub struct Binding {
    pub id: AstNodeId,
    pub is_mut: bool,
    pub name: Ident,
    pub span: Span,
}

#[derive(Debug)]
pub struct Struct {
    pub id: AstNodeId,
    pub name: Ident,
    pub derives: Vec<Derive>,
    pub fields: Vec<StructField>,
    pub span: Span,
}

#[derive(Debug)]
pub struct Derive {
    pub kind: DeriveKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum DeriveKind {
    Copy,
    Clone,
    PartialEq,
    Eq,
}

#[derive(Debug)]
pub struct StructField {
    pub id: AstNodeId,
    pub name: Ident,
    pub ty: TyAst,
    pub span: Span,
}

#[derive(Debug)]
pub struct ConstItem {
    pub id: AstNodeId,
    pub name: Ident,
    pub ty: TyAst,
    pub value: ConstValue,

    pub span: Span,
}

#[derive(Debug)]
pub struct ConstValue {
    pub id: AstNodeId,
    pub kind: ConstValueKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum ConstValueKind {
    Int(IntLiteral),
    Bool(bool),
    Path(Path),
    Neg(Box<ConstValue>),
}

#[derive(Debug)]
pub struct IntLiteral {
    pub raw: String, // raw does not contain suffix
    pub suffix: Option<IntSuffix>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum IntSuffix {
    I32,
    U32,
    ISize,
    USize,
}

#[derive(Debug)]
pub struct Impl {
    pub id: AstNodeId,
    pub ty: TyAst,
    pub items: Vec<AssociatedItem>,
    pub span: Span,
}

#[derive(Debug)]
pub struct AssociatedItem {
    pub kind: AssociatedItemKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum AssociatedItemKind {
    ConstItem(ConstItem),
    Function(Function),
}

#[derive(Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum StmtKind {
    Let(LetStmt),
    Expr(ExprStmt),
}

#[derive(Debug)]
pub struct LetStmt {
    pub id: AstNodeId,
    pub binding: Binding,
    pub ty: Option<TyAst>,
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug)]
pub struct ExprStmt {
    pub expr: Expr,
    pub has_semi: bool,
    pub span: Span,
}

#[derive(Debug)]
pub struct TyAst {
    pub id: AstNodeId,
    pub kind: TyAstKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum TyAstKind {
    Path(Path),
    Unit,
    Ref(RefTy),
    Array(ArrayTy),
}

#[derive(Debug)]
pub struct RefTy {
    pub is_mut: bool,
    pub ty: Box<TyAst>,
}

#[derive(Debug)]
pub struct ArrayTy {
    pub ty: Box<TyAst>,
    pub len: ConstValue,
}

#[derive(Debug)]
pub struct Path {
    pub id: AstNodeId,
    pub segments: Vec<PathSegment>,
    pub span: Span,
}

#[derive(Debug)]
pub struct PathSegment {
    pub id: AstNodeId,
    pub name: PathIdent,
    pub generic_args: Vec<TyAst>,
    pub span: Span,
}

#[derive(Debug)]
pub enum PathIdent {
    Ident(Ident),
    SelfValue, // self
    SelfType,  // Self
}

#[derive(Debug)]
pub struct Expr {
    pub id: AstNodeId,
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub enum LiteralExpr {
    Int(IntLiteral),
    Bool(bool),
}

#[derive(Debug)]
pub struct BlockExpr {
    pub id: AstNodeId,
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug)]
pub struct UnaryExpr {
    pub op: UnaryOp,
    pub operand: Box<Expr>,
}

#[derive(Debug)]
pub enum UnaryOp {
    Neg,
    Not,
    Borrow,
    BorrowMut,
    Deref,
}

#[derive(Debug)]
pub struct BinaryExpr {
    pub op: BinaryOp,
    pub lhs: Box<Expr>,
    pub rhs: Box<Expr>,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct AssignExpr {
    pub op: AssignOp,
    pub lhs: Box<Expr>,
    pub rhs: Box<Expr>,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct CastExpr {
    pub expr: Box<Expr>,
    pub ty: TyAst,
}

#[derive(Debug)]
pub enum ArrayExpr {
    Elements(Vec<Expr>),
    Repeat { value: Box<Expr>, len: ConstValue },
}

#[derive(Debug)]
pub struct IndexExpr {
    pub base: Box<Expr>,
    pub index: Box<Expr>,
}

#[derive(Debug)]
pub struct StructExpr {
    pub path: Path,
    pub fields: Vec<StructExprField>,
}

#[derive(Debug)]
pub struct StructExprField {
    pub id: AstNodeId,
    pub name: Ident,
    pub value: Expr,
}

#[derive(Debug)]
pub struct CallExpr {
    pub callee: Box<Expr>,
    pub args: Vec<Expr>,
}

#[derive(Debug)]
pub struct MethodCallExpr {
    pub receiver: Box<Expr>,
    pub method: PathSegment,
    pub args: Vec<Expr>,
}

#[derive(Debug)]
pub struct FieldExpr {
    pub base: Box<Expr>,
    pub field: Ident,
}

#[derive(Debug)]
pub enum LoopExpr {
    Loop {
        body: BlockExpr,
    },
    While {
        condition: Box<Expr>,
        body: BlockExpr,
    },
}

#[derive(Debug)]
pub struct IfExpr {
    pub condition: Box<Expr>,
    pub then_branch: BlockExpr,
    pub else_branch: Option<ElseBranch>,
}

#[derive(Debug)]
pub enum ElseBranch {
    BlockExpr(BlockExpr),
    IfExpr(Box<Expr>),
}
