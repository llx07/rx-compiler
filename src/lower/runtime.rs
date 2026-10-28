use crate::{
    ir::{self, IrTy, Operand, Signature},
    semantic::DefId,
};

use super::IrLowerer;

impl IrLowerer<'_> {
    pub(super) fn wrap_main(&mut self) {
        let source_main = self.main_func.expect("source main must be declared");
        let main_id = self.builder.declare_function(
            "main".into(),
            Signature {
                params: Vec::new(),
                result: IrTy::I32,
            },
        );
        self.builder.define_function(main_id);
        let entry = self.builder.create_block();
        self.builder.position_at_end(entry);
        self.builder.call(source_main, Vec::new());
        self.builder
            .terminate(ir::Terminator::Ret(Some(Operand::Constant(
                ir::Constant::I32(0),
            ))));
    }

    pub(super) fn declare_builtins(&mut self) {
        for (name, params, result) in [
            ("get_i32", Vec::new(), IrTy::I32),
            ("print_i32", vec![IrTy::I32], IrTy::Void),
            ("println_i32", vec![IrTy::I32], IrTy::Void),
        ] {
            let value = self
                .semantic
                .lookup_value_def(name)
                .expect("I/O builtin must be collected");
            let id = self
                .builder
                .declare_function(name.into(), Signature { params, result });
            self.functions.insert(DefId::GlobalValue(value), id);
        }

        // The allocator is a runtime helper, not a source-visible value.
        self.alloc_func = Some(self.builder.declare_function(
            "__rx_alloc".into(),
            Signature {
                params: vec![IrTy::I32, IrTy::I32],
                result: IrTy::Ptr,
            },
        ));
    }
}
