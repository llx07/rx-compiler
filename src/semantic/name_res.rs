use std::collections::HashMap;

use crate::{
    diagnostic::Diagnostic,
    frontend::ast::{ConstValueKind, Crate, ExprKind, Path, PathIdent, PathSegment, TyAstKind},
    semantic::{
        DefId, LocalDef, LocalDefKind, LocalId, SemanticCtx, TypeId,
        visitor::{
            Visitor, walk_block, walk_const_value, walk_expr, walk_function, walk_let_stmt,
            walk_struct, walk_ty,
        },
    },
};

struct NameResolution<'a> {
    ctx: &'a mut SemanticCtx,
    diagnostics: &'a mut Vec<Diagnostic>,

    scopes: Vec<HashMap<String, LocalId>>,

    current_impl_target: Option<TypeId>,
    current_self_ty: Option<TypeId>,
}

impl<'a> Visitor for NameResolution<'a> {
    fn visit_struct(&mut self, node: &crate::frontend::ast::Struct) {
        let DefId::Type(ty) = self
            .ctx
            .node_def(node.id)
            .expect("struct must be bound in def_collect")
        else {
            unreachable!("binding of struct must be type");
        };

        self.current_self_ty = Some(ty);
        walk_struct(self, node);
        self.current_self_ty = None;
    }

    fn visit_impl(&mut self, node: &crate::frontend::ast::Impl) {
        let DefId::Type(target) = self
            .ctx
            .node_def(node.id)
            .expect("impl must be bound in def_collect")
        else {
            unreachable!("binding of impl must be type");
        };

        self.current_impl_target = Some(target);
        self.current_self_ty = self.current_impl_target;
        // not visiting target to prevent resolving target again
        for item in &node.items {
            self.visit_associated_item(item);
        }
        self.current_self_ty = None;
        self.current_impl_target = None;
    }

    fn visit_function(&mut self, node: &crate::frontend::ast::Function) {
        self.scopes.push(HashMap::new());
        if let Some(receiver) = &node.receiver {
            if let Some(target) = self.current_impl_target {
                let self_id = self.ctx.alloc_local(LocalDef {
                    name: "self".to_string(),
                    ast_node_id: receiver.id,
                    span: receiver.span.clone(),
                    is_mut: receiver.is_mut && !receiver.is_ref,
                    kind: LocalDefKind::Receiver { ty: target },
                });

                self.ctx.record_node_def(receiver.id, DefId::Local(self_id));
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert("self".to_string(), self_id);
            } else {
                self.diagnostics.push(Diagnostic::new(
                    "receiver is only allowed in associated function",
                    receiver.span.clone(),
                ));
            }
        }
        for param in &node.params {
            let name = &param.binding.name.name;

            if self.scopes.last().unwrap().contains_key(name) {
                self.diagnostics.push(Diagnostic::new(
                    "duplicate parameter",
                    param.binding.name.span.clone(),
                ));
                continue;
            }

            let param_id = self.ctx.alloc_local(LocalDef {
                name: name.clone(),
                ast_node_id: param.id,
                span: param.span.clone(),
                is_mut: param.binding.is_mut,
                kind: LocalDefKind::Param,
            });
            self.ctx.record_node_def(param.id, DefId::Local(param_id));

            self.scopes
                .last_mut()
                .expect("scopes is not empty")
                .insert(name.clone(), param_id);
        }

        walk_function(self, node);
        self.scopes.pop();
    }

    fn visit_block(&mut self, node: &crate::frontend::ast::BlockExpr) {
        self.scopes.push(HashMap::new());
        walk_block(self, node);
        self.scopes.pop();
    }

    fn visit_let_stmt(&mut self, node: &crate::frontend::ast::LetStmt) {
        walk_let_stmt(self, node);
        let let_id = self.ctx.alloc_local(LocalDef {
            name: node.binding.name.name.clone(),
            ast_node_id: node.id,
            span: node.span.clone(),
            is_mut: node.binding.is_mut,
            kind: LocalDefKind::Let,
        });
        self.ctx.record_node_def(node.id, DefId::Local(let_id));

        self.scopes
            .last_mut()
            .expect("scopes is not empty")
            .insert(node.binding.name.name.clone(), let_id);
    }

    fn visit_const_value(&mut self, node: &crate::frontend::ast::ConstValue) {
        if let ConstValueKind::Path(path) = &node.kind {
            let def_id = match self.resolve_expr_path(path) {
                Ok(Some(def_id)) => def_id,
                Ok(None) => {
                    self.diagnostics
                        .push(Diagnostic::new("Unknown const", node.span.clone()));
                    return;
                }
                Err(diag) => {
                    self.diagnostics.push(diag);
                    return;
                }
            };

            if !def_id
                .as_value_def(self.ctx)
                .is_some_and(|def| def.is_const())
            {
                self.diagnostics
                    .push(Diagnostic::new("Not a const value", node.span.clone()));
                return;
            }

            self.ctx.record_node_def(node.id, def_id);
        }
        walk_const_value(self, node);
    }

