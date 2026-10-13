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

impl Visitor for BodyChecker<'_> {}

impl BodyChecker<'_> {
    fn check_expr(&mut self, expr: &Expr, expected: Option<&Ty>) -> Result<bool, ()> {
        // can complete
        todo!()
    }
}
