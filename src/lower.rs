mod equality;
mod expr;
mod function;
mod runtime;
mod ty;

use std::collections::HashMap;

use crate::{
    frontend::ast,
    ir::{self, BlockId, FunctionId, Operand, StructId, builder::Builder},
    semantic::{DefId, LocalId, SemanticCtx, Ty, TypeId},
};

struct LoopInfo {
    entry: BlockId,
    exit: Option<BlockId>,
    result: Option<Operand>,
}

struct IrLowerer<'a> {
    semantic: &'a SemanticCtx,
    builder: Builder,
    locals: HashMap<LocalId, expr::LoweredExpr>,
    loop_info: HashMap<ast::AstNodeId, LoopInfo>,
    structs: HashMap<TypeId, StructId>,
    functions: HashMap<DefId, FunctionId>,
    equality_fns: HashMap<Ty, FunctionId>,
    pending_equality: Vec<(Ty, FunctionId)>,
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

        self.define_equalities();
        self.wrap_main();

        self.builder.finish()
    }
}

pub fn lower(krate: &ast::Crate, semantic: &SemanticCtx) -> ir::Module {
    IrLowerer {
        semantic,
        builder: Builder::new(),
        locals: HashMap::new(),
        loop_info: HashMap::new(),
        structs: HashMap::new(),
        functions: HashMap::new(),
        equality_fns: HashMap::new(),
        pending_equality: Vec::new(),
        vec_struct: None,
        main_func: None,
        alloc_func: None,
    }
    .lower_krate(krate)
}
