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

    pub(super) fn as_value(&self, lowerer: &mut IrLowerer<'_>) -> Operand {
        if self.kind == LoweredExprKind::Value {
            return self.operand;
        }
        let ty = lowerer.lower_ty(self.ty.clone());
        let align = lowerer.align_of(&self.ty);
        lowerer.builder.load(ty, self.operand, align)
    }

    pub(super) fn as_place(&self) -> Operand {
        assert_eq!(
            self.kind,
            LoweredExprKind::Place,
            "expected an existing place"
        );
        self.operand
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

    pub(super) fn borrow(self, is_mut: bool) -> Self {
        let ptr = self.as_place();
        Self::value(
            Ty::Ref {
                is_mut,
                inner: Box::new(self.ty),
            },
            ptr,
        )
    }
}
