mod function;
mod runtime;
mod ty;

use std::collections::HashMap;

use crate::{
    frontend::ast,
    ir::{self, FunctionId, StructId, builder::ModuleBuilder},
    semantic::{DefId, SemanticCtx, TypeId},
};

struct IrLowerer<'a> {
    semantic: &'a SemanticCtx,
    module: ModuleBuilder,
    structs: HashMap<TypeId, StructId>,
    functions: HashMap<DefId, FunctionId>,
    vec_struct: Option<StructId>,
    main_func: Option<FunctionId>,
    alloc_func: Option<FunctionId>,
}

impl IrLowerer<'_> {
    fn lower_krate(mut self, krate: &ast::Crate) -> ir::Module {
        self.declare_builtins();

        let type_ids = self.declare_types();

        for type_id in type_ids {
            self.lower_struct(type_id);
        }

        let mut functions = Vec::new();
        for item in &krate.items {
            match &item.kind {
                ast::ItemKind::Function(function) => functions.push(function),
                ast::ItemKind::Impl(implementation) => {
                    for item in &implementation.items {
                        if let ast::AssociatedItemKind::Function(function) = &item.kind {
                            functions.push(function);
                        }
                    }
                }
                ast::ItemKind::Struct(_) | ast::ItemKind::Const(_) => {}
            }
        }

        for function in &functions {
            self.declare_function(function);
        }

        for function in functions {
            let def = self
                .semantic
                .node_def(function.id)
                .expect("function must be collected");
            self.lower_function(function, self.functions[&def]);
        }

        self.wrap_main();

        self.module.finish()
    }
}

pub fn lower(krate: &ast::Crate, semantic: &SemanticCtx) -> ir::Module {
    IrLowerer {
        semantic,
        module: ModuleBuilder::new(),
        structs: HashMap::new(),
        functions: HashMap::new(),
        vec_struct: None,
        main_func: None,
        alloc_func: None,
    }
    .lower_krate(krate)
}
