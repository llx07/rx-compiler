use crate::{
    diagnostic::Diagnostic,
    frontend::ast::Crate,
    semantic::{ctx::SemanticCtx, def_collect::def_collect},
};

pub fn semantic_check(ast: &Crate) -> Result<SemanticCtx, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let mut ctx = SemanticCtx::new();

    def_collect(ast, &mut ctx.defs, &mut diagnostics);

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    Ok(ctx)
}