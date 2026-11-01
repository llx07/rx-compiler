use crate::{
    frontend::ast,
    ir::{FunctionId, IntPredicate, IrTy, Operand, Signature, Terminator},
    semantic::{Ty, TypeDef},
};

use super::{IrLowerer, expr::LoweredExpr};

impl IrLowerer<'_> {
    fn encode_equality_ty(&self, ty: &Ty) -> String {
        let (name, inner) = match ty {
            Ty::I32 => ("i32".into(), None),
            Ty::U32 => ("u32".into(), None),
            Ty::ISize => ("isize".into(), None),
            Ty::USize => ("usize".into(), None),
            Ty::Bool => ("bool".into(), None),
            Ty::Unit => ("unit".into(), None),
            Ty::Never => ("never".into(), None),
            Ty::Struct(id) => {
                let TypeDef::Struct(structure) = self.semantic.get_type_def(*id) else {
                    unreachable!("struct type must refer to a struct definition")
                };
                (structure.name.clone(), None)
            }
            Ty::Vec(inner) => ("Vec".into(), Some(inner.as_ref())),
            Ty::Box(inner) => ("Box".into(), Some(inner.as_ref())),
            Ty::Ref { is_mut, inner } => (
                if *is_mut { "refmut" } else { "ref" }.into(),
                Some(inner.as_ref()),
            ),
            Ty::Array { elem, len } => (format!("array{len}"), Some(elem.as_ref())),
        };
        let mut encoded = format!("ty_{}{name}", name.len());
        if let Some(inner) = inner {
            encoded.push_str(&self.encode_equality_ty(inner));
        }
        encoded
    }

    fn get_or_declare_equality(&mut self, ty: &Ty) -> FunctionId {
        if let Some(id) = self.equality_fns.get(ty) {
            return *id;
        }
        let name = format!("__rx_builtin_{}_eq", self.encode_equality_ty(ty));
        let id = self.builder.declare_function(
            name,
            Signature {
                params: vec![IrTy::Ptr, IrTy::Ptr],
                result: IrTy::I1,
            },
        );
        self.equality_fns.insert(ty.clone(), id);
        self.pending_equality.push((ty.clone(), id));
        id
    }

    pub(super) fn lower_equality_expr(
        &mut self,
        binary: &ast::BinaryExpr,
    ) -> Result<LoweredExpr, ()> {
        let lhs = self.lower_expr(&binary.lhs)?;
        let lhs = lhs.deref_all(self);
        let rhs = self.lower_expr(&binary.rhs)?;
        let rhs = rhs.deref_all(self);
        Ok(self.lower_equality(binary.op, lhs, rhs))
    }

    fn lower_equality(
        &mut self,
        op: ast::BinaryOp,
        lhs: LoweredExpr,
        rhs: LoweredExpr,
    ) -> LoweredExpr {
        let lhs = lhs.deref_all(self);
        let rhs = rhs.deref_all(self);
        let value = match lhs.ty() {
            Ty::I32 | Ty::U32 | Ty::ISize | Ty::USize | Ty::Bool => {
                let predicate = if op == ast::BinaryOp::Eq {
                    IntPredicate::Eq
                } else {
                    IntPredicate::Ne
                };
                let lhs = lhs.as_value(self);
                let rhs = rhs.as_value(self);
                self.builder.icmp(predicate, lhs, rhs)
            }
            Ty::Struct(_) | Ty::Array { .. } | Ty::Vec(_) => {
                let function = self.get_or_declare_equality(lhs.ty());
                let lhs = lhs.as_place(self);
                let rhs = rhs.as_place(self);
                let equal = self.builder.call(function, vec![lhs, rhs]);
                if op == ast::BinaryOp::Ne {
                    self.builder.not(equal)
                } else {
                    equal
                }
            }
            _ => unreachable!("unsupported equality operand"),
        };
        LoweredExpr::value(Ty::Bool, value)
    }

    pub(super) fn define_equalities(&mut self) {
        while let Some((ty, function_id)) = self.pending_equality.pop() {
            self.define_equality(&ty, &function_id);
        }
    }

    fn define_equality(&mut self, ty: &Ty, function_id: &FunctionId) {
        self.builder.define_function(*function_id);
        let mut cur = self.builder.create_block();
        let success = self.builder.create_block();
        let failed = self.builder.create_block();

        self.builder.position_at_end(success);
        self.builder
            .terminate(Terminator::Ret(Operand::const_i1(true)));

        self.builder.position_at_end(failed);
        self.builder
            .terminate(Terminator::Ret(Operand::const_i1(false)));

        self.builder.position_at_end(cur);

        match ty {
            Ty::Array { elem, .. } | Ty::Vec(elem) => {
                let align = self.align_of(&Ty::USize);

                let (lhs_data, rhs_data, len) = match ty {
                    Ty::Array { len, .. } => (
                        Operand::Parameter(0),
                        Operand::Parameter(1),
                        Operand::const_i32(*len as u32),
                    ),
                    Ty::Vec(_) => {
                        let ir_ty = self.lower_ty(ty.clone());
                        let [(lhs, len), (rhs, rhs_len)] =
                            [Operand::Parameter(0), Operand::Parameter(1)].map(|ptr| {
                                let data_ptr = self.builder.get_element_ptr(
                                    ir_ty.clone(),
                                    ptr,
                                    vec![Operand::const_i32(0), Operand::const_i32(0)],
                                );
                                let len_ptr = self.builder.get_element_ptr(
                                    ir_ty.clone(),
                                    ptr,
                                    vec![Operand::const_i32(0), Operand::const_i32(1)],
                                );
                                let data = self.builder.load(IrTy::Ptr, data_ptr, align);
                                let len = self.builder.load(IrTy::I32, len_ptr, align);
                                (data, len)
                            });
                        // comparing their length before
                        let equal = self.builder.icmp(IntPredicate::Eq, len, rhs_len);
                        let next = self.builder.create_block();
                        self.builder.terminate(Terminator::CondBr {
                            condition: equal,
                            then_block: next,
                            else_block: failed,
                        });
                        self.builder.position_at_end(next);
                        (lhs, rhs, len)
                    }
                    _ => unreachable!(),
                };

                let index_ptr = self.builder.alloca_in_entry(IrTy::I32, align);
                self.builder.store(Operand::const_i32(0), index_ptr, align);

                let condition = self.builder.create_block();
                let body = self.builder.create_block();

                self.builder.terminate(Terminator::Br(condition));

                self.builder.position_at_end(condition);
                let index = self.builder.load(IrTy::I32, index_ptr, align);
                let in_bounds = self.builder.icmp(IntPredicate::Ult, index, len);
                self.builder.terminate(Terminator::CondBr {
                    condition: in_bounds,
                    then_block: body,
                    else_block: success,
                });

                self.builder.position_at_end(body);
                let ir_ty = self.lower_ty(elem.as_ref().clone());
                let lhs = self
                    .builder
                    .get_element_ptr(ir_ty.clone(), lhs_data, vec![index]);
                let rhs = self.builder.get_element_ptr(ir_ty, rhs_data, vec![index]);
                let lhs = LoweredExpr::place(elem.as_ref().clone(), lhs);
                let rhs = LoweredExpr::place(elem.as_ref().clone(), rhs);
                let equal = self
                    .lower_equality(ast::BinaryOp::Eq, lhs, rhs)
                    .as_value(self);
                let index =
                    self.builder
                        .binary(crate::ir::BinaryOp::Add, index, Operand::const_i32(1));
                self.builder.store(index, index_ptr, align);
                self.builder.terminate(Terminator::CondBr {
                    condition: equal,
                    then_block: condition,
                    else_block: failed,
                });
            }
            Ty::Struct(type_id) => {
                let TypeDef::Struct(structure) = self.semantic.get_type_def(*type_id) else {
                    unreachable!("struct type must refer to a struct definition")
                };
                let fields = structure
                    .fields
                    .iter()
                    .map(|(_, field)| field.ty.as_ref().unwrap().clone())
                    .collect::<Vec<_>>();
                let ir_ty = self.lower_ty(ty.clone());
                for (index, field_ty) in fields.into_iter().enumerate() {
                    let indices = vec![Operand::const_i32(0), Operand::const_i32(index as u32)];
                    let lhs = self.builder.get_element_ptr(
                        ir_ty.clone(),
                        Operand::Parameter(0),
                        indices.clone(),
                    );
                    let rhs =
                        self.builder
                            .get_element_ptr(ir_ty.clone(), Operand::Parameter(1), indices);
                    let lhs = LoweredExpr::place(field_ty.clone(), lhs);
                    let rhs = LoweredExpr::place(field_ty, rhs);
                    let equal = self
                        .lower_equality(ast::BinaryOp::Eq, lhs, rhs)
                        .as_value(self);
                    let next = self.builder.create_block();
                    self.builder.terminate(Terminator::CondBr {
                        condition: equal,
                        then_block: next,
                        else_block: failed,
                    });
                    cur = next;
                    self.builder.position_at_end(cur);
                }
                self.builder.terminate(Terminator::Br(success));
            }
            _ => unreachable!("helper is only needed for aggregate types"),
        }
    }
}
