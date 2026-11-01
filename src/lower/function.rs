use crate::{
    frontend::ast::{self, CallExpr, Expr, UnaryOp},
    ir::{BinaryOp, CastOp, FunctionId, IntPredicate, IrTy, Operand, Signature, Terminator},
    semantic::{self, Adjustment, DefId, Ty, TypeDef, TypedConstKind},
};

use super::{IrLowerer, expr::LoweredExpr};

/// Encodes a source type name for use in associated-value symbols.
fn mangle_ty(name: &str) -> String {
    format!("__rx_ty_{}{name}", name.len())
}

/// Encodes a source value name, optionally qualified by its owning type.
fn mangle_value(name: &str, owner: Option<&str>) -> String {
    match owner {
        Some(owner) => format!("{}_val_{}{name}", mangle_ty(owner), name.len()),
        None => format!("__rx_val_{}{name}", name.len()),
    }
}

impl IrLowerer<'_> {
    pub(super) fn declare_function(&mut self, function: &ast::Function) {
        let def = self
            .semantic
            .node_def(function.id)
            .expect("function must be collected");
        let sig = def
            .as_function_def(self.semantic)
            .expect("function definition must exist")
            .sig
            .as_ref()
            .expect("function signature must be resolved");
        let owner = match def {
            DefId::GlobalValue(_) => None,
            DefId::AssociatedValue { owner, .. } => {
                let TypeDef::Struct(structure) = self.semantic.get_type_def(owner) else {
                    unreachable!("source associated function must belong to a struct")
                };
                Some(structure.name.as_str())
            }
            _ => unreachable!("function must be a global or associated value"),
        };
        let signature = Signature {
            params: sig
                .receiver
                .iter()
                .chain(&sig.params)
                .map(|ty| self.lower_ty(ty.clone()))
                .collect(),
            result: self.lower_ty(sig.result.clone()),
        };
        let id = self
            .builder
            .declare_function(mangle_value(&function.name.name, owner), signature);
        self.functions.insert(def, id);
        if owner.is_none() && function.name.name == "main" {
            self.main_func = Some(id);
        }
    }

    pub(super) fn lower_function(&mut self, function: &ast::Function, id: FunctionId) {
        self.locals.clear();
        self.builder.define_function(id);
        let _ = self.lower_body(function);
    }
}

