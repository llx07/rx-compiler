use crate::{
    diagnostic::Diagnostic,
    frontend::ast::Crate,
    semantic::{ctx::DefTable, visitor::Visitor},
};

pub struct DefCollector<'a> {
    defs: &'a mut DefTable,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl<'a> Visitor for DefCollector<'a> {}

pub fn def_collect(ast: &Crate, defs: &mut DefTable, diagnostics: &mut Vec<Diagnostic>) {}
