use crate::{
    frontend::ast::Expr,
    ir::{IntPredicate, Operand},
    semantic::Ty,
};

use super::{IrLowerer, expr::LoweredExpr, runtime::RuntimeFunction};

impl IrLowerer<'_> {
    pub(super) fn lower_vec_new(&mut self, inner_ty: &Ty) -> Result<LoweredExpr, ()> {
        let ty = Ty::Vec(Box::new(inner_ty.clone()));
        let vec_ptr = self.builder.alloca_in_entry(self.lower_ty(ty.clone()));
        self.builder.call(
            self.runtime_functions[&RuntimeFunction::VecNew],
            vec![vec_ptr],
        );
        Ok(LoweredExpr::place(ty, vec_ptr))
    }

    pub(super) fn lower_vec_len(&mut self, receiver: &Expr) -> Result<LoweredExpr, ()> {
        let vec_ptr = self.lower_expr(receiver)?.deref_all(self).as_place(self);
        let len = self.builder.call(
            self.runtime_functions[&RuntimeFunction::VecLen],
            vec![vec_ptr],
        );
        Ok(LoweredExpr::value(Ty::USize, len))
    }

    pub(super) fn lower_vec_is_empty(&mut self, receiver: &Expr) -> Result<LoweredExpr, ()> {
        let vec_ptr = self.lower_expr(receiver)?.deref_all(self).as_place(self);
        let result = self.builder.call(
            self.runtime_functions[&RuntimeFunction::VecIsEmpty],
            vec![vec_ptr],
        );
        let is_empty = self
            .builder
            .icmp(IntPredicate::Ne, result, Operand::const_i32(0));
        Ok(LoweredExpr::value(Ty::Bool, is_empty))
    }

    pub(super) fn lower_vec_push(
        &mut self,
        inner_ty: &Ty,
        receiver: &Expr,
        value: &Expr,
    ) -> Result<LoweredExpr, ()> {
        let vec_ptr = self.lower_expr(receiver)?.deref_all(self).as_place(self);
        let value = self.lower_expr(value)?.as_place(self);
        self.builder.call(
            self.runtime_functions[&RuntimeFunction::VecPush],
            vec![vec_ptr, value, Operand::const_i32(self.size_of(inner_ty))],
        );
        Ok(LoweredExpr::value(Ty::Unit, Operand::Void))
    }

    pub(super) fn lower_vec_remove(
        &mut self,
        inner_ty: &Ty,
        receiver: &Expr,
        index: &Expr,
    ) -> Result<LoweredExpr, ()> {
        let vec_ptr = self.lower_expr(receiver)?.deref_all(self).as_place(self);
        let index = self.lower_expr(index)?.as_value(self);
        let result = self
            .builder
            .alloca_in_entry(self.lower_ty(inner_ty.clone()));
        self.builder.call(
            self.runtime_functions[&RuntimeFunction::VecRemove],
            vec![
                vec_ptr,
                index,
                result,
                Operand::const_i32(self.size_of(inner_ty)),
            ],
        );
        Ok(LoweredExpr::place(inner_ty.clone(), result))
    }
}
