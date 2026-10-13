use crate::{
    diagnostic::Diagnostic,
    frontend::ast::Crate,
    semantic::{
        ctx::{BuiltinDef, BuiltinType, Namespace, SemanticCtx, TypeDef, ValueDef, ValueDefKind},
        def_collect::def_collect,
        name_res::name_resolve,
        trait_check::trait_check,
        type_res::type_resolve,
    },
};

pub fn semantic_check(ast: &Crate) -> Result<SemanticCtx, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let mut ctx = SemanticCtx::new();

    for builtin_type in BuiltinType::ALL {
        let mut type_def = TypeDef::Builtin(BuiltinDef {
            kind: builtin_type,
            associated_values: Namespace::new(),
        });

        let associated_functions: &[&str] = match builtin_type {
            BuiltinType::Box => &["new"],
            BuiltinType::Vec => &["new", "len", "is_empty", "push", "remove"],
            _ => &[],
        };

        for &name in associated_functions {
            type_def
                .associated_values_mut()
                .define(
                    name.to_string(),
                    ValueDef {
                        name: name.to_string(),
                        ast_node_id: None,
                        kind: ValueDefKind::BuiltinFunction,
                    },
                )
                .expect("builtin define must success");
        }

        ctx.define_type(builtin_type.name().to_string(), type_def)
            .expect("builtin name must not collide");
    }
    for builtin_value in ["get_i32", "print_i32", "println_i32"] {
        ctx.define_value(
            builtin_value.to_string(),
            ValueDef {
                name: builtin_value.to_string(),
                ast_node_id: None,
                kind: ValueDefKind::BuiltinFunction,
            },
        )
        .expect("builtin name must not collide");
    }

    def_collect(ast, &mut ctx, &mut diagnostics);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    name_resolve(ast, &mut ctx, &mut diagnostics);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    type_resolve(ast, &mut ctx, &mut diagnostics);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    trait_check(ast, &mut ctx, &mut diagnostics);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    Ok(ctx)
}
