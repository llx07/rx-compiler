use super::{IrLowerer, expr::LoweredExpr};

impl IrLowerer<'_> {
    pub(super) fn lower_clone(&mut self, _value: LoweredExpr) -> LoweredExpr {
        todo!()
    }
}
