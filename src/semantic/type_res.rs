use std::collections::HashMap;

use crate::{
    diagnostic::Diagnostic,
    frontend::ast::TyAstKind,
    semantic::{
        ctx::{BuiltinType, DefId, SemanticCtx, Ty, TypeDef, TypedConst},
        visitor::Visitor,
    },
};

#[derive(Debug, PartialEq, Eq)]
enum ConstState {
    Visiting,
    Done(TypedConst),
    Failed,
}

pub struct TypeResolver<'a> {
    ctx: &'a mut SemanticCtx,
    diagnostics: &'a mut Vec<Diagnostic>,

    const_states: HashMap<DefId, ConstState>,
}

impl Visitor for TypeResolver<'_> {
    fn visit_ty(&mut self, node: &crate::frontend::ast::TyAst) {
        if let Err(diag) = self.resolve_ty(node) {
            self.diagnostics.push(diag);
        }
    }
}

impl TypeResolver<'_> {
    fn resolve_ty(&mut self, node: &crate::frontend::ast::TyAst) -> Result<Ty, Diagnostic> {
        let ty = match &node.kind {
            TyAstKind::Unit => Ty::Unit,
            TyAstKind::Ref(reference) => {
                let inner = self.resolve_ty(&reference.ty)?;
                Ty::Ref {
                    is_mut: reference.is_mut,
                    inner: Box::new(inner),
                }
            }
            TyAstKind::Array(array) => {
                let elem = self.resolve_ty(&array.ty)?;
                let TypedConst::Usize(len) = self.resolve_const(&array.len, &Ty::USize)? else {
                    unreachable!()
                };
                Ty::Array {
                    elem: Box::new(elem),
                    len,
                }
            }
            TyAstKind::Path(path) => {
                let [segment] = path.segments.as_slice() else {
                    return Err(Diagnostic::new("Unknown type", path.span.clone()));
                };
                let DefId::Type(type_id) = self
                    .ctx
                    .binding(node.id)
                    .ok_or_else(|| Diagnostic::new("Unknown type", path.span.clone()))?
                else {
                    return Err(Diagnostic::new("Unknown type", path.span.clone()));
                };

                let builtin = match self.ctx.get_type(type_id) {
                    TypeDef::Builtin { kind, .. } => Some(*kind),
                    TypeDef::Struct { .. } => None,
                };
                let expected_args = match builtin {
                    Some(kind) => kind.type_arg_count(),
                    None => 0,
                };
                if segment.generic_args.len() != expected_args {
                    return Err(Diagnostic::new(
                        format!("Type requires {expected_args} type argument(s)"),
                        segment.span.clone(),
                    ));
                }

                match builtin {
                    Some(BuiltinType::I32) => Ty::I32,
                    Some(BuiltinType::U32) => Ty::U32,
                    Some(BuiltinType::ISize) => Ty::ISize,
                    Some(BuiltinType::USize) => Ty::USize,
                    Some(BuiltinType::Bool) => Ty::Bool,
                    Some(BuiltinType::Box) => {
                        Ty::Box(Box::new(self.resolve_ty(&segment.generic_args[0])?))
                    }
                    Some(BuiltinType::Vec) => {
                        Ty::Vec(Box::new(self.resolve_ty(&segment.generic_args[0])?))
                    }
                    None => Ty::Struct(type_id),
                }
            }
        };
        Ok(ty)
    }

    fn resolve_const(
        &mut self,
        node: &crate::frontend::ast::ConstValue,
        expected: &Ty,
    ) -> Result<TypedConst, Diagnostic> {
        todo!()
    }

    fn check_recursive_structs(&mut self) {
        todo!()
    }
}