    fn visit_expr(&mut self, node: &crate::frontend::ast::Expr) {
        if let ExprKind::Path(path) = &node.kind {
            match self.resolve_expr_path(path) {
                Ok(id) => {
                    if let Some(id) = id {
                        self.ctx.record_node_def(node.id, id);
                    }
                }
                Err(diag) => {
                    self.diagnostics.push(diag);
                }
            }
        }
        if let ExprKind::Struct(structure) = &node.kind {
            match self.resolve_type_path(&structure.path) {
                Ok(id) => {
                    self.ctx.record_node_def(structure.path.id, id);
                }
                Err(diag) => {
                    self.diagnostics.push(diag);
                }
            }
        }
        walk_expr(self, node);
    }

    fn visit_ty(&mut self, node: &crate::frontend::ast::TyAst) {
        if let TyAstKind::Path(path) = &node.kind {
            match self.resolve_type_path(path) {
                Ok(id) => {
                    self.ctx.record_node_def(node.id, id);
                }
                Err(diag) => {
                    self.diagnostics.push(diag);
                }
            }
        }
        walk_ty(self, node);
    }
}

impl<'a> NameResolution<'a> {
    fn resolve_type_path(&self, path: &Path) -> Result<DefId, Diagnostic> {
        let [segment] = path.segments.as_slice() else {
            return Err(Diagnostic::new("invalid type path", path.span.clone()));
        };
        self.resolve_type_segment(segment)
    }

    fn resolve_type_segment(&self, segment: &PathSegment) -> Result<DefId, Diagnostic> {
        match &segment.name {
            PathIdent::SelfValue => Err(Diagnostic::new(
                "self is not a type name, consider Self",
                segment.span.clone(),
            )),
            PathIdent::SelfType => {
                if let Some(ty) = self.current_self_ty {
                    Ok(DefId::Type(ty))
                } else {
                    Err(Diagnostic::new(
                        "Using Self outside impl or struct",
                        segment.span.clone(),
                    ))
                }
            }
            PathIdent::Ident(ident) => {
                if let Some(ty) = self.ctx.lookup_type_def(&ident.name) {
                    Ok(DefId::Type(ty))
                } else {
                    Err(Diagnostic::new("Unknown type name", segment.span.clone()))
                }
            }
        }
    }

    fn resolve_expr_path(&mut self, path: &Path) -> Result<Option<DefId>, Diagnostic> {
        match path.segments.as_slice() {
            [segment] => self.resolve_unqualified_value(segment),

            [ty_segment, member_segment] => {
                self.resolve_associated_value(ty_segment, member_segment)
            }

            _ => Err(Diagnostic::new(
                "invalid expression path",
                path.span.clone(),
            )),
        }
    }

    fn resolve_unqualified_value(
        &self,
        path_seg: &PathSegment,
    ) -> Result<Option<DefId>, Diagnostic> {
        let name = match &path_seg.name {
            PathIdent::Ident(ident) => &ident.name,
            PathIdent::SelfValue => &"self".to_string(),
            PathIdent::SelfType => {
                return Err(Diagnostic::new(
                    "Self is not a valid value",
                    path_seg.span.clone(),
                ));
            }
        };

        let first_found = self
            .scopes
            .iter()
            .rev()
            .map(|scope| scope.get(name))
            .find(|id| id.is_some());

        if let Some(found) = first_found {
            Ok(Some(DefId::Local(*found.expect("found is some"))))
        } else {
            // lookup in global namespace
            let global_result = self.ctx.lookup_value_def(name);
            if let Some(found) = global_result {
                Ok(Some(DefId::GlobalValue(found)))
            } else {
                Err(Diagnostic::new("name not found", path_seg.span.clone()))
            }
        }
    }

    fn resolve_associated_value(
        &mut self,
        ty_seg: &PathSegment,
        mem_seg: &PathSegment,
    ) -> Result<Option<DefId>, Diagnostic> {
        let DefId::Type(ty) = self.resolve_type_segment(ty_seg)? else {
            unreachable!("must be type");
        };
        self.ctx.record_node_def(ty_seg.id, DefId::Type(ty));

        let ty_def = self.ctx.get_type_def(ty);
        let name = match &mem_seg.name {
            PathIdent::Ident(ident) => ident.name.as_str(),
            _ => {
                return Err(Diagnostic::new(
                    "not a valid name of associated item",
                    mem_seg.span.clone(),
                ));
            }
        };

        if let Some(id) = ty_def.associated_values().lookup(name) {
            Ok(Some(DefId::AssociatedValue {
                owner: ty,
                value: id,
            }))
        } else if name == "clone" {
            Ok(None)
        } else {
            Err(Diagnostic::new("name not found", mem_seg.span.clone()))
        }
    }
}

pub fn name_resolve(ast: &Crate, ctx: &mut SemanticCtx, diagnostics: &mut Vec<Diagnostic>) {
    NameResolution {
        ctx,
        diagnostics,

        scopes: Vec::new(),
        current_impl_target: None,
        current_self_ty: None,
    }
    .visit_crate(ast);
}