impl IrLowerer<'_> {
    // Err(()) means control flow has already terminated.
    fn lower_body(&mut self, function: &ast::Function) -> Result<(), ()> {
        let entry = self.builder.create_block();
        self.builder.position_at_end(entry);

        self.bind_parameters(function);

        let value = self.lower_block(&function.body)?;
        let result = value.as_value(self);

        self.builder.terminate(Terminator::Ret(result));
        Ok(())
    }

    fn bind_parameters(&mut self, function: &ast::Function) {
        let params = function
            .receiver
            .iter()
            .map(|receiver| receiver.id)
            .chain(function.params.iter().map(|param| param.id));

        for (index, id) in params.enumerate() {
            let source_ty = self.semantic.node_ty(id).unwrap();
            let DefId::Local(local_id) = self.semantic.node_def(id).unwrap() else {
                unreachable!("param is local binding");
            };
            let align = self.align_of(source_ty);
            let ty = self.lower_ty(source_ty.clone());
            let ptr = self.builder.alloca_in_entry(ty, align);
            self.builder
                .store(Operand::Parameter(index as u32), ptr, align);
            self.locals
                .insert(local_id, LoweredExpr::place(source_ty.clone(), ptr));
        }
    }

    fn lower_stmt(&mut self, stmt: &ast::Stmt) -> Result<(), ()> {
        match &stmt.kind {
            ast::StmtKind::Expr(stmt) => self.lower_expr(&stmt.expr).map(|_| ()),
            ast::StmtKind::Let(stmt) => self.lower_let_stmt(stmt),
        }
    }

    fn lower_let_stmt(&mut self, stmt: &ast::LetStmt) -> Result<(), ()> {
        let ty = self.semantic.node_ty(stmt.id).unwrap();
        let value = self.lower_expr(&stmt.expr)?;
        let align = self.align_of(ty);
        let def_id = self.semantic.node_def(stmt.id).unwrap();
        let DefId::Local(local_id) = def_id else {
            unreachable!("let stmt is local binding");
        };
        let ir_ty = self.lower_ty(ty.clone());

        let ptr = self.builder.alloca_in_entry(ir_ty, align);

        debug_assert_eq!(value.ty(), ty);
        let operand = value.as_value(self);
        self.builder.store(operand, ptr, align);

        self.locals
            .insert(local_id, LoweredExpr::place(ty.clone(), ptr));

        Ok(())
    }

    pub(super) fn lower_expr(&mut self, expr: &ast::Expr) -> Result<LoweredExpr, ()> {
        let mut lowered = match &expr.kind {
            ast::ExprKind::Literal(_) => self.lower_literal(expr.id),
            ast::ExprKind::Path(_) => self.lower_path(expr.id),
            ast::ExprKind::Block(block) => self.lower_block(block),
            ast::ExprKind::Unary(unary) => self.lower_unary(expr.id, unary),
            ast::ExprKind::Binary(binary) => self.lower_binary(expr.id, binary),
            ast::ExprKind::Assign(assign) => self.lower_assign(assign),
            ast::ExprKind::Cast(cast) => self.lower_cast(expr.id, cast),
            ast::ExprKind::Unit => self.lower_unit(),
            ast::ExprKind::Array(array) => self.lower_array(expr.id, array),
            ast::ExprKind::Index(index) => self.lower_index(expr.id, index),
            ast::ExprKind::Struct(structure) => self.lower_struct_expr(expr.id, structure),
            ast::ExprKind::Call(call_expr) => self.lower_call(expr.id, call_expr),
            ast::ExprKind::MethodCall(call) => self.lower_method_call(expr.id, call),
            ast::ExprKind::Field(field) => self.lower_field(expr.id, field),
            ast::ExprKind::Loop(_) => self.lower_loop(expr.id),
            ast::ExprKind::Break(_) => self.lower_break(expr.id),
            ast::ExprKind::Continue => self.lower_continue(expr.id),
            ast::ExprKind::If(_) => self.lower_if(expr.id),
            ast::ExprKind::Return(return_value) => self.lower_return(return_value.as_deref()),
        }?;
        if let Some(adjustment) = self.semantic.node_adjustment(expr.id) {
            for step in &adjustment.steps {
                lowered = match step {
                    Adjustment::Borrow { is_mut } => lowered.borrow(*is_mut, self),
                    Adjustment::Deref => lowered.deref(self),
                    Adjustment::MaterializeTemporary => lowered.materialize(self),
                };
            }
        }
        Ok(lowered)
    }

    fn lower_literal(&mut self, id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        let ty = self.semantic.node_ty(id).unwrap().clone();
        let value = self.semantic.node_literal(id).unwrap();
        let constant = match ty {
            semantic::Ty::Bool => Operand::const_i1(value != 0),
            _ => Operand::const_i32(value as u32),
        };
        Ok(LoweredExpr::value(ty, constant))
    }

    fn lower_path(&mut self, id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        let def_id = self.semantic.node_def(id).unwrap();
        Ok(match def_id {
            DefId::AssociatedValue { .. } | DefId::GlobalValue(_) => {
                let def = def_id.as_value_def(self.semantic).unwrap();
                let value = self.semantic.node_const(def.ast_node_id.unwrap()).unwrap();
                let constant = match value.kind {
                    TypedConstKind::Bool => Operand::const_i1(value.value != 0),
                    _ => Operand::const_i32(value.value as u32),
                };
                LoweredExpr::value(
                    self.semantic
                        .node_ty(id)
                        .expect("expression must have a type")
                        .clone(),
                    constant,
                )
            }
            DefId::Local(local) => self
                .locals
                .get(&local)
                .expect("local must be bound")
                .clone(),
            _ => unreachable!("path must resolve to a value or local binding"),
        })
    }

    fn lower_block(&mut self, block: &ast::BlockExpr) -> Result<LoweredExpr, ()> {
        for stmt in &block.stmts {
            self.lower_stmt(stmt)?;
        }

        match &block.tail {
            Some(expr) => {
                let value = self.lower_expr(expr)?;
                Ok(LoweredExpr::value(value.ty().clone(), value.as_value(self)))
            }
            None => Ok(LoweredExpr::value(semantic::Ty::Unit, Operand::Void)),
        }
    }

    fn lower_unary(
        &mut self,
        id: ast::AstNodeId,
        unary: &ast::UnaryExpr,
    ) -> Result<LoweredExpr, ()> {
        let operand = self.lower_expr(&unary.operand)?;

        let lowered = match unary.op {
            UnaryOp::Neg | UnaryOp::Not => {
                let operand = if matches!(operand.ty(), semantic::Ty::Ref { .. }) {
                    operand.deref(self)
                } else {
                    operand
                };
                let operand_value = operand.as_value(self);
                let value = match unary.op {
                    UnaryOp::Neg => self.builder.neg(operand_value),
                    UnaryOp::Not => self.builder.not(operand_value),
                    _ => unreachable!(),
                };
                LoweredExpr::value(self.semantic.node_ty(id).unwrap().clone(), value)
            }
            UnaryOp::Deref => operand.deref(self),
            UnaryOp::Borrow | UnaryOp::BorrowMut => {
                operand.borrow(unary.op == UnaryOp::BorrowMut, self)
            }
        };

        Ok(lowered)
    }

    fn lower_binary(
        &mut self,
        id: ast::AstNodeId,
        binary: &ast::BinaryExpr,
    ) -> Result<LoweredExpr, ()> {
        use ast::BinaryOp as Op;

        match binary.op {
            Op::And | Op::Or => return self.lower_short_circuit(binary),
            Op::Eq | Op::Ne => return self.lower_equality_expr(binary),
            _ => {}
        }

        let ty = self.semantic.node_ty(id).unwrap().clone();
        let lhs = self.lower_expr(&binary.lhs)?;
        let mut lhs = LoweredExpr::value(lhs.ty().clone(), lhs.as_value(self));
        let rhs = self.lower_expr(&binary.rhs)?;
        let mut rhs = LoweredExpr::value(rhs.ty().clone(), rhs.as_value(self));

        while matches!(lhs.ty(), semantic::Ty::Ref { .. }) {
            lhs = lhs.deref(self);
        }
        while matches!(rhs.ty(), semantic::Ty::Ref { .. }) {
            rhs = rhs.deref(self);
        }

        let signed = matches!(lhs.ty(), semantic::Ty::I32 | semantic::Ty::ISize);

        let lhs = lhs.as_value(self);
        let rhs = rhs.as_value(self);
        let value = match binary.op {
            Op::Lt | Op::Le | Op::Gt | Op::Ge => self.lower_compare(binary.op, lhs, rhs, signed),
            _ => self.lower_binary_arithmetic(binary.op, lhs, rhs, signed),
        };
        Ok(LoweredExpr::value(ty, value))
    }

    fn lower_compare(
        &mut self,
        op: ast::BinaryOp,
        lhs: Operand,
        rhs: Operand,
        signed: bool,
    ) -> Operand {
        use ast::BinaryOp as Op;

        let predicate = match (op, signed) {
            (Op::Lt, true) => IntPredicate::Slt,
            (Op::Lt, false) => IntPredicate::Ult,
            (Op::Le, true) => IntPredicate::Sle,
            (Op::Le, false) => IntPredicate::Ule,
            (Op::Gt, true) => IntPredicate::Sgt,
            (Op::Gt, false) => IntPredicate::Ugt,
            (Op::Ge, true) => IntPredicate::Sge,
            (Op::Ge, false) => IntPredicate::Uge,
            _ => unreachable!(),
        };
        self.builder.icmp(predicate, lhs, rhs)
    }

    fn lower_binary_arithmetic(
        &mut self,
        op: ast::BinaryOp,
        lhs: Operand,
        mut rhs: Operand,
        signed: bool,
    ) -> Operand {
        use ast::BinaryOp as Op;

        if matches!(op, Op::Shl | Op::Shr) {
            rhs = self
                .builder
                .binary(BinaryOp::And, rhs, Operand::const_i32(31));
        }
        let op = match (op, signed) {
            (Op::Add, _) => BinaryOp::Add,
            (Op::Sub, _) => BinaryOp::Sub,
            (Op::Mul, _) => BinaryOp::Mul,
            (Op::Div, true) => BinaryOp::SDiv,
            (Op::Div, false) => BinaryOp::UDiv,
            (Op::Rem, true) => BinaryOp::SRem,
            (Op::Rem, false) => BinaryOp::URem,
            (Op::BitAnd, _) => BinaryOp::And,
            (Op::BitOr, _) => BinaryOp::Or,
            (Op::BitXor, _) => BinaryOp::Xor,
            (Op::Shl, _) => BinaryOp::Shl,
            (Op::Shr, true) => BinaryOp::AShr,
            (Op::Shr, false) => BinaryOp::LShr,
            _ => unreachable!(),
        };
        self.builder.binary(op, lhs, rhs)
    }

    fn lower_short_circuit(&mut self, binary: &ast::BinaryExpr) -> Result<LoweredExpr, ()> {
        let align = self.align_of(&Ty::Bool);

        let lhs = self.lower_expr(&binary.lhs)?;
        let result = self.builder.alloca_in_entry(self.lower_ty(Ty::Bool), align);
        let lhs_value = lhs.as_value(self);
        self.builder.store(lhs_value, result, align);

        let cond = match binary.op {
            ast::BinaryOp::And => self.builder.not(lhs_value),
            ast::BinaryOp::Or => lhs_value,
            _ => unreachable!("only short circuit operator"),
        };

        let rhs_block = self.builder.create_block();
        let merge_block = self.builder.create_block();

        self.builder.terminate(Terminator::CondBr {
            condition: cond,
            then_block: merge_block,
            else_block: rhs_block,
        });

        self.builder.position_at_end(rhs_block);
        if let Ok(rhs) = self.lower_expr(&binary.rhs) {
            let rhs_value = rhs.as_value(self);
            self.builder.store(rhs_value, result, align);
            self.builder.terminate(Terminator::Br(merge_block));
        }

        self.builder.position_at_end(merge_block);
        Ok(LoweredExpr::value(
            Ty::Bool,
            self.builder.load(IrTy::I1, result, align),
        ))
    }

    fn lower_assign(&mut self, assign: &ast::AssignExpr) -> Result<LoweredExpr, ()> {
        use ast::AssignOp as Op;

        let rhs = self.lower_expr(&assign.rhs)?;
        let rhs = LoweredExpr::value(rhs.ty().clone(), rhs.as_value(self));
        let lhs = self.lower_expr(&assign.lhs)?;
        let place = lhs.as_place(self);
        let align = self.align_of(lhs.ty());

        let result = match assign.op {
            Op::Assign => rhs.as_value(self),
            _ => {
                let op = match assign.op {
                    Op::AddAssign => ast::BinaryOp::Add,
                    Op::SubAssign => ast::BinaryOp::Sub,
                    Op::MulAssign => ast::BinaryOp::Mul,
                    Op::DivAssign => ast::BinaryOp::Div,
                    Op::RemAssign => ast::BinaryOp::Rem,
                    Op::BitAndAssign => ast::BinaryOp::BitAnd,
                    Op::BitOrAssign => ast::BinaryOp::BitOr,
                    Op::BitXorAssign => ast::BinaryOp::BitXor,
                    Op::ShlAssign => ast::BinaryOp::Shl,
                    Op::ShrAssign => ast::BinaryOp::Shr,
                    Op::Assign => unreachable!(),
                };
                let signed = matches!(lhs.ty(), Ty::I32 | Ty::ISize);
                let lhs = lhs.as_value(self);
                let rhs = if matches!(rhs.ty(), Ty::Ref { .. }) {
                    rhs.deref(self)
                } else {
                    rhs
                };
                let rhs = rhs.as_value(self);
                self.lower_binary_arithmetic(op, lhs, rhs, signed)
            }
        };

        self.builder.store(result, place, align);
        self.lower_unit()
    }

    fn lower_cast(&mut self, id: ast::AstNodeId, cast: &ast::CastExpr) -> Result<LoweredExpr, ()> {
        let operand = self.lower_expr(&cast.expr)?;
        let value = operand.as_value(self);
        let value = if matches!(operand.ty(), semantic::Ty::Bool) {
            self.builder.cast(CastOp::ZExt, value, IrTy::I32)
        } else {
            value
        };
        Ok(LoweredExpr::value(
            self.semantic.node_ty(id).unwrap().clone(),
            value,
        ))
    }

    fn lower_unit(&mut self) -> Result<LoweredExpr, ()> {
        Ok(LoweredExpr::value(semantic::Ty::Unit, Operand::Void))
    }

    fn lower_array(
        &mut self,
        id: ast::AstNodeId,
        array: &ast::ArrayExpr,
    ) -> Result<LoweredExpr, ()> {
        let ty = self.semantic.node_ty(id).unwrap().clone();
        let Ty::Array { elem, len } = &ty else {
            unreachable!("array expression must have an array type")
        };
        let ir_ty = self.lower_ty(ty.clone());
        let align = self.align_of(elem);
        let ptr = self.builder.alloca_in_entry(ir_ty.clone(), align);

        match array {
            ast::ArrayExpr::Elements(elements) => {
                for (index, element) in elements.iter().enumerate() {
                    let value = self.lower_expr(element)?.as_value(self);
                    let element_ptr = self.builder.get_element_ptr(
                        ir_ty.clone(),
                        ptr,
                        vec![Operand::const_i32(0), Operand::const_i32(index as u32)],
                    );
                    self.builder.store(value, element_ptr, align);
                }
            }
            ast::ArrayExpr::Repeat { value, .. } => {
                let value = self.lower_expr(value)?.as_value(self);
                if *len == 1 {
                    let element_ptr = self.builder.get_element_ptr(
                        ir_ty,
                        ptr,
                        vec![Operand::const_i32(0), Operand::const_i32(0)],
                    );
                    self.builder.store(value, element_ptr, align);
                } else {
                    let index_ptr = self.builder.alloca_in_entry(IrTy::I32, 4);
                    self.builder.store(Operand::const_i32(0), index_ptr, 4);
                    let condition = self.builder.create_block();
                    let body = self.builder.create_block();
                    let end = self.builder.create_block();
                    self.builder.terminate(Terminator::Br(condition));

                    self.builder.position_at_end(condition);
                    let index = self.builder.load(IrTy::I32, index_ptr, 4);
                    let in_bounds = self.builder.icmp(
                        IntPredicate::Ult,
                        index,
                        Operand::const_i32(*len as u32),
                    );
                    self.builder.terminate(Terminator::CondBr {
                        condition: in_bounds,
                        then_block: body,
                        else_block: end,
                    });

                    self.builder.position_at_end(body);
                    let element_ptr = self.builder.get_element_ptr(
                        ir_ty,
                        ptr,
                        vec![Operand::const_i32(0), index],
                    );
                    self.builder.store(value, element_ptr, align);
                    let next = self
                        .builder
                        .binary(BinaryOp::Add, index, Operand::const_i32(1));
                    self.builder.store(next, index_ptr, 4);
                    self.builder.terminate(Terminator::Br(condition));
                    self.builder.position_at_end(end);
                }
            }
        }

        Ok(LoweredExpr::place(ty, ptr))
    }

    fn lower_index(
        &mut self,
        id: ast::AstNodeId,
        expr: &ast::IndexExpr,
    ) -> Result<LoweredExpr, ()> {
        let base = self.lower_expr(&expr.base)?;
        let index = self.lower_expr(&expr.index)?.as_value(self);

        let ty = self.semantic.node_ty(id).unwrap().clone();
        let ir_ty = self.lower_ty(ty.clone());
        let base = match base.ty() {
            Ty::Array { .. } => base.as_place(self),
            Ty::Vec(_) => {
                let vec_ty = self.lower_ty(base.ty().clone());
                let align = self.align_of(base.ty());
                let base = base.as_place(self);
                let data_ptr = self.builder.get_element_ptr(
                    vec_ty,
                    base,
                    vec![Operand::const_i32(0), Operand::const_i32(0)],
                );
                self.builder.load(IrTy::Ptr, data_ptr, align)
            }
            _ => unreachable!("base of index must resolve to an array or Vec"),
        };
        let ptr = self.builder.get_element_ptr(ir_ty, base, vec![index]);
        Ok(LoweredExpr::place(ty, ptr))
    }

    fn lower_struct_expr(
        &mut self,
        id: ast::AstNodeId,
        structure: &ast::StructExpr,
    ) -> Result<LoweredExpr, ()> {
        let ty = self.semantic.node_ty(id).unwrap().clone();
        let ir_ty = self.lower_ty(ty.clone());
        let ptr = self
            .builder
            .alloca_in_entry(ir_ty.clone(), self.align_of(&ty));

        for initializer in &structure.fields {
            let DefId::Field { field, .. } = self.semantic.node_def(initializer.id).unwrap() else {
                unreachable!("struct initializer must resolve to a field")
            };
            let value = self.lower_expr(&initializer.value)?;
            let align = self.align_of(value.ty());
            let value = value.as_value(self);
            let field_ptr = self.builder.get_element_ptr(
                ir_ty.clone(),
                ptr,
                vec![Operand::const_i32(0), Operand::const_i32(field.into())],
            );
            self.builder.store(value, field_ptr, align);
        }

        Ok(LoweredExpr::place(ty, ptr))
    }

    fn lower_call(&mut self, id: ast::AstNodeId, call_expr: &CallExpr) -> Result<LoweredExpr, ()> {
        let target = self.semantic.node_call(id).unwrap();
        let function_id = match &target.target {
            semantic::CallTarget::Builtin {
                op:
                    op @ (semantic::BuiltinOp::GetI32
                    | semantic::BuiltinOp::PrintI32
                    | semantic::BuiltinOp::PrintlnI32),
                ..
            } => {
                let name = match op {
                    semantic::BuiltinOp::GetI32 => "get_i32",
                    semantic::BuiltinOp::PrintI32 => "print_i32",
                    semantic::BuiltinOp::PrintlnI32 => "println_i32",
                    _ => unreachable!(),
                };
                let value = self
                    .semantic
                    .lookup_value_def(name)
                    .expect("I/O builtin must be collected");
                self.functions[&DefId::GlobalValue(value)]
            }
            semantic::CallTarget::Builtin { op, type_arg } => todo!(),
            semantic::CallTarget::User(def) => self.functions[def],
        };

        let args = call_expr
            .args
            .iter()
            .map(|expr| self.lower_expr(expr).map(|value| value.as_value(self)))
            .collect::<Result<Vec<_>, _>>()?;

        let return_value = self.builder.call(function_id, args);

        Ok(LoweredExpr::value(
            self.semantic.node_ty(id).unwrap().clone(),
            return_value,
        ))
    }

    fn lower_method_call(
        &mut self,
        id: ast::AstNodeId,
        call: &ast::MethodCallExpr,
    ) -> Result<LoweredExpr, ()> {
        let target = self.semantic.node_call(id).unwrap();
        let function_id = match &target.target {
            semantic::CallTarget::User(def) => self.functions[def],
            semantic::CallTarget::Builtin { .. } => todo!(),
        };

        let args = std::iter::once(call.receiver.as_ref())
            .chain(&call.args)
            .map(|expr| self.lower_expr(expr).map(|value| value.as_value(self)))
            .collect::<Result<Vec<_>, _>>()?;
        let return_value = self.builder.call(function_id, args);

        Ok(LoweredExpr::value(
            self.semantic.node_ty(id).unwrap().clone(),
            return_value,
        ))
    }

    fn lower_field(
        &mut self,
        id: ast::AstNodeId,
        field: &ast::FieldExpr,
    ) -> Result<LoweredExpr, ()> {
        let base = self.lower_expr(&field.base)?;
        let DefId::Field { owner, field } = self.semantic.node_def(id).unwrap() else {
            unreachable!("field access must resolve to a field")
        };
        let ir_ty = self.lower_ty(Ty::Struct(owner));
        let base = base.as_place(self);
        let ptr = self.builder.get_element_ptr(
            ir_ty,
            base,
            vec![Operand::const_i32(0), Operand::const_i32(field.into())],
        );
        Ok(LoweredExpr::place(
            self.semantic.node_ty(id).unwrap().clone(),
            ptr,
        ))
    }

    fn lower_loop(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
    }

    fn lower_break(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
    }

    fn lower_continue(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
    }

    fn lower_if(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
    }

    fn lower_return(&mut self, return_value: Option<&Expr>) -> Result<LoweredExpr, ()> {
        let operand = return_value
            .map(|expr| self.lower_expr(expr).map(|value| value.as_value(self)))
            .transpose()?;
        self.builder
            .terminate(Terminator::Ret(operand.unwrap_or(Operand::Void)));
        Err(())
    }
}
