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
                let target = LoweredExpr::place(ty.clone(), cloned_ptr);

                for (index, field) in structure.fields.iter() {
                    let field_ty = field.ty.as_ref().unwrap();
                    let source = original.field(index.into(), field_ty.clone(), self);
                    let destination = target.field(index.into(), field_ty.clone(), self);
                    let field_value = self.lower_clone(source).as_value(self);
                    let field_ptr = destination.as_place(self);
                    self.builder.store(field_value, field_ptr);
                }

                self.builder.load(ir_ty, cloned_ptr)
            }
            Ty::Array { .. } => todo!(),
            Ty::Vec(inner_ty) => {
                let ir_ty = self.lower_ty(ty.clone());
                let cloned_ptr = self.builder.alloca_in_entry(ir_ty.clone());
                let elem_size = self.size_of(inner_ty);
                let len = original.field(1, Ty::USize, self).as_value(self);
                self.builder.call(
                    self.runtime_functions[&RuntimeFunction::VecNewWithSize],
                    vec![cloned_ptr, len, Operand::const_i32(elem_size)],
                );

                let index_ptr = self.builder.alloca_in_entry(IrTy::I32);
                self.builder.store(Operand::const_i32(0), index_ptr);

                let cond_block = self.builder.create_block();
                let clone_block = self.builder.create_block();
                let final_block = self.builder.create_block();
                self.builder.terminate(Terminator::Br(cond_block));

                self.builder.position_at_end(cond_block);
                let index = self.builder.load(IrTy::I32, index_ptr);
                let in_bounds = self.builder.icmp(crate::ir::IntPredicate::Ult, index, len);
                self.builder.terminate(Terminator::CondBr {
                    condition: in_bounds,
                    then_block: clone_block,
                    else_block: final_block,
                });

                self.builder.position_at_end(clone_block);
                let data_ty = Ty::Ref {
                    is_mut: false,
                    inner: inner_ty.clone(),
                };
                let source_data = original.field(0, data_ty.clone(), self).as_value(self);
                let target = LoweredExpr::place(ty.clone(), cloned_ptr);
                let target_data = target.field(0, data_ty, self).as_value(self);
                let elem_ir_ty = self.lower_ty(inner_ty.as_ref().clone());
                let source_ptr =
                    self.builder
                        .get_element_ptr(elem_ir_ty.clone(), source_data, vec![index]);
                let target_ptr = self
                    .builder
                    .get_element_ptr(elem_ir_ty, target_data, vec![index]);
                let source = LoweredExpr::place(inner_ty.as_ref().clone(), source_ptr);
                let cloned = self.lower_clone(source).as_value(self);
                self.builder.store(cloned, target_ptr);
                let next_index =
                    self.builder
                        .binary(crate::ir::BinaryOp::Add, index, Operand::const_i32(1));
                self.builder.store(next_index, index_ptr);
                self.builder.terminate(Terminator::Br(cond_block));

                self.builder.position_at_end(final_block);
                self.builder.load(ir_ty, cloned_ptr)
            }
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
