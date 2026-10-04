use crate::{
    diagnostic::Diagnostic,
    frontend::ast::{AstNodeId, Crate},
    semantic::{
        ctx::{DefTable, TypeDef, TypeDefKind, ValueDef, ValueDefKind},
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
        if let Err(_) = self.defs.types.define(
            name.clone(),
            TypeDef {
                name: name.clone(),
                ast_node_id: Some(self.current_item_id.expect("struct must be in an item")),
                kind: TypeDefKind::Struct,
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
}
