use crate::{ir::Operand, semantic::Ty};

use super::IrLowerer;

#[derive(Clone)]
pub(super) struct LoweredExpr {
    ty: Ty, // for place, Ty is the inner of ptr
    kind: LoweredExprKind,
    operand: Operand,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LoweredExprKind {
    Value,
    Place,
}

impl LoweredExpr {
    pub(super) fn value(ty: Ty, operand: Operand) -> Self {
        Self {
            ty,
            kind: LoweredExprKind::Value,
            operand,
        }
    }

    pub(super) fn place(ty: Ty, operand: Operand) -> Self {
        Self {
            ty,
            kind: LoweredExprKind::Place,
            operand,
        }
    }

    pub(super) fn ty(&self) -> &Ty {
        &self.ty
    }

    /// Returns the value operand, loading from storage if needed.
    pub(super) fn as_value(&self, lowerer: &mut IrLowerer<'_>) -> Operand {
        if self.kind == LoweredExprKind::Value {
            return self.operand;
        }
        let ty = lowerer.lower_ty(self.ty.clone());
        let align = lowerer.align_of(&self.ty);
        lowerer.builder.load(ty, self.operand, align)
    }

    /// Returns the storage address, materializing a value if needed.
    pub(super) fn as_place(&self, lowerer: &mut IrLowerer<'_>) -> Operand {
        match self.kind {
            LoweredExprKind::Place => self.operand,
            LoweredExprKind::Value => self.clone().materialize(lowerer).operand,
        }
    }

    pub(super) fn materialize(self, lowerer: &mut IrLowerer<'_>) -> Self {
        let operand = self.as_value(lowerer);
        let align = lowerer.align_of(&self.ty);
        let ty = lowerer.lower_ty(self.ty.clone());
        let ptr = lowerer.builder.alloca_in_entry(ty, align);
        lowerer.builder.store(operand, ptr, align);
        Self::place(self.ty, ptr)
    }

    pub(super) fn deref(self, lowerer: &mut IrLowerer<'_>) -> Self {
        let ptr = self.as_value(lowerer);
        let ty = match self.ty {
            Ty::Ref { inner, .. } | Ty::Box(inner) => *inner,
            _ => panic!("deref requires a reference or Box"),
        };
        Self::place(ty, ptr)
    }

    /// Dereferences all outer reference and Box layers.
    pub(super) fn deref_all(mut self, lowerer: &mut IrLowerer<'_>) -> Self {
        while matches!(self.ty, Ty::Ref { .. } | Ty::Box(_)) {
            self = self.deref(lowerer);
        }
        self
    }

    pub(super) fn borrow(self, is_mut: bool, lowerer: &mut IrLowerer<'_>) -> Self {
        let ptr = self.as_place(lowerer);
        Self::value(
            Ty::Ref {
                is_mut,
                inner: Box::new(self.ty),
            },
            ptr,
        )
    }
}
