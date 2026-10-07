use crate::{
    diagnostic::Diagnostic,
    frontend::ast::{AssociatedItemKind, AstNodeId, Crate, ItemKind, PathIdent, TyAst, TyAstKind},
    semantic::{
        ctx::{DefId, FieldDef, Namespace, SemanticCtx, TypeDef, TypeId, ValueDef, ValueDefKind},
        visitor::{Visitor, walk_item},
    },
};

pub struct DefCollector<'a> {
    ctx: &'a mut SemanticCtx,
    diagnostics: &'a mut Vec<Diagnostic>,

    current_item_id: Option<AstNodeId>,
}

impl<'a> Visitor for DefCollector<'a> {
    fn visit_item(&mut self, node: &crate::frontend::ast::Item) {
        self.current_item_id = Some(node.id);
        walk_item(self, node);
        self.current_item_id = None;
    }

    fn visit_function(&mut self, node: &crate::frontend::ast::Function) {
        let name = &node.name.name;
        let ast_node_id = self.current_item_id.expect("function must be in an item");

        match self.ctx.define_value(
            name.clone(),
            ValueDef {
                name: name.clone(),
                ast_node_id: Some(ast_node_id),
                kind: ValueDefKind::Function,
            },
        ) {
            Ok(id) => {
                self.ctx.bind(ast_node_id, DefId::GlobalValue(id));
            }
            Err(_) => self.diagnostics.push(Diagnostic {
                message: format!("Duplicate definition of {}", name),
                span: node.name.span.clone(),
            }),
        }
    }

    fn visit_const_item(&mut self, node: &crate::frontend::ast::ConstItem) {
        let name = &node.name.name;
        let ast_node_id = self.current_item_id.expect("const item must be in an item");

        match self.ctx.define_value(
            name.clone(),
            ValueDef {
                name: name.clone(),
                ast_node_id: Some(ast_node_id),
                kind: ValueDefKind::Const,
            },
        ) {
            Ok(id) => {
                self.ctx.bind(ast_node_id, DefId::GlobalValue(id));
            }
            Err(_) => self.diagnostics.push(Diagnostic {
                message: format!("Duplicate definition of {}", name),
                span: node.name.span.clone(),
            }),
        }
    }

    fn visit_struct(&mut self, node: &crate::frontend::ast::Struct) {
        let name = &node.name.name;
        let ast_node_id = self.current_item_id.expect("struct must be in an item");
        let mut structure = TypeDef {
            name: name.clone(),
            ast_node_id: Some(ast_node_id),
            fields: Namespace::new(),
            associated_values: Namespace::new(),
        };

        for field in &node.fields {
            let name = &field.name.name;
            if structure
                .fields
                .define(
                    name.clone(),
                    FieldDef {
                        name: name.clone(),
                        ast_node_id: field.id,
                    },
                )
                .is_err()
            {
                self.diagnostics.push(Diagnostic::new(
                    format!("Duplicate field definition of {name}"),
                    field.name.span.clone(),
                ));
            }
        }
        match self.ctx.define_type(name.clone(), structure) {
            Ok(id) => {
                self.ctx.bind(ast_node_id, DefId::Type(id));
            }
            Err(_) => self.diagnostics.push(Diagnostic {
                message: format!("Duplicate definition of {}", name),
                span: node.name.span.clone(),
            }),
        }
    }

    fn visit_impl(&mut self, _: &crate::frontend::ast::Impl) {
        // do nothing to skip associated items
    }
}

pub fn def_collect(ast: &Crate, ctx: &mut SemanticCtx, diagnostics: &mut Vec<Diagnostic>) {
    DefCollector {
        ctx,
        diagnostics,
        current_item_id: None,
    }
    .visit_crate(ast);

    if diagnostics.is_empty() {
        collect_impls(ast, ctx, diagnostics);
    }
}

fn collect_impls(ast: &Crate, ctx: &mut SemanticCtx, diagnostics: &mut Vec<Diagnostic>) {
    for item in &ast.items {
        let ItemKind::Impl(implementation) = &item.kind else {
            continue;
        };
        let target = match resolve_impl_target(&implementation.ty, ctx) {
            Ok(target) => target,
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                continue;
            }
        };
        ctx.bind(implementation.ty.id, DefId::Type(target));

        for item in &implementation.items {
            let (name, kind) = match &item.kind {
                AssociatedItemKind::Function(function) => (&function.name, ValueDefKind::Function),
                AssociatedItemKind::ConstItem(constant) => (&constant.name, ValueDefKind::Const),
            };

            let result = ctx.get_type_mut(target).associated_values.define(
                name.name.clone(),
                ValueDef {
                    name: name.name.clone(),
                    ast_node_id: Some(item.id),
                    kind,
                },
            );

            match result {
                Ok(id) => {
                    ctx.bind(
                        item.id,
                        DefId::AssociatedValue {
                            owner: target,
                            value: id,
                        },
                    );
                }
                Err(_) => {
                    diagnostics.push(Diagnostic::new(
                        format!("Duplicate associated value definition of {}", name.name),
                        name.span.clone(),
                    ));
                }
            }
        }
    }
}

fn resolve_impl_target(ty: &TyAst, ctx: &SemanticCtx) -> Result<TypeId, Diagnostic> {
    let TyAstKind::Path(path) = &ty.kind else {
        return Err(Diagnostic::new(
            "Impl target must be a user struct name",
            ty.span.clone(),
        ));
    };
    let [segment] = path.segments.as_slice() else {
        return Err(Diagnostic::new("Invalid type path", ty.span.clone()));
    };
    let PathIdent::Ident(name) = &segment.name else {
        return Err(Diagnostic::new(
            "Impl target must be a user struct name",
            ty.span.clone(),
        ));
    };
    if !segment.generic_args.is_empty() {
        return Err(Diagnostic::new(
            "Impl target must not have generic arg",
            ty.span.clone(),
        ));
    }

    let id = ctx.lookup_type(&name.name).ok_or_else(|| {
        Diagnostic::new(
            format!("Unknown impl target {}", name.name),
            name.span.clone(),
        )
    })?;

    if ctx.get_type(id).is_builtin() {
        return Err(Diagnostic::new(
            "Impl target must be a user struct name",
            ty.span.clone(),
        ));
    }

    Ok(id)
}
