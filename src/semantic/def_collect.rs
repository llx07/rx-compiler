use crate::{
    diagnostic::Diagnostic,
    frontend::ast::{AssociatedItemKind, Crate, ItemKind, PathIdent, TyAst, TyAstKind},
    semantic::{
        ctx::{
            DefId, FieldDef, FunctionDef, Namespace, SemanticCtx, StructDef, TraitSet, TypeDef,
            TypeId, ValueDef, ValueDefKind,
        },
        visitor::Visitor,
    },
};

pub struct DefCollector<'a> {
    ctx: &'a mut SemanticCtx,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl<'a> Visitor for DefCollector<'a> {
    fn visit_function(&mut self, node: &crate::frontend::ast::Function) {
        let name = &node.name.name;

        match self.ctx.define_value(
            name.clone(),
            ValueDef {
                name: name.clone(),
                ast_node_id: Some(node.id),
                span: Some(node.span.clone()),
                kind: ValueDefKind::Function(FunctionDef { sig: None }),
            },
        ) {
            Ok(id) => {
                self.ctx.record_node_def(node.id, DefId::GlobalValue(id));
            }
            Err(_) => self.diagnostics.push(Diagnostic {
                message: format!("Duplicate definition of {}", name),
                span: node.name.span.clone(),
            }),
        }
    }

    fn visit_const_item(&mut self, node: &crate::frontend::ast::ConstItem) {
        let name = &node.name.name;

        match self.ctx.define_value(
            name.clone(),
            ValueDef {
                name: name.clone(),
                ast_node_id: Some(node.id),
                span: Some(node.span.clone()),
                kind: ValueDefKind::Const,
            },
        ) {
            Ok(id) => {
                self.ctx.record_node_def(node.id, DefId::GlobalValue(id));
            }
            Err(_) => self.diagnostics.push(Diagnostic {
                message: format!("Duplicate definition of {}", name),
                span: node.name.span.clone(),
            }),
        }
    }

    fn visit_struct(&mut self, node: &crate::frontend::ast::Struct) {
        let name = &node.name.name;
        let mut fields = Namespace::new();

        for field in &node.fields {
            let name = &field.name.name;
            if fields
                .define(
                    name.clone(),
                    FieldDef {
                        name: name.clone(),
                        ast_node_id: field.id,
                        span: field.span.clone(),
                        ty: None,
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

        let mut traits = TraitSet::new();
        for derive in &node.derives {
            if traits.contains(derive.kind) {
                self.diagnostics.push(Diagnostic::new(
                    format!("Duplicate derive {:?} for struct '{name}'", derive.kind),
                    derive.span.clone(),
                ));
            }
            traits.set(derive.kind);
        }

        let structure = TypeDef::Struct(StructDef {
            name: name.clone(),
            ast_node_id: node.id,
            span: node.span.clone(),
            fields,
            traits,
            associated_values: Namespace::new(),
        });
        match self.ctx.define_type(name.clone(), structure) {
            Ok(id) => {
                self.ctx.record_node_def(node.id, DefId::Type(id));
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
    DefCollector { ctx, diagnostics }.visit_crate(ast);

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
        ctx.record_node_def(implementation.id, DefId::Type(target));
        ctx.record_node_def(implementation.ty.id, DefId::Type(target));

        for item in &implementation.items {
            let (ast_node_id, span, name, kind) = match &item.kind {
                AssociatedItemKind::Function(function) => (
                    function.id,
                    &function.span,
                    &function.name,
                    ValueDefKind::Function(FunctionDef { sig: None }),
                ),
                AssociatedItemKind::ConstItem(constant) => (
                    constant.id,
                    &constant.span,
                    &constant.name,
                    ValueDefKind::Const,
                ),
            };

            let result = ctx.get_type_def_mut(target).associated_values_mut().define(
                name.name.clone(),
                ValueDef {
                    name: name.name.clone(),
                    ast_node_id: Some(ast_node_id),
                    span: Some(span.clone()),
                    kind,
                },
            );

            match result {
                Ok(id) => {
                    ctx.record_node_def(
                        ast_node_id,
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

    let id = ctx.lookup_type_def(&name.name).ok_or_else(|| {
        Diagnostic::new(
            format!("Unknown impl target {}", name.name),
            name.span.clone(),
        )
    })?;

    match ctx.get_type_def(id) {
        TypeDef::Struct(_) => Ok(id),
        TypeDef::Builtin(_) => Err(Diagnostic::new(
            "Impl target must be a user struct name",
            ty.span.clone(),
        )),
    }
}
