use crate::{
    diagnostic::Diagnostic,
    semantic::{DefId, SemanticCtx, Ty, ValueDefKind},
};

pub fn entry_check(ctx: &SemanticCtx, diagnostics: &mut Vec<Diagnostic>) {
    let Some(main_id) = ctx.lookup_value_def("main") else {
        diagnostics.push(Diagnostic::new("program entry point main not found", 0..0));
        return;
    };

    let main_def = DefId::GlobalValue(main_id)
        .as_value_def(ctx)
        .expect("symbol must be collected");

    let ValueDefKind::Function(main_func) = &main_def.kind else {
        diagnostics.push(Diagnostic::new(
            "main must be a function",
            main_def
                .span
                .as_ref()
                .expect("main has a source span")
                .clone(),
        ));
        return;
    };

    let sig = main_func
        .sig
        .as_ref()
        .expect("function signature is resolved");

    if sig.result != Ty::Unit {
        diagnostics.push(Diagnostic::new(
            "main must return ()",
            main_def
                .span
                .as_ref()
                .expect("main has a source span")
                .clone(),
        ));
        return;
    }

    if !sig.params.is_empty() {
        diagnostics.push(Diagnostic::new(
            "main must have no params",
            main_def
                .span
                .as_ref()
                .expect("main has a source span")
                .clone(),
        ));
    }
}
