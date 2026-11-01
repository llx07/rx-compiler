use crate::{
    ir::{self, IrTy, Operand, Signature},
    semantic::DefId,
};

use super::IrLowerer;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum RuntimeFunction {
    Alloc,
    Memcpy,
    Memmove,
    Memset,
    VecNew,
    VecLen,
    VecIsEmpty,
    VecPush,
    VecRemove,
}

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
            .terminate(ir::Terminator::Ret(Operand::const_i32(0)));
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

        // Runtime helpers are not source-visible values.
        for (function, name, params, result) in [
            (
                RuntimeFunction::Alloc,
                "__rx_alloc",
                vec![IrTy::I32, IrTy::I32],
                IrTy::Ptr,
            ),
            (
                RuntimeFunction::Memcpy,
                "memcpy",
                vec![IrTy::Ptr, IrTy::Ptr, IrTy::I32],
                IrTy::Ptr,
            ),
            (
                RuntimeFunction::Memmove,
                "memmove",
                vec![IrTy::Ptr, IrTy::Ptr, IrTy::I32],
                IrTy::Ptr,
            ),
            (
                RuntimeFunction::Memset,
                "memset",
                vec![IrTy::Ptr, IrTy::I32, IrTy::I32],
                IrTy::Ptr,
            ),
            (
                RuntimeFunction::VecNew,
                "__rx_vec_new",
                vec![IrTy::Ptr],
                IrTy::Void,
            ),
            (
                RuntimeFunction::VecLen,
                "__rx_vec_len",
                vec![IrTy::Ptr],
                IrTy::I32,
            ),
            (
                RuntimeFunction::VecIsEmpty,
                "__rx_vec_is_empty",
                vec![IrTy::Ptr],
                IrTy::I32,
            ),
            (
                RuntimeFunction::VecPush,
                "__rx_vec_push",
                vec![IrTy::Ptr, IrTy::Ptr, IrTy::I32],
                IrTy::Void,
            ),
            (
                RuntimeFunction::VecRemove,
                "__rx_vec_remove",
                vec![IrTy::Ptr, IrTy::I32, IrTy::Ptr, IrTy::I32],
                IrTy::Void,
            ),
        ] {
            let id = self
                .builder
                .declare_function(name.into(), Signature { params, result });
            self.runtime_functions.insert(function, id);
        }
    }
}
