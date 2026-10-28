use std::collections::HashMap;

use crate::{
    frontend::ast,
    ir::{self, FunctionId, IrTy, Operand, Signature, StructId, builder::ModuleBuilder},
    semantic::{BuiltinType, DefId, SemanticCtx, Ty, TypeDef, TypeId},
};

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

        let mut type_ids = Vec::new();
        for (type_id, def) in self.semantic.iter_types() {
            if let TypeDef::Struct(structure) = def {
                let id = self.module.declare_struct(structure.name.clone());
                self.structs.insert(type_id, id);
                type_ids.push(type_id);
            } else if let TypeDef::Builtin(builtin) = def {
                if builtin.kind == BuiltinType::Vec {
                    let id = self.module.declare_struct("__rx_vec".into());
                    self.module
                        .define_struct(id, vec![IrTy::Ptr, IrTy::I32, IrTy::I32]);
                    self.vec_struct = Some(id);
                }
            }
        }

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

    fn declare_function(&mut self, function: &ast::Function) {
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
            .module
            .declare_function(mangle_value(&function.name.name, owner), signature);
        self.functions.insert(def, id);
        if owner.is_none() && function.name.name == "main" {
            self.main_func = Some(id);
        }
    }

    fn lower_function(&mut self, _function: &ast::Function, _id: FunctionId) {
        // TODO: Lower the function body.
    }

    fn wrap_main(&mut self) {
        let source_main = self.main_func.expect("source main must be declared");
        let main_id = self.module.declare_function(
            "main".into(),
            Signature {
                params: Vec::new(),
                result: IrTy::I32,
            },
        );
        let mut builder = self.module.define_function(main_id);
        let entry = builder.create_block();
        builder.position_at_end(entry);
        builder.call(source_main, Vec::new());
        builder.terminate(ir::Terminator::Ret(Some(Operand::Constant(
            ir::Constant::I32(0),
        ))));
    }

    fn declare_builtins(&mut self) {
        for (name, params, result) in [
            ("get_i32", Vec::new(), IrTy::I32),
            ("print_i32", vec![IrTy::I32], IrTy::Void),
            ("println_i32", vec![IrTy::I32], IrTy::Void),
        ] {
            let value = self
                .semantic
                .lookup_value_def(name)
                .expect("I/O builtin must be collected");
            let id = self
                .module
                .declare_function(name.into(), Signature { params, result });
            self.functions.insert(DefId::GlobalValue(value), id);
        }

        // The allocator is a runtime helper, not a source-visible value.
        self.alloc_func = Some(self.module.declare_function(
            "__rx_alloc".into(),
            Signature {
                params: vec![IrTy::I32, IrTy::I32],
                result: IrTy::Ptr,
            },
        ));
    }

    fn lower_ty(&self, ty: Ty) -> IrTy {
        match ty {
            Ty::Vec(_) => IrTy::Struct(self.vec_struct.expect("Vec must be declared")),
            Ty::I32 | Ty::U32 | Ty::ISize | Ty::USize => IrTy::I32,
            Ty::Bool => IrTy::I1,
            Ty::Unit | Ty::Never => IrTy::Void,
            Ty::Struct(type_id) => {
                IrTy::Struct(*self.structs.get(&type_id).expect("all struct defined"))
            }
            Ty::Ref { .. } | Ty::Box(_) => IrTy::Ptr,
            Ty::Array { elem, len } => IrTy::Array {
                element: Box::new(self.lower_ty(*elem)),
                len: len as u32,
            },
        }
    }

    fn lower_struct(&mut self, type_id: TypeId) {
        let type_def = self.semantic.get_type_def(type_id);
        let TypeDef::Struct(struct_def) = type_def else {
            unreachable!("type_id must be a struct")
        };

        let fields = struct_def
            .fields
            .iter()
            .map(|(_, field_def)| {
                self.lower_ty(
                    field_def
                        .ty
                        .as_ref()
                        .expect("ty of field should be collected")
                        .clone(),
                )
            })
            .collect::<Vec<_>>();

        let struct_id = self
            .structs
            .get(&type_id)
            .expect("struct should be defined");

        self.module.define_struct(*struct_id, fields);
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
