use std::ops::Deref;

use crate::{
    ir::{FunctionId, IrTy, Operand, Signature, Terminator},
    lower::RuntimeFunction,
    semantic::{Ty, TypeDef},
};

use super::{IrLowerer, expr::LoweredExpr};

impl IrLowerer<'_> {
    fn get_or_declare_clone(&mut self, ty: &Ty) -> FunctionId {
        if let Some(id) = self.clone_fns.get(ty) {
            return *id;
        }
        let name = format!("__rx_builtin_{}_clone", self.mangle_ty(ty));
        let id = self.builder.declare_function(
            name,
            Signature {
                params: vec![IrTy::Ptr],
                result: self.lower_ty(ty.clone()),
            },
        );
        self.clone_fns.insert(ty.clone(), id);
        self.pending_clone.push((ty.clone(), id));
        id
    }

    // value must be a place
    pub(super) fn lower_clone(&mut self, value: LoweredExpr) -> LoweredExpr {
        let ty = value.ty();

        let cloned = match ty {
            Ty::I32 | Ty::U32 | Ty::ISize | Ty::USize | Ty::Bool | Ty::Ref { .. } => {
                value.as_value(self)
            }
            Ty::Struct(_) | Ty::Array { .. } | Ty::Vec(_) | Ty::Box(_) => {
                let function = self.get_or_declare_clone(ty);
                let ptr = value.as_place(self);
                self.builder.call(function, vec![ptr])
            }
            Ty::Unit | Ty::Never => unreachable!("invalid clone type"),
        };

        LoweredExpr::value(value.ty().clone(), cloned)
    }

    pub(super) fn define_clones(&mut self) {
        while let Some((ty, function_id)) = self.pending_clone.pop() {
            self.define_clone(&ty, &function_id);
        }
    }

    fn define_clone(&mut self, ty: &Ty, function_id: &FunctionId) {
        self.builder.define_function(*function_id);
        let entry = self.builder.create_block();
        self.builder.position_at_end(entry);

        let original = LoweredExpr::place(ty.clone(), crate::ir::Operand::Parameter(0));

        let result = match ty {
            Ty::Struct(type_id) => {
                let ir_ty = self.lower_ty(ty.clone());
                let cloned_ptr = self.builder.alloca_in_entry(ir_ty.clone());

                let TypeDef::Struct(structure) = self.semantic.get_type_def(*type_id) else {
                    unreachable!("struct type must refer to a struct definition")
                };
                let original = original.as_place(self);

                for (index, field) in structure.fields.iter() {
                    let source = self.builder.get_element_ptr(
                        ir_ty.clone(),
                        original,
                        vec![Operand::const_i32(0), Operand::const_i32(index.into())],
                    );
                    let source_place =
                        LoweredExpr::place(field.ty.as_ref().unwrap().clone(), source);
                    let field_ptr = self.builder.get_element_ptr(
                        ir_ty.clone(),
                        cloned_ptr,
                        vec![Operand::const_i32(0), Operand::const_i32(index.into())],
                    );
                    let field_value = self.lower_clone(source_place).as_value(self);
                    self.builder.store(field_value, field_ptr);
                }

                self.builder.load(ir_ty, cloned_ptr)
            }
            Ty::Array { .. } => todo!(),
            Ty::Vec(_) => todo!(),
            Ty::Box(inner_ty) => {
                let source = original.deref(self);
                let cloned = self.lower_clone(source).as_value(self);

                let result = self.builder.call(
                    self.runtime_functions[&RuntimeFunction::Alloc],
                    vec![
                        Operand::const_i32(self.size_of(inner_ty)),
                        Operand::const_i32(self.align_of(inner_ty)),
                    ],
                );
                self.builder.store(cloned, result);
                result
            }
            _ => unreachable!("scalar and references should not generate helper"),
        };

        self.builder.terminate(Terminator::Ret(result));
    }
}
