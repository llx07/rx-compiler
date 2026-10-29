use crate::{
    frontend::ast,
    ir::{Constant, FunctionId, Operand, Signature, Terminator},
    semantic::{Adjustment, DefId, TypeDef, TypedConstKind},
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

        for stmt in &function.body.stmts {
            self.lower_stmt(stmt)?;
        }

        let result = match &function.body.tail {
            Some(expr) => {
                let value = self.lower_expr(expr)?;
                value.as_value(self)
            }
            None => Operand::Void,
        };

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
            ast::ExprKind::Path(_) => {
                let def_id = self.semantic.node_def(expr.id).unwrap();
                match def_id {
                    DefId::AssociatedValue { .. } | DefId::GlobalValue(_) => {
                        let def = def_id.as_value_def(self.semantic).unwrap();
                        let value = self.semantic.node_const(def.ast_node_id.unwrap()).unwrap();
                        let constant = match value.kind {
                            TypedConstKind::Bool => Constant::I1(value.value != 0),
                            _ => Constant::I32(value.value as u32),
                        };
                        LoweredExpr::value(
                            self.semantic
                                .node_ty(expr.id)
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
                }
            }
            _ => todo!(),
        };
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
}
