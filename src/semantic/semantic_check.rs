use crate::{
    diagnostic::Diagnostic,
    frontend::ast::Crate,
    semantic::{
        ctx::{SemanticCtx, TypeDef, TypeDefKind, ValueDef, ValueDefKind},
        def_collect::def_collect,
    },
};

pub fn semantic_check(ast: &Crate) -> Result<SemanticCtx, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let mut ctx = SemanticCtx::new();

    for builtin_type in ["i32", "u32", "isize", "usize", "bool", "Box", "Vec"] {
        ctx.defs
            .types
            .define(
                builtin_type.to_string(),
                TypeDef {
                    name: builtin_type.to_string(),
                    ast_node_id: None,
                    kind: TypeDefKind::Builtin,
                },
            )
            .expect("builtin name must not collide");
    }
    for builtin_value in ["get_i32", "print_i32", "println_i32"] {
        ctx.defs
            .values
            .define(
                builtin_value.to_string(),
                ValueDef {
                    name: builtin_value.to_string(),
                    ast_node_id: None,
                    kind: ValueDefKind::BuiltinFunction,
                },
            )
            .expect("builtin name must not collide");
    }

    def_collect(ast, &mut ctx.defs, &mut diagnostics);

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    Ok(ctx)
}
