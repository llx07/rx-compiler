use crate::{
    diagnostic::Diagnostic,
    frontend::ast::{AssociatedItemKind, AstNodeId, Crate, ItemKind, PathIdent, TyAst, TyAstKind},
    semantic::{
        ctx::{
            DefTable, FieldDef, StructDef, TypeDef, TypeDefKind, TypeId, ValueDef, ValueDefKind,
        },
        visitor::{Visitor, walk_item},
    },
};

pub struct DefCollector<'a> {
    defs: &'a mut DefTable,
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
        if let Err(_) = self.defs.values.define(
            name.clone(),
            ValueDef {
                name: name.clone(),
                ast_node_id: Some(self.current_item_id.expect("function must be in an item")),
                kind: ValueDefKind::Function,
            },
        ) {
            self.diagnostics.push(Diagnostic {
                message: format!("Duplicate definition of {}", name),
                span: node.name.span.clone(),
            });
        }
    }

    fn visit_const_item(&mut self, node: &crate::frontend::ast::ConstItem) {
        let name = &node.name.name;
        if let Err(_) = self.defs.values.define(
            name.clone(),
            ValueDef {
                name: name.clone(),
                ast_node_id: Some(self.current_item_id.expect("const item must be in an item")),
                kind: ValueDefKind::Const,
            },
        ) {
            self.diagnostics.push(Diagnostic {
                message: format!("Duplicate definition of {}", name),
                span: node.name.span.clone(),
            });
        }
    }

    fn visit_struct(&mut self, node: &crate::frontend::ast::Struct) {
        let name = &node.name.name;
        let mut structure = StructDef::new();
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
        if let Err(_) = self.defs.types.define(
            name.clone(),
            TypeDef {
                name: name.clone(),
                ast_node_id: Some(self.current_item_id.expect("struct must be in an item")),
                kind: TypeDefKind::Struct(structure),
            },
        ) {
            self.diagnostics.push(Diagnostic {
                message: format!("Duplicate definition of {}", name),
                span: node.name.span.clone(),
            });
        }
    }

    fn visit_impl(&mut self, _: &crate::frontend::ast::Impl) {
        // do nothing to skip associated items
    }
}

pub fn def_collect(ast: &Crate, defs: &mut DefTable, diagnostics: &mut Vec<Diagnostic>) {
    DefCollector {
        defs: defs,
        diagnostics: diagnostics,
        current_item_id: None,
    }
    .visit_crate(ast);

    if diagnostics.is_empty() {
        collect_impls(ast, defs, diagnostics);
    }
}

fn collect_impls(ast: &Crate, defs: &mut DefTable, diagnostics: &mut Vec<Diagnostic>) {
    for item in &ast.items {
        let ItemKind::Impl(implementation) = &item.kind else {
            continue;
        };
        let target = match resolve_impl_target(&implementation.ty, defs) {
            Ok(target) => target,
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                continue;
            }
        };
        let TypeDefKind::Struct(structure) = &mut defs.types.get_mut(target).kind else {
            unreachable!("resolved impl target must be a user struct");
        };
        for item in &implementation.items {
            let (name, kind) = match &item.kind {
                AssociatedItemKind::Function(function) => (&function.name, ValueDefKind::Function),
                AssociatedItemKind::ConstItem(constant) => (&constant.name, ValueDefKind::Const),
            };
            if structure
                .associated_values
                .define(
                    name.name.clone(),
                    ValueDef {
                        name: name.name.clone(),
                        ast_node_id: Some(item.id),
                        kind,
                    },
                )
                .is_err()
            {
                diagnostics.push(Diagnostic::new(
                    format!("Duplicate associated value definition of {}", name.name),
                    name.span.clone(),
                ));
            }
        }
    }
}

fn resolve_impl_target(ty: &TyAst, defs: &DefTable) -> Result<TypeId, Diagnostic> {
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

    let id = defs.types.lookup(&name.name).ok_or_else(|| {
        Diagnostic::new(
            format!("Unknown impl target {}", name.name),
            name.span.clone(),
        )
    })?;

    if !matches!(&defs.types.get(id).kind, TypeDefKind::Struct(_)) {
        return Err(Diagnostic::new(
            "Impl target must be a user struct name",
            ty.span.clone(),
        ));
    }
    Ok(id)
}
