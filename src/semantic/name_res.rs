use std::{any::TypeId, collections::HashMap};

use crate::{
    diagnostic::Diagnostic,
    frontend::ast::Crate,
    semantic::{
        ctx::{DefTable, LocalDef, LocalTable},
        visitor::Visitor,
    },
};

struct NameResolution<'a> {
    defs: &'a mut DefTable,
    locals: &'a mut LocalTable,
    diagnostics: &'a mut Vec<Diagnostic>,

    scopes: Vec<HashMap<String, LocalDef>>,
    current_impl: Option<TypeId>,
}

impl<'a> Visitor for NameResolution<'a> {
    
}

pub fn name_resolve(
    ast: &Crate,
    defs: &mut DefTable,
    locals: &mut LocalTable,
    diagnostics: &mut Vec<Diagnostic>,
) {
    NameResolution {
        defs: defs,
        locals: locals,
        diagnostics: diagnostics,

        scopes: Vec::new(),
        current_impl: None,
    }
    .visit_crate(ast);
}
