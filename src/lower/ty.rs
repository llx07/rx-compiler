use crate::{
    ir::IrTy,
    semantic::{BuiltinType, Ty, TypeDef, TypeId},
};

use super::IrLowerer;

impl IrLowerer<'_> {
    pub(super) fn declare_types(&mut self) -> Vec<TypeId> {
        let mut type_ids = Vec::new();
        for (type_id, def) in self.semantic.iter_types() {
            if let TypeDef::Struct(structure) = def {
                let id = self.module.declare_struct(structure.name.clone());
                self.structs.insert(type_id, id);
                type_ids.push(type_id);
            } else if let TypeDef::Builtin(builtin) = def {
                if builtin.kind == BuiltinType::Vec {
                    let id = self.module.declare_struct("__rx_vec".into());
                    self.module
                        .define_struct(id, vec![IrTy::Ptr, IrTy::I32, IrTy::I32]);
                    self.vec_struct = Some(id);
                }
            }
        }
        type_ids
    }

    pub(super) fn lower_ty(&self, ty: Ty) -> IrTy {
        match ty {
            Ty::Vec(_) => IrTy::Struct(self.vec_struct.expect("Vec must be declared")),
            Ty::I32 | Ty::U32 | Ty::ISize | Ty::USize => IrTy::I32,
            Ty::Bool => IrTy::I1,
            Ty::Unit | Ty::Never => IrTy::Void,
            Ty::Struct(type_id) => {
                IrTy::Struct(*self.structs.get(&type_id).expect("all struct defined"))
            }
            Ty::Ref { .. } | Ty::Box(_) => IrTy::Ptr,
            Ty::Array { elem, len } => IrTy::Array {
                element: Box::new(self.lower_ty(*elem)),
                len: len as u32,
            },
        }
    }

    pub(super) fn lower_struct(&mut self, type_id: TypeId) {
        let type_def = self.semantic.get_type_def(type_id);
        let TypeDef::Struct(struct_def) = type_def else {
            unreachable!("type_id must be a struct")
        };

        let fields = struct_def
            .fields
            .iter()
            .map(|(_, field_def)| {
                self.lower_ty(
                    field_def
                        .ty
                        .as_ref()
                        .expect("ty of field should be collected")
                        .clone(),
                )
            })
            .collect::<Vec<_>>();

        let struct_id = self
            .structs
            .get(&type_id)
            .expect("struct should be defined");

        self.module.define_struct(*struct_id, fields);
    }
}
