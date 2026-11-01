use crate::{frontend::ast::Expr, ir::Operand, semantic::Ty};

use super::{IrLowerer, expr::LoweredExpr};

impl IrLowerer<'_> {
    pub(super) fn lower_box_new(&mut self, inner_ty: &Ty, value: &Expr) -> Result<LoweredExpr, ()> {
        let value = self.lower_expr(value)?.as_value(self);
        let ptr = self.builder.call(
            self.alloc_func.unwrap(),
            vec![
                Operand::const_i32(self.size_of(inner_ty)),
                Operand::const_i32(self.align_of(inner_ty)),
            ],
        );
        self.builder.store(value, ptr);

        Ok(LoweredExpr::value(Ty::Box(Box::new(inner_ty.clone())), ptr))
    }
}
