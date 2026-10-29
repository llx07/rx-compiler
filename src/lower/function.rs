use crate::{
    frontend::ast::{self, CallExpr, Expr},
    ir::{Constant, FunctionId, Operand, Signature, Terminator},
    semantic::{self, Adjustment, DefId, TypeDef, TypedConstKind},
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

    fn lower_expr(&mut self, expr: &ast::Expr) -> Result<LoweredExpr, ()> {
        let mut lowered = match &expr.kind {
            ast::ExprKind::Literal(_) => self.lower_literal(expr.id),
            ast::ExprKind::Path(_) => self.lower_path(expr.id),
            ast::ExprKind::Block(block) => self.lower_block(block),
            ast::ExprKind::Unary(_) => self.lower_unary(expr.id),
            ast::ExprKind::Binary(_) => self.lower_binary(expr.id),
            ast::ExprKind::Assign(_) => self.lower_assign(expr.id),
            ast::ExprKind::Cast(_) => self.lower_cast(expr.id),
            ast::ExprKind::Unit => self.lower_unit(),
            ast::ExprKind::Array(_) => self.lower_array(expr.id),
            ast::ExprKind::Index(_) => self.lower_index(expr.id),
            ast::ExprKind::Struct(_) => self.lower_struct_expr(expr.id),
            ast::ExprKind::Call(call_expr) => self.lower_call(expr.id, call_expr),
            ast::ExprKind::MethodCall(_) => self.lower_method_call(expr.id),
            ast::ExprKind::Field(_) => self.lower_field(expr.id),
            ast::ExprKind::Loop(_) => self.lower_loop(expr.id),
            ast::ExprKind::Break(_) => self.lower_break(expr.id),
            ast::ExprKind::Continue => self.lower_continue(expr.id),
            ast::ExprKind::If(_) => self.lower_if(expr.id),
            ast::ExprKind::Return(return_value) => self.lower_return(return_value.as_deref()),
        }?;
        if let Some(adjustment) = self.semantic.node_adjustment(expr.id) {
            for step in &adjustment.steps {
                lowered = match step {
                    Adjustment::Borrow { is_mut } => lowered.borrow(*is_mut),
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
            semantic::Ty::Bool => Constant::I1(value != 0),
            _ => Constant::I32(value as u32),
        };
        Ok(LoweredExpr::value(ty, Operand::Constant(constant)))
    }

    fn lower_path(&mut self, id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        let def_id = self.semantic.node_def(id).unwrap();
        Ok(match def_id {
            DefId::AssociatedValue { .. } | DefId::GlobalValue(_) => {
                let def = def_id.as_value_def(self.semantic).unwrap();
                let value = self.semantic.node_const(def.ast_node_id.unwrap()).unwrap();
                let constant = match value.kind {
                    TypedConstKind::Bool => Constant::I1(value.value != 0),
                    _ => Constant::I32(value.value as u32),
                };
                LoweredExpr::value(
                    self.semantic
                        .node_ty(id)
                        .expect("expression must have a type")
                        .clone(),
                    Operand::Constant(constant),
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

    fn lower_unary(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
    }

    fn lower_binary(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
    }

    fn lower_assign(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
    }

    fn lower_cast(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
    }

    fn lower_unit(&mut self) -> Result<LoweredExpr, ()> {
        Ok(LoweredExpr::value(semantic::Ty::Unit, Operand::Void))
    }

    fn lower_array(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
    }

    fn lower_index(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
    }

    fn lower_struct_expr(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
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

    fn lower_method_call(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
    }

    fn lower_field(&mut self, _id: ast::AstNodeId) -> Result<LoweredExpr, ()> {
        todo!()
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
