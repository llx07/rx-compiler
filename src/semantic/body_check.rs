use crate::{
    diagnostic::{Diagnostic, Span},
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
    fn coerce_node(&mut self, id: AstNodeId, expected: &Ty, span: Span) -> Result<(), ()> {
        let Some(from) = self.ctx.node_ty(id) else {
            self.diagnostics
                .push(Diagnostic::new("cannot coerce a node without a type", span));
            return Err(());
        };

        match plan_coercion(from, expected) {
            Ok(adjustment) => {
                self.ctx.record_node_adjustment(id, adjustment);
                Ok(())
            }
            Err(message) => {
                self.diagnostics.push(Diagnostic::new(message, span));
                Err(())
            }
        }
    }

    fn check_function(&mut self, function: &Function) {
        let def_id = self
            .ctx
            .node_def(function.id)
            .expect("function is resolved");
        let ValueDefKind::Function(fn_def) = &self
            .ctx
            .value_def(def_id)
            .expect("function is resolved")
            .kind
        else {
            unreachable!("function must be function");
        };

        let expect_ty = fn_def
            .sig
            .as_ref()
            .expect("function signature was resolved")
            .result
            .clone();

        debug_assert!(self.current_function.is_none());

        self.current_function = Some(FunctionContext {
            id: function.id,
            return_ty: expect_ty.clone(),
        });

        let _ = self.check_block(&function.body, Some(&expect_ty));

        self.current_function = None;
    }

    // result stands for whether diverge
    fn check_block(&mut self, block: &BlockExpr, expected: Option<&Ty>) -> Result<bool, ()> {
        let mut diverged = false;

        for stmt in &block.stmts {
            match &stmt.kind {
                StmtKind::Expr(expr) => {
                    diverged |= self.check_expr(
                        &expr.expr,
                        if expr.has_semi { None } else { Some(&Ty::Unit) },
                    )?;
                }
                StmtKind::Let(let_stmt) => {
                    diverged |= self.check_let_stmt(let_stmt)?;
                }
            };
        }

        if let Some(tail) = &block.tail {
            diverged |= self.check_expr(&tail, expected)?;
            self.ctx
                .record_node_ty(block.id, self.ctx.node_value_ty(tail.id).ok_or(())?.clone());
        } else {
            let ty = if diverged { Ty::Never } else { Ty::Unit };
            self.ctx.record_node_ty(block.id, ty);

            if let Some(expected_ty) = expected {
                self.coerce_node(block.id, &expected_ty, block.span.clone())?;
            }
        };

        Ok(diverged)
    }

    // result stands whether diverge
    fn check_let_stmt(&mut self, stmt: &LetStmt) -> Result<bool, ()> {
        let expected = stmt
            .ty
            .as_ref()
            .map(|ty_node| self.ctx.node_ty(ty_node.id).expect("type is some"));

        let diverged = self.check_expr(&stmt.expr, expected.cloned().as_ref())?;

        let ty = self.ctx.node_value_ty(stmt.expr.id).ok_or(())?.clone();
        self.ctx.record_node_ty(stmt.id, ty);

        let Some(DefId::Local(local_id)) = self.ctx.node_def(stmt.id) else {
            unreachable!("let binding is resolved");
        };
        let local_def = self.ctx.get_local(local_id);

        self.ctx.record_node_place(
            stmt.id,
            PlaceInfo {
                writable: local_def.is_mut,
                mutable_access_blocked: false,
                origin: PlaceOrigin::Existing,
            },
        );

        Ok(diverged)
    }

    // result stands whether diverge
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

fn ty_deref(ty: &Ty) -> Result<(bool, &Ty), String> {
    match ty {
        Ty::Ref { is_mut, inner } => Ok((*is_mut, inner.as_ref())),
        // Box introduces no shared access; place mutability is checked separately.
        Ty::Box(inner) => Ok((true, inner.as_ref())),
        _ => Err(format!("expected a dereferenceable type, got {:?}", ty)),
    }
}

fn plan_coercion(from: &Ty, to: &Ty) -> Result<UseAdjustment, String> {
    if from == to {
        return Ok(UseAdjustment {
            target: to.clone(),
            steps: Vec::new(),
        });
    };

    if *from == Ty::Never {
        return Ok(UseAdjustment {
            target: to.clone(),
            steps: vec![Adjustment::NeverTo],
        });
    }

    if !matches!(from, Ty::Ref { .. }) || !matches!(to, Ty::Ref { .. }) {
        return Err(format!("cannot coerce {:?} to {:?}", from, to));
    }

    let (from_mut, mut from_inner) = ty_deref(from)?;
    let (to_mut, to_inner) = ty_deref(to)?;

    let mut pass_shared = !from_mut;

    let mut steps = vec![Adjustment::Deref];
    while from_inner != to_inner {
        let (is_mut, inner) = ty_deref(from_inner)?;
        pass_shared |= !is_mut;
        steps.push(Adjustment::Deref);
        from_inner = inner;
    }

    if to_mut && pass_shared {
        return Err(format!("cannot coerce {:?} to {:?}", from, to));
    }

    steps.push(Adjustment::Borrow { is_mut: to_mut });
    Ok(UseAdjustment {
        target: to.clone(),
        steps,
    })
}
