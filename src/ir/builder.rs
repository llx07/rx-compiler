use crate::ir::BasicBlock;

use super::{
    BinaryOp, BlockId, CastOp, Function, FunctionBody, FunctionId, InstId, Instruction,
    IntPredicate, IrTy, Module, Operand, Signature, StructDef, StructId, Terminator,
};

pub struct ModuleBuilder {
    module: Module,
}

impl ModuleBuilder {
    pub fn new() -> Self {
        Self {
            module: Module {
                structs: Vec::new(),
                functions: Vec::new(),
            },
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

    pub fn define_function(&mut self, id: FunctionId) -> FunctionBuilder<'_> {
        self.module.functions[id.0 as usize].body = Some(FunctionBody {
            instructions: Vec::new(),
            blocks: Vec::new(),
        });
        FunctionBuilder {
            module: &mut self.module,
            function: id,
            current_block_id: None,
        }
    }

    pub fn finish(self) -> Module {
        self.module
    }
}

pub struct FunctionBuilder<'a> {
    module: &'a mut Module,
    function: FunctionId,
    current_block_id: Option<BlockId>,
}

impl FunctionBuilder<'_> {
    fn body(&self) -> &FunctionBody {
        self.module.functions[self.function.0 as usize]
            .body
            .as_ref()
            .expect("function builder must have a body")
    }

    fn body_mut(&mut self) -> &mut FunctionBody {
        self.module.functions[self.function.0 as usize]
            .body
            .as_mut()
            .expect("function builder must have a body")
    }

    fn append_instruction(&mut self, inst: Instruction) -> InstId {
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
        Operand::Instruction(self.append_instruction(Instruction::Alloca { ty, align }))
    }

    pub fn load(&mut self, ty: IrTy, ptr: Operand, align: u32) -> Operand {
        Operand::Instruction(self.append_instruction(Instruction::Load { ty, ptr, align }))
    }

    pub fn store(&mut self, value: Operand, ptr: Operand, align: u32) {
        self.append_instruction(Instruction::Store { value, ptr, align });
    }

    pub fn get_element_ptr(
        &mut self,
        source_ty: IrTy,
        ptr: Operand,
        indices: Vec<Operand>,
    ) -> Operand {
        Operand::Instruction(self.append_instruction(Instruction::GetElementPtr {
            source_ty,
            ptr,
            indices,
        }))
    }

    pub fn binary(&mut self, op: BinaryOp, lhs: Operand, rhs: Operand) -> Operand {
        Operand::Instruction(self.append_instruction(Instruction::Binary { op, lhs, rhs }))
    }

    pub fn icmp(&mut self, predicate: IntPredicate, lhs: Operand, rhs: Operand) -> Operand {
        Operand::Instruction(self.append_instruction(Instruction::ICmp {
            predicate,
            lhs,
            rhs,
        }))
    }

    pub fn cast(&mut self, op: CastOp, value: Operand, to: IrTy) -> Operand {
        Operand::Instruction(self.append_instruction(Instruction::Cast { op, value, to }))
    }

    pub fn call(&mut self, function: FunctionId, args: Vec<Operand>) -> Option<Operand> {
        let has_result = self.module.functions[function.0 as usize].signature.result != IrTy::Void;
        let id = self.append_instruction(Instruction::Call { function, args });
        has_result.then_some(Operand::Instruction(id))
    }

    pub fn terminate(&mut self, terminator: Terminator) {
        let block_id = self.current_block_id.expect("no insertion block selected");
        let body = self.body_mut();
        let block = body
            .blocks
            .get_mut(block_id.0 as usize)
            .expect("insertion block must exist");
        block.terminator = Some(terminator);
    }
}
