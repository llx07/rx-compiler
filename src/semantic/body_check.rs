use crate::{
    diagnostic::Diagnostic,
    frontend::ast::*,
    semantic::{ctx::*, visitor::Visitor},
};

#[derive(Debug, Clone, Copy)]
enum PlaceUse {
    AssignTarget,
    Borrow { is_mut: bool },
    ProjectionBase,
    Receiver { is_mut: bool },
}
#[derive(Debug, Clone)]
struct MethodCandidate {
    call: ResolvedCall,
    receiver_adjustment: UseAdjustment,
}

#[derive(Debug, Clone, Copy)]
enum LoopKind {
    Loop,
    While,
}

#[derive(Debug, Clone, Copy)]
enum LoopPhase {
    Condition,
    Body,
}

#[derive(Debug, Clone)]
struct BreakValue {
    source: AstNodeId,
    operand: Option<AstNodeId>, // None for `break;`
}

struct LoopFrame {
    id: AstNodeId,
    kind: LoopKind,
    phase: LoopPhase,
    breaks: Vec<BreakValue>,
}

struct FunctionContext {
    id: AstNodeId,
    return_ty: Ty,
}

pub struct BodyChecker<'a> {
    ctx: &'a mut SemanticCtx,
    diagnostics: &'a mut Vec<Diagnostic>,

    current_function: Option<FunctionContext>,
    loop_stack: Vec<LoopFrame>,
}

impl Visitor for BodyChecker<'_> {
    fn visit_function(&mut self, node: &Function) {
        self.check_function(node);
    }
}

impl BodyChecker<'_> {
    fn check_function(&mut self, function: &Function) {}

    // result stands for can complete or not
    fn check_expr(&mut self, expr: &Expr, expected: Option<&Ty>) -> Result<bool, ()> {
        todo!()
    }

    fn check_entry(&mut self, krate: &Crate) {
        // find main
        let Some(main_id) = self.ctx.lookup_value_def("main") else {
            self.diagnostics
                .push(Diagnostic::new("program entry point main not found", 0..0));
            return;
        };

        let main_def = self
            .ctx
            .value_def(DefId::GlobalValue(main_id))
            .expect("symbol must be collected");

        let ValueDefKind::Function(main_func) = &main_def.kind else {
            self.diagnostics.push(Diagnostic::new(
                "main must be a function",
                main_def.span.as_ref().expect("main is a function").clone(),
            ));
            return;
        };

        let sig = main_func
            .sig
            .as_ref()
            .expect("function signature is resolved");

        if sig.result != Ty::Unit {
            self.diagnostics.push(Diagnostic::new(
                "main must return ()",
                main_def.span.as_ref().expect("main is a function").clone(),
            ));
            return;
        }

        if !sig.params.is_empty() {
            self.diagnostics.push(Diagnostic::new(
                "main must have no params",
                main_def.span.as_ref().expect("main is a function").clone(),
            ));
            return;
        }
    }
}

pub fn body_check(ast: &Crate, ctx: &mut SemanticCtx, diagnostics: &mut Vec<Diagnostic>) {
    let mut checker = BodyChecker {
        ctx,
        diagnostics,

        current_function: None,
        loop_stack: Vec::new(),
    };

    checker.visit_crate(ast);
    checker.check_entry(ast);
}
