use crate::ir::BasicBlock;

use super::{
    BinaryOp, BlockId, CastOp, Constant, Function, FunctionBody, FunctionId, InstId, Instruction,
    InstructionKind, IntPredicate, IrTy, Module, Operand, Signature, StructDef, StructId,
    Terminator,
};

pub struct Builder {
    module: Module,
    current_function: Option<FunctionId>,
    current_block_id: Option<BlockId>,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            module: Module {
                structs: Vec::new(),
                functions: Vec::new(),
            },
            current_function: None,
            current_block_id: None,
        }
    }

    pub fn declare_struct(&mut self, name: String) -> StructId {
        let id = StructId(self.module.structs.len() as u32);
        self.module.structs.push(StructDef { name, fields: None });
        id
    }

    pub fn define_struct(&mut self, id: StructId, fields: Vec<IrTy>) {
        self.module.structs[id.0 as usize].fields = Some(fields);
    }

    pub fn declare_function(&mut self, name: String, signature: Signature) -> FunctionId {
        let id = FunctionId(self.module.functions.len() as u32);
        self.module.functions.push(Function {
            name,
            signature,
            body: None,
        });
        id
    }

    pub fn define_function(&mut self, id: FunctionId) {
        self.module.functions[id.0 as usize].body = Some(FunctionBody {
            instructions: Vec::new(),
            blocks: Vec::new(),
        });
        self.current_function = Some(id);
        self.current_block_id = None;
    }

    pub fn finish(self) -> Module {
        self.module
    }
    fn body(&self) -> &FunctionBody {
        self.module.functions[self.current_function.expect("no function selected").0 as usize]
            .body
            .as_ref()
            .expect("function builder must have a body")
    }

    fn body_mut(&mut self) -> &mut FunctionBody {
        self.module.functions[self.current_function.expect("no function selected").0 as usize]
            .body
            .as_mut()
            .expect("function builder must have a body")
    }

    fn build_instruction(&self, kind: InstructionKind) -> Instruction {
        let result_ty = match &kind {
            InstructionKind::Alloca { .. } | InstructionKind::GetElementPtr { .. } => {
                Some(IrTy::Ptr)
            }
            InstructionKind::Load { ty, .. } => Some(ty.clone()),
            InstructionKind::Store { .. } => None,
            InstructionKind::Binary { lhs, rhs, .. } => {
                let function = &self.module.functions
                    [self.current_function.expect("no function selected").0 as usize];
                let ty = lhs.ty(function);
                assert_eq!(ty, rhs.ty(function), "binary operand types must match");
                Some(ty.clone())
            }
            InstructionKind::ICmp { .. } => Some(IrTy::I1),
            InstructionKind::Cast { to, .. } => Some(to.clone()),
            InstructionKind::Call { function, .. } => {
                let ty = &self.module.functions[function.0 as usize].signature.result;
                (*ty != IrTy::Void).then(|| ty.clone())
            }
        };
        Instruction { kind, result_ty }
    }

    fn append_instruction(&mut self, kind: InstructionKind) -> InstId {
        let inst = self.build_instruction(kind);
        let block_id = self.current_block_id.expect("no insertion block selected");
        let body = self.body_mut();
        let block = body
            .blocks
            .get_mut(block_id.0 as usize)
            .expect("insertion block must exist");
        assert!(
            block.terminator.is_none(),
            "cannot append an instruction to a terminated block"
        );
        let id = InstId(body.instructions.len() as u32);
        body.instructions.push(inst);
        block.instructions.push(id);
        id
    }

    pub fn create_block(&mut self) -> BlockId {
        let blocks = &mut self.body_mut().blocks;
        let id = BlockId(blocks.len() as u32);
        blocks.push(BasicBlock {
            instructions: Vec::new(),
            terminator: None,
        });
        id
    }

    pub fn position_at_end(&mut self, block: BlockId) {
        self.current_block_id = Some(block);
    }

    pub fn alloca(&mut self, ty: IrTy, align: u32) -> Operand {
        Operand::Instruction(self.append_instruction(InstructionKind::Alloca { ty, align }))
    }

    pub fn alloca_in_entry(&mut self, ty: IrTy, align: u32) -> Operand {
        let inst = self.build_instruction(InstructionKind::Alloca { ty, align });
        let body = self.body_mut();
        let entry = body.blocks.first_mut().expect("entry block must exist");
        let id = InstId(body.instructions.len() as u32);
        body.instructions.push(inst);
        entry.instructions.insert(0, id);
        Operand::Instruction(id)
    }

    pub fn load(&mut self, ty: IrTy, ptr: Operand, align: u32) -> Operand {
        Operand::Instruction(self.append_instruction(InstructionKind::Load { ty, ptr, align }))
    }

    pub fn store(&mut self, value: Operand, ptr: Operand, align: u32) {
        self.append_instruction(InstructionKind::Store { value, ptr, align });
    }

    pub fn get_element_ptr(
        &mut self,
        source_ty: IrTy,
        ptr: Operand,
        indices: Vec<Operand>,
    ) -> Operand {
        Operand::Instruction(self.append_instruction(InstructionKind::GetElementPtr {
            source_ty,
            ptr,
            indices,
        }))
    }

    pub fn binary(&mut self, op: BinaryOp, lhs: Operand, rhs: Operand) -> Operand {
        Operand::Instruction(self.append_instruction(InstructionKind::Binary { op, lhs, rhs }))
    }

    pub fn neg(&mut self, value: Operand) -> Operand {
        self.binary(BinaryOp::Sub, Operand::Constant(Constant::I32(0)), value)
    }

    pub fn not(&mut self, value: Operand) -> Operand {
        let function = &self.module.functions
            [self.current_function.expect("no function selected").0 as usize];
        let mask = match value.ty(function) {
            IrTy::I1 => Constant::I1(true),
            IrTy::I8 => Constant::I8(u8::MAX),
            IrTy::I32 => Constant::I32(u32::MAX),
            _ => unreachable!("not requires an integer operand"),
        };
        self.binary(BinaryOp::Xor, value, Operand::Constant(mask))
    }

    pub fn icmp(&mut self, predicate: IntPredicate, lhs: Operand, rhs: Operand) -> Operand {
        Operand::Instruction(self.append_instruction(InstructionKind::ICmp {
            predicate,
            lhs,
            rhs,
        }))
    }

    pub fn cast(&mut self, op: CastOp, value: Operand, to: IrTy) -> Operand {
        Operand::Instruction(self.append_instruction(InstructionKind::Cast { op, value, to }))
    }

    pub fn call(&mut self, function: FunctionId, args: Vec<Operand>) -> Operand {
        let id = self.append_instruction(InstructionKind::Call { function, args });
        self.body().instructions[id.0 as usize]
            .result_ty()
            .map_or(Operand::Void, |_| Operand::Instruction(id))
    }

    pub fn terminate(&mut self, terminator: Terminator) {
        let block_id = self.current_block_id.expect("no insertion block selected");
        let body = self.body_mut();
        let block = body
            .blocks
            .get_mut(block_id.0 as usize)
            .expect("insertion block must exist");
        debug_assert!(block.terminator.is_none(), "block is already terminated");
        block.terminator = Some(terminator);
    }
}
