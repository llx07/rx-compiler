use crate::{
    diagnostic::Diagnostic,
    frontend::ast::Crate,
    semantic::{
        ctx::{Namespace, SemanticCtx, TypeDef, ValueDef, ValueDefKind},
        def_collect::def_collect,
        name_res::name_resolve,
    },
};

pub fn semantic_check(ast: &Crate) -> Result<SemanticCtx, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let mut ctx = SemanticCtx::new();

    for builtin_type in ["i32", "u32", "isize", "usize", "bool", "Box", "Vec"] {
        let mut type_def = TypeDef {
            name: builtin_type.to_string(),
            ast_node_id: None,
            fields: Namespace::new(),
            associated_values: Namespace::new(),
        };

        if matches!(builtin_type, "Box" | "Vec") {
            type_def
                .associated_values
                .define(
                    "new".to_string(),
                    ValueDef {
                        name: "new".to_string(),
                        ast_node_id: None,
                        kind: ValueDefKind::BuiltinFunction,
                    },
                )
                .expect("builtin define must success");
        }

        ctx.define_type(builtin_type.to_string(), type_def)
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

    Ok(ctx)
}
