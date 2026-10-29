pub mod builder;
pub mod printer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstId(u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrTy {
    Void,
    I1,
    I8,
    I32,
    Ptr,
    Array { element: Box<IrTy>, len: u32 },
    Struct(StructId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub params: Vec<IrTy>,
    pub result: IrTy,
}

#[derive(Debug)]
pub struct Module {
    pub structs: Vec<StructDef>,
    pub functions: Vec<Function>,
}

#[derive(Debug)]
pub struct StructDef {
    pub name: String,
    pub fields: Option<Vec<IrTy>>,
}

#[derive(Debug)]
pub struct Function {
    pub name: String,
    pub signature: Signature,
    pub body: Option<FunctionBody>,
}

#[derive(Debug)]
pub struct FunctionBody {
    pub instructions: Vec<Instruction>,
    /// The first block is the function entry.
    pub blocks: Vec<BasicBlock>,
}

#[derive(Debug)]
pub struct BasicBlock {
    pub instructions: Vec<InstId>,
    pub terminator: Option<Terminator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constant {
    I1(bool),
    I8(u8),
    I32(u32),
    Null,
}

impl Constant {
    pub fn ty(&self) -> &'static IrTy {
        match self {
            Self::I1(_) => &IrTy::I1,
            Self::I8(_) => &IrTy::I8,
            Self::I32(_) => &IrTy::I32,
            Self::Null => &IrTy::Ptr,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    Parameter(u32),
    Instruction(InstId),
    Constant(Constant),
}

impl Operand {
    pub fn ty<'a>(&self, function: &'a Function) -> &'a IrTy {
        match self {
            Self::Parameter(index) => &function.signature.params[*index as usize],
            Self::Instruction(id) => function
                .body
                .as_ref()
                .expect("instruction operand must belong to a function body")
                .instructions[id.0 as usize]
                .result_ty()
                .expect("instruction operand must have a result"),
            Self::Constant(constant) => constant.ty(),
        }
    }
}

#[derive(Debug)]
pub struct Instruction {
    pub kind: InstructionKind,
    result_ty: Option<IrTy>,
}

#[derive(Debug)]
pub enum InstructionKind {
    Alloca {
        ty: IrTy,
        align: u32,
    },
    Load {
        ty: IrTy,
        ptr: Operand,
        align: u32,
    },
    Store {
        value: Operand,
        ptr: Operand,
        align: u32,
    },
    GetElementPtr {
        source_ty: IrTy,
        ptr: Operand,
        indices: Vec<Operand>,
    },
    Binary {
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
    },
    ICmp {
        predicate: IntPredicate,
        lhs: Operand,
        rhs: Operand,
    },
    Cast {
        op: CastOp,
        value: Operand,
        to: IrTy,
    },
    Call {
        function: FunctionId,
        args: Vec<Operand>,
    },
}

impl Instruction {
    pub fn result_ty(&self) -> Option<&IrTy> {
        self.result_ty.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    SDiv,
    UDiv,
    SRem,
    URem,
    And,
    Or,
    Xor,
    Shl,
    AShr,
    LShr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntPredicate {
    Eq,
    Ne,
    Slt,
    Sle,
    Sgt,
    Sge,
    Ult,
    Ule,
    Ugt,
    Uge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastOp {
    ZExt,
    Trunc,
}

#[derive(Debug)]
pub enum Terminator {
    Br(BlockId),
    CondBr {
        condition: Operand,
        then_block: BlockId,
        else_block: BlockId,
    },
    /// None for returning void
    Ret(Option<Operand>),
    Unreachable,
}
