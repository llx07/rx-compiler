use std::collections::HashSet;

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
    operand_diverged: bool,
    span: Span,
}

struct LoopFrame {
    id: AstNodeId,
    kind: LoopKind,
    phase: LoopPhase,
    expected: Option<Ty>,
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
                // Reborrowing a stored &mut must respect shared access and
                // the mutable container borrow required by Vec indexing.
                if matches!(from, Ty::Ref { .. })
                    && matches!(expected, Ty::Ref { is_mut: true, .. })
                    && self
                        .ctx
                        .node_place(id)
                        .is_some_and(|place| place.mutable_access_blocked)
                {
                    self.diagnostics.push(Diagnostic::new(
                        "cannot mutably reborrow through this access path",
                        span,
                    ));
                    return Err(());
                }
                self.ctx.record_node_adjustment(id, adjustment);
                Ok(())
            }
            Err(_) => {
                self.diagnostics.push(Diagnostic::new(
                    format!("expected {:?}, found {:?}", expected, from),
                    span,
                ));
                Err(())
            }
        }
    }

    fn check_function(&mut self, function: &Function) {
        let def_id = self
            .ctx
            .node_def(function.id)
            .expect("function is resolved");
        let fn_def = def_id
            .as_function_def(self.ctx)
            .expect("function must have a function definition");

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

        for param in &function.params {
            let local_def = self
                .ctx
                .node_local(param.id)
                .expect("parameter is resolved");
            self.ctx.record_node_place(
                param.id,
                PlaceInfo {
                    writable: local_def.is_mut,
                    mutable_access_blocked: false,
                },
            );
        }

        if let Some(receiver) = &function.receiver {
            let local_def = self
                .ctx
                .node_local(receiver.id)
                .expect("receiver is resolved");
            self.ctx.record_node_place(
                receiver.id,
                PlaceInfo {
                    writable: local_def.is_mut,
                    mutable_access_blocked: false,
                },
            );
        }

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

        let local_def = self
            .ctx
            .node_local(stmt.id)
            .expect("let binding is resolved");

        self.ctx.record_node_place(
            stmt.id,
            PlaceInfo {
                writable: local_def.is_mut,
                mutable_access_blocked: false,
            },
        );

        Ok(diverged)
    }

    // result stands whether diverge
    fn check_expr(&mut self, expr: &Expr, expected: Option<&Ty>) -> Result<bool, ()> {
        match &expr.kind {
            ExprKind::Literal(literal) => {
                self.check_literal(literal, expr.id, &expr.span, expected)
            }
            ExprKind::Unit => self.check_unit(expr.id, &expr.span, expected),
            ExprKind::Path(_) => self.check_path(expr.id, &expr.span, expected),
            ExprKind::Block(block) => {
                let diverged = self.check_block(block, expected)?;

                self.ctx.record_node_ty(
                    expr.id,
                    self.ctx
                        .node_value_ty(block.id)
                        .expect("block expression should have type")
                        .clone(),
                );
                Ok(diverged)
            }
            ExprKind::Unary(unary) => self.check_unary(unary, expr.id, &expr.span, expected),
            ExprKind::Binary(binary) => self.check_binary(binary, expr.id, &expr.span, expected),
            ExprKind::Assign(assign) => self.check_assign(assign, expr.id, &expr.span, expected),
            ExprKind::Cast(cast) => self.check_cast(cast, expr.id, &expr.span, expected),
            ExprKind::Array(array) => self.check_array(array, expr.id, &expr.span, expected),
            ExprKind::Index(index) => self.check_index(index, expr.id, &expr.span, expected),
            ExprKind::Struct(structure) => {
                self.check_struct(structure, expr.id, &expr.span, expected)
            }
            ExprKind::Call(call) => self.check_call(call, expr.id, &expr.span, expected),
            ExprKind::MethodCall(call) => {
                self.check_method_call(call, expr.id, &expr.span, expected)
            }
            ExprKind::Field(field) => self.check_field(field, expr.id, &expr.span, expected),
            ExprKind::Loop(loop_expr) => self.check_loop(loop_expr, expr.id, &expr.span, expected),
            ExprKind::Break(value) => {
                self.check_break(value.as_deref(), expr.id, &expr.span, expected)
            }
            ExprKind::Continue => self.check_continue(expr.id, &expr.span, expected),
            ExprKind::If(if_expr) => self.check_if(if_expr, expr.id, &expr.span, expected),
            ExprKind::Return(value) => {
                self.check_return(value.as_deref(), expr.id, &expr.span, expected)
            }
        }
    }

    fn check_literal(
        &mut self,
        expr: &LiteralExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let ty = match expr {
            LiteralExpr::Int(int) => {
                let suffix_ty = int.suffix.as_ref().map(|suf| match suf {
                    IntSuffix::I32 => Ty::I32,
                    IntSuffix::ISize => Ty::ISize,
                    IntSuffix::U32 => Ty::U32,
                    IntSuffix::USize => Ty::USize,
                });

                suffix_ty
                    .or_else(|| {
                        expected
                            .filter(|ty| matches!(ty, Ty::I32 | Ty::ISize | Ty::U32 | Ty::USize))
                            .cloned()
                    })
                    .unwrap_or(Ty::I32)
            }

            LiteralExpr::Bool(_) => Ty::Bool,
        };

        self.ctx.record_node_ty(id, ty);
        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }
        Ok(false)
    }

    fn check_unit(
        &mut self,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        self.ctx.record_node_ty(id, Ty::Unit);
        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }

        Ok(false)
    }

    fn check_path(
        &mut self,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        // here path will not be an callee, as callee will be resolved in CallExpr
        // path is either an local binding or constant value

        let node_def = self
            .ctx
            .node_def(id)
            .expect("path is collected, as path is not Foo::clone");

        let (ty, place) = match node_def {
            DefId::AssociatedValue { .. } | DefId::GlobalValue(_) => {
                let value_def = node_def
                    .as_value_def(self.ctx)
                    .expect("value def is collected");

                match value_def.kind {
                    ValueDefKind::Const => (
                        self.ctx
                            .node_value_ty(value_def.ast_node_id.expect("const has an id"))
                            .expect("const has a type")
                            .clone(),
                        None,
                    ),
                    _ => {
                        unreachable!("using function as value is undefined behavior");
                    }
                }
            }
            DefId::Field { .. } => {
                let field_def = node_def
                    .as_field_def(self.ctx)
                    .expect("field def is collected");
                (
                    field_def.ty.clone().expect("field type should be resolved"),
                    self.ctx.node_place(field_def.ast_node_id),
                )
            }
            DefId::Local(_) => {
                let local_def = node_def
                    .as_local_def(self.ctx)
                    .expect("field def is collected");
                (
                    self.ctx
                        .node_value_ty(local_def.ast_node_id)
                        .expect("local def has type")
                        .clone(),
                    self.ctx.node_place(local_def.ast_node_id),
                )
            }
            DefId::Type(_) => {
                unreachable!("incompatible type in path expression is rejected before")
            }
        };

        self.ctx.record_node_ty(id, ty.clone());
        if let Some(place) = place {
            self.ctx.record_node_place(id, place);
        };

        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }

        Ok(ty == Ty::Never)
    }

    fn check_unary(
        &mut self,
        expr: &UnaryExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let diverged = self.check_expr(&expr.operand, None)?;
        let inner_ty = self
            .ctx
            .node_value_ty(expr.operand.id)
            .expect("inner operand has a type")
            .clone();

        match expr.op {
            UnaryOp::Borrow | UnaryOp::BorrowMut => {
                let is_mut = expr.op == UnaryOp::BorrowMut;

                let place = if let Some(place) = self.ctx.node_place(expr.operand.id) {
                    place
                } else {
                    self.ctx.materialize_value(expr.operand.id)
                };

                if is_mut && (!place.writable || place.mutable_access_blocked) {
                    self.diagnostics.push(Diagnostic::new(
                        "cannot mutably borrow an immutable place",
                        expr.operand.span.clone(),
                    ));
                    return Err(());
                }

                self.ctx.record_node_ty(
                    id,
                    Ty::Ref {
                        is_mut,
                        inner: Box::new(inner_ty),
                    },
                );
            }
            UnaryOp::Deref => {
                let operand_place = self.ctx.node_place(expr.operand.id);
                let (ty, place) = match inner_ty {
                    Ty::Ref { is_mut, inner } => {
                        // An immutable binding can hold a usable &mut, but shared
                        // access earlier in the path cannot be made mutable again.
                        let mutable_access_blocked = !is_mut
                            || operand_place.is_some_and(|place| place.mutable_access_blocked);
                        (
                            *inner,
                            PlaceInfo {
                                writable: is_mut && !mutable_access_blocked,
                                mutable_access_blocked,
                            },
                        )
                    }
                    Ty::Box(inner) => {
                        // Owned Box access preserves its owner's mutability
                        let place = operand_place.unwrap_or_else(|| PlaceInfo {
                            writable: true,
                            mutable_access_blocked: false,
                        });
                        (*inner, place)
                    }
                    _ => {
                        self.diagnostics.push(Diagnostic::new(
                            format!("cannot dereference {:?}", inner_ty),
                            span.clone(),
                        ));
                        return Err(());
                    }
                };

                self.ctx.record_node_ty(id, ty);
                self.ctx.record_node_place(id, place);
                self.ctx.record_node_access(id, ResolvedAccess::Deref);
            }
            UnaryOp::Neg | UnaryOp::Not => {
                let scalar_ty = match expr.op {
                    UnaryOp::Neg => inner_ty
                        .as_integer()
                        .filter(|ty| matches!(ty, Ty::I32 | Ty::ISize)),
                    UnaryOp::Not => inner_ty.as_scalar(),
                    _ => unreachable!(),
                };

                let Some(scalar_ty) = scalar_ty else {
                    let operator = if expr.op == UnaryOp::Neg { "-" } else { "!" };
                    self.diagnostics.push(Diagnostic::new(
                        format!("cannot apply unary {operator} to {:?}", inner_ty),
                        span.clone(),
                    ));
                    return Err(());
                };

                self.ctx.record_node_ty(id, scalar_ty.clone());
            }
        };

        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }

        Ok(diverged)
    }

    fn check_binary(
        &mut self,
        expr: &BinaryExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let diverged = self.check_expr(&expr.lhs, None)? | self.check_expr(&expr.rhs, None)?;

        let lhs_ty = self.ctx.node_value_ty(expr.lhs.id).expect("ty is resolved");
        let rhs_ty = self.ctx.node_value_ty(expr.rhs.id).expect("ty is resolved");

        match expr.op {
            BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Rem
            | BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::Shl
            | BinaryOp::Shr => {
                let (lhs_operand_ty, rhs_operand_ty, expected_type) = match expr.op {
                    BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => (
                        lhs_ty.as_scalar(),
                        rhs_ty.as_scalar(),
                        "an integer or bool type",
                    ),
                    _ => (lhs_ty.as_integer(), rhs_ty.as_integer(), "an integer type"),
                };
                let Some(lhs_ty) = lhs_operand_ty else {
                    self.diagnostics.push(Diagnostic::new(
                        format!("expected {expected_type}, found {:?}", lhs_ty),
                        expr.lhs.span.clone(),
                    ));
                    return Err(());
                };
                let Some(rhs_ty) = rhs_operand_ty else {
                    self.diagnostics.push(Diagnostic::new(
                        format!("expected {expected_type}, found {:?}", rhs_ty),
                        expr.rhs.span.clone(),
                    ));
                    return Err(());
                };
                if !matches!(expr.op, BinaryOp::Shl | BinaryOp::Shr) && lhs_ty != rhs_ty {
                    self.diagnostics.push(Diagnostic::new(
                        format!("expected same type, found {:?} and {:?}", lhs_ty, rhs_ty),
                        expr.rhs.span.clone(),
                    ));
                    return Err(());
                }
                self.ctx.record_node_ty(id, lhs_ty.clone());
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                // because cross-type equality, we will not check rhs
                if !self.ctx.implements_trait(lhs_ty, TraitKind::PartialEq) {
                    self.diagnostics.push(Diagnostic::new(
                        format!("{:?} does not implement trait PartialEq", lhs_ty),
                        expr.rhs.span.clone(),
                    ));
                    return Err(());
                }

                self.ctx.record_node_ty(id, Ty::Bool);
            }
            BinaryOp::Le | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Gt => {
                if lhs_ty.as_scalar().is_none() {
                    self.diagnostics.push(Diagnostic::new(
                        format!("expected an integer or bool type, found {:?}", lhs_ty),
                        expr.lhs.span.clone(),
                    ));
                    return Err(());
                }
                if rhs_ty.as_scalar().is_none() {
                    self.diagnostics.push(Diagnostic::new(
                        format!("expected an integer or bool type, found {:?}", rhs_ty),
                        expr.rhs.span.clone(),
                    ));
                    return Err(());
                }
                if lhs_ty.as_scalar() != rhs_ty.as_integer() {
                    self.diagnostics.push(Diagnostic::new(
                        format!("cannot compare between {:?} and {:?}", lhs_ty, rhs_ty),
                        span.clone(),
                    ));
                    return Err(());
                }

                if lhs_ty != rhs_ty {
                    // the only exception: &T < &mut T
                    let Ty::Ref {
                        is_mut: lhs_is_mut,
                        inner: lhs_inner,
                    } = lhs_ty
                    else {
                        self.diagnostics.push(Diagnostic::new(
                            format!("cannot compare between {:?} and {:?}", lhs_ty, rhs_ty),
                            span.clone(),
                        ));
                        return Err(());
                    };
                    let Ty::Ref {
                        is_mut: rhs_is_mut,
                        inner: rhs_inner,
                    } = rhs_ty
                    else {
                        self.diagnostics.push(Diagnostic::new(
                            format!("cannot compare between {:?} and {:?}", lhs_ty, rhs_ty),
                            span.clone(),
                        ));
                        return Err(());
                    };

                    if *lhs_is_mut || !rhs_is_mut || lhs_inner != rhs_inner {
                        self.diagnostics.push(Diagnostic::new(
                            format!("cannot compare between {:?} and {:?}", lhs_ty, rhs_ty),
                            span.clone(),
                        ));
                        return Err(());
                    }
                }

                self.ctx.record_node_ty(id, Ty::Bool);
            }

            BinaryOp::And | BinaryOp::Or => {
                if *lhs_ty != Ty::Bool || *rhs_ty != Ty::Bool {
                    self.diagnostics.push(Diagnostic::new(
                        format!("expected a bool type, found {:?} and {:?}", lhs_ty, rhs_ty),
                        span.clone(),
                    ));
                    return Err(());
                }
                self.ctx.record_node_ty(id, Ty::Bool);
            }
        };

        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }
        Ok(diverged)
    }

    fn check_assign(
        &mut self,
        expr: &AssignExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let mut diverged = self.check_expr(&expr.lhs, None)?;

        let lhs_ty = self
            .ctx
            .node_value_ty(expr.lhs.id)
            .expect("type resolved")
            .clone();

        diverged |= self.check_expr(
            &expr.rhs,
            if expr.op == AssignOp::Assign {
                Some(&lhs_ty)
            } else {
                None
            },
        )?;

        if self
            .ctx
            .node_place(expr.lhs.id)
            .is_none_or(|place| !place.writable)
        {
            self.diagnostics.push(Diagnostic::new(
                "cannot assign to this",
                expr.lhs.span.clone(),
            ));

            return Err(());
        };

        if expr.op != AssignOp::Assign {
            let is_bitwise = matches!(
                expr.op,
                AssignOp::BitAndAssign | AssignOp::BitOrAssign | AssignOp::BitXorAssign
            );
            let expected_type: &str = if is_bitwise {
                "an integer or bool type"
            } else {
                "an integer type"
            };

            if !matches!(lhs_ty, Ty::I32 | Ty::ISize | Ty::U32 | Ty::USize)
                && !(is_bitwise && lhs_ty == Ty::Bool)
            {
                self.diagnostics.push(Diagnostic::new(
                    format!("expected {expected_type}, found {:?}", lhs_ty),
                    expr.lhs.span.clone(),
                ));
                return Err(());
            }

            let rhs_ty = self.ctx.node_value_ty(expr.rhs.id).expect("type resolved");
            let rhs_operand_ty = if is_bitwise {
                rhs_ty.as_scalar()
            } else {
                rhs_ty.as_integer()
            };
            let Some(rhs_ty) = rhs_operand_ty else {
                self.diagnostics.push(Diagnostic::new(
                    format!("expected {expected_type}, found {:?}", rhs_ty),
                    expr.rhs.span.clone(),
                ));
                return Err(());
            };

            if !matches!(expr.op, AssignOp::ShlAssign | AssignOp::ShrAssign) && lhs_ty != *rhs_ty {
                self.diagnostics.push(Diagnostic::new(
                    format!("expected same type, found {:?} and {:?}", lhs_ty, rhs_ty),
                    span.clone(),
                ));
                return Err(());
            }
        }

        self.ctx.record_node_ty(id, Ty::Unit);
        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }
        Ok(diverged)
    }

    fn check_cast(
        &mut self,
        expr: &CastExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let diverged = self.check_expr(&expr.expr, None)?;

        let from = self.ctx.node_value_ty(expr.expr.id).expect("resolved");
        let to = self.ctx.node_ty(expr.ty.id).expect("resolved");

        if !matches!(from, Ty::U32 | Ty::USize | Ty::I32 | Ty::ISize | Ty::Bool)
            || !matches!(to, Ty::U32 | Ty::USize | Ty::I32 | Ty::ISize)
        {
            self.diagnostics.push(Diagnostic::new(
                format!("invalid cast between {:?} and {:?}", from, to),
                span.clone(),
            ));
            return Err(());
        };

        self.ctx.record_node_ty(id, to.clone());
        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }
        Ok(diverged)
    }

    fn check_array(
        &mut self,
        expr: &ArrayExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let (expected_ty, expected_len) = if let Some(expected) = expected {
            if let Ty::Array { elem, len } = expected {
                (Some(*elem.clone()), Some(len))
            } else {
                self.diagnostics
                    .push(Diagnostic::new("incompatible type", span.clone()));
                return Err(());
            }
        } else {
            (None, None)
        };

        let mut diverged = false;

        let array_len = match expr {
            ArrayExpr::Elements(elems) => {
                for elem in elems {
                    diverged |= self.check_expr(elem, expected_ty.as_ref())?;
                }
                elems.len()
            }
            ArrayExpr::Repeat { value, len } => {
                diverged |= self.check_expr(value, expected_ty.as_ref())?;
                self.ctx
                    .node_const(len.id)
                    .expect("const value should be collected")
                    .value as usize
            }
        };

        if expected_len.is_some_and(|expected_len| *expected_len != array_len) {
            self.diagnostics
                .push(Diagnostic::new("array len mismatch", span.clone()));
            return Err(());
        };

        let final_ty = match expr {
            ArrayExpr::Elements(elems) => {
                let ty = lub_coerce(
                    elems
                        .iter()
                        .map(|expr| {
                            self.ctx
                                .node_value_ty(expr.id)
                                .expect("node type is resolved")
                        })
                        .collect::<Vec<_>>(),
                )
                .ok_or(())?;
                for elem in elems {
                    self.coerce_node(elem.id, &ty, elem.span.clone())?;
                }
                ty
            }
            ArrayExpr::Repeat { value, .. } => {
                let ty = self
                    .ctx
                    .node_value_ty(value.id)
                    .expect("node type is resolved")
                    .clone();

                if array_len > 1 && !self.ctx.implements_trait(&ty, TraitKind::Copy) {
                    self.diagnostics.push(Diagnostic::new(
                        format!("{:?} does not implement trait Copy", ty),
                        span.clone(),
                    ));
                    return Err(());
                }
                ty
            }
        };

        self.ctx.record_node_ty(
            id,
            Ty::Array {
                elem: Box::new(final_ty),
                len: array_len,
            },
        );

        Ok(diverged)
    }

    fn check_index(
        &mut self,
        expr: &IndexExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let mut diverged = self.check_expr(&expr.base, None)?;
        diverged |= self.check_expr(&expr.index, Some(&Ty::USize))?;

        let mut base_ty = self
            .ctx
            .node_value_ty(expr.base.id)
            .expect("index base has a type")
            .clone();
        let mut steps = Vec::new();
        let mut place = if let Some(place) = self.ctx.node_place(expr.base.id) {
            place
        } else {
            steps.push(Adjustment::MaterializeTemporary);
            self.ctx.materialize_value(expr.base.id)
        };

        loop {
            match base_ty {
                Ty::Ref { is_mut, inner } => {
                    place.mutable_access_blocked |= !is_mut;
                    place.writable = is_mut && !place.mutable_access_blocked;
                    base_ty = *inner;
                    steps.push(Adjustment::Deref);
                }
                Ty::Box(inner) => {
                    base_ty = *inner;
                    steps.push(Adjustment::Deref);
                }
                _ => break,
            }
        }

        let (elem_ty, kind) = match &base_ty {
            Ty::Array { elem, .. } => (elem.as_ref().clone(), IndexKind::Array),
            Ty::Vec(elem) => {
                // Unlike array projection, Vec indexing borrows the container.
                // Later dereferences must not bypass an immutable Vec owner.
                place.mutable_access_blocked |= !place.writable;
                (elem.as_ref().clone(), IndexKind::Vec)
            }
            _ => {
                self.diagnostics.push(Diagnostic::new(
                    format!("cannot index {:?}", base_ty),
                    expr.base.span.clone(),
                ));
                return Err(());
            }
        };

        self.ctx.record_node_adjustment(
            expr.base.id,
            UseAdjustment {
                target: base_ty,
                steps,
            },
        );
        self.ctx.record_node_ty(id, elem_ty);
        self.ctx.record_node_place(id, place);
        self.ctx.record_node_access(id, ResolvedAccess::Index(kind));

        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }
        Ok(diverged)
    }

    fn check_struct(
        &mut self,
        expr: &StructExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let def_id = self
            .ctx
            .node_def(expr.path.id)
            .expect("struct expression path is resolved");
        let Some(structure) = def_id.as_struct_def(self.ctx) else {
            self.diagnostics.push(Diagnostic::new(
                "expected a named-field struct",
                expr.path.span.clone(),
            ));
            return Err(());
        };
        let DefId::Type(owner) = def_id else {
            unreachable!("a struct definition has a type ID");
        };

        let mut initialized = HashSet::new();
        let mut initializers = Vec::with_capacity(expr.fields.len());

        for field in &expr.fields {
            let Some(field_id) = structure.fields.lookup(&field.name.name) else {
                self.diagnostics.push(Diagnostic::new(
                    format!("unknown field {}", field.name.name),
                    field.name.span.clone(),
                ));
                return Err(());
            };
            if !initialized.insert(field_id) {
                self.diagnostics.push(Diagnostic::new(
                    format!("duplicate initializer for field {}", field.name.name),
                    field.name.span.clone(),
                ));
                return Err(());
            }

            let field_ty = structure
                .fields
                .get(field_id)
                .ty
                .clone()
                .expect("struct field type is resolved");
            initializers.push((field, field_id, field_ty));
        }

        if let Some((_, field)) = structure
            .fields
            .iter()
            .find(|(field_id, _)| !initialized.contains(field_id))
        {
            self.diagnostics.push(Diagnostic::new(
                format!("missing initializer for field {}", field.name),
                span.clone(),
            ));
            return Err(());
        }

        let mut diverged = false;
        for (field, field_id, field_ty) in initializers {
            self.ctx.record_node_def(
                field.id,
                DefId::Field {
                    owner,
                    field: field_id,
                },
            );
            diverged |= self.check_expr(&field.value, Some(&field_ty))?;
        }

        self.ctx.record_node_ty(id, Ty::Struct(owner));
        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }
        Ok(diverged)
    }

    fn check_call(
        &mut self,
        _expr: &CallExpr,
        _id: AstNodeId,
        _span: &Span,
        _expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        todo!()
    }

    fn check_method_call(
        &mut self,
        _expr: &MethodCallExpr,
        _id: AstNodeId,
        _span: &Span,
        _expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        todo!()
    }

    fn check_field(
        &mut self,
        expr: &FieldExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let diverged = self.check_expr(&expr.base, None)?;
        let mut node_ty = self
            .ctx
            .node_value_ty(expr.base.id)
            .expect("type resolved")
            .clone();
        let mut steps = Vec::new();
        let mut base_place = self.ctx.node_place(expr.base.id).unwrap_or_else(|| {
            steps.push(Adjustment::MaterializeTemporary);
            self.ctx.materialize_value(expr.base.id)
        });

        let base_ty_id = loop {
            match node_ty {
                Ty::Struct(type_id) => break type_id,
                Ty::Ref { is_mut, inner } => {
                    base_place.mutable_access_blocked |= !is_mut;
                    base_place.writable = is_mut && !base_place.mutable_access_blocked;
                    node_ty = *inner;
                    steps.push(Adjustment::Deref);
                }
                Ty::Box(inner) => {
                    node_ty = *inner;
                    steps.push(Adjustment::Deref);
                }
                _ => {
                    self.diagnostics.push(Diagnostic::new(
                        format!("cannot access a field on {:?}", node_ty),
                        expr.base.span.clone(),
                    ));
                    return Err(());
                }
            }
        };

        let struct_def = DefId::Type(base_ty_id)
            .as_struct_def(self.ctx)
            .expect("struct must have struct def");

        let member = &expr.field.name;

        let Some(field_id) = struct_def.fields.lookup(member) else {
            self.diagnostics.push(Diagnostic::new(
                format!("unknown field {}", member),
                expr.field.span.clone(),
            ));
            return Err(());
        };

        let field_ty = struct_def
            .fields
            .get(field_id)
            .ty
            .clone()
            .expect("struct field type is resolved");

        self.ctx.record_node_ty(id, field_ty);
        self.ctx.record_node_adjustment(
            expr.base.id,
            UseAdjustment {
                target: Ty::Struct(base_ty_id),
                steps,
            },
        );

        self.ctx.record_node_place(id, base_place);
        self.ctx.record_node_def(
            id,
            DefId::Field {
                owner: base_ty_id,
                field: field_id,
            },
        );
        self.ctx.record_node_access(
            id,
            ResolvedAccess::Field {
                owner: base_ty_id,
                field: field_id,
            },
        );

        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }

        Ok(diverged)
    }

    fn check_loop(
        &mut self,
        expr: &LoopExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let (kind, phase, break_expected) = match expr {
            LoopExpr::Loop { .. } => (LoopKind::Loop, LoopPhase::Body, expected.cloned()),
            LoopExpr::While { .. } => (LoopKind::While, LoopPhase::Condition, None),
        };
        self.loop_stack.push(LoopFrame {
            id,
            kind,
            phase,
            expected: break_expected,
            breaks: Vec::new(),
        });

        // make Err(()) returns after pop
        let checked = (|| {
            let (body, condition_diverged) = match expr {
                LoopExpr::Loop { body } => (body, false),
                LoopExpr::While { condition, body } => {
                    let diverged = self.check_expr(condition, Some(&Ty::Bool))?;
                    self.loop_stack.last_mut().unwrap().phase = LoopPhase::Body;
                    (body, diverged)
                }
            };
            self.check_block(body, Some(&Ty::Unit))?;
            Ok(condition_diverged)
        })();
        let mut frame = self.loop_stack.pop().expect("loop frame is present");
        let condition_diverged = checked?;

        // make sure LUB coercion follows source order.
        frame.breaks.sort_by_key(|value| value.span.start);
        for value in &frame.breaks {
            self.ctx
                .record_node_jump(value.source, JumpTarget::Break(frame.id));
        }

        let ty = if matches!(kind, LoopKind::While) {
            Ty::Unit
        } else if frame.breaks.is_empty() {
            Ty::Never
        } else if let Some(expected) = expected {
            expected.clone()
        } else {
            let types = frame
                .breaks
                .iter()
                .map(|value| match value.operand {
                    Some(operand) => self.ctx.node_ty(operand).expect("break value has a type"),
                    None => &Ty::Unit,
                })
                .collect();
            let Some(target) = lub_coerce(types) else {
                self.diagnostics.push(Diagnostic::new(
                    "incompatible break value types",
                    span.clone(),
                ));
                return Err(());
            };
            for value in &frame.breaks {
                if let Some(operand) = value.operand {
                    self.coerce_node(operand, &target, value.span.clone())?;
                }
            }
            target
        };

        let diverged = match kind {
            LoopKind::Loop => frame.breaks.iter().all(|value| value.operand_diverged),
            LoopKind::While => condition_diverged,
        };
        self.ctx.record_node_ty(id, ty);
        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }
        Ok(diverged)
    }

    fn check_break(
        &mut self,
        value: Option<&Expr>,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let Some(frame) = self.loop_stack.last() else {
            self.diagnostics
                .push(Diagnostic::new("break outside a loop", span.clone()));
            return Err(());
        };
        if matches!(frame.phase, LoopPhase::Condition) {
            self.diagnostics.push(Diagnostic::new(
                "break cannot target a loop from its while condition",
                span.clone(),
            ));
            return Err(());
        }
        if matches!(frame.kind, LoopKind::While) && value.is_some() {
            self.diagnostics.push(Diagnostic::new(
                "while loops cannot accept break values",
                span.clone(),
            ));
            return Err(());
        }

        let break_expected = frame.expected.clone();
        if let (None, Some(target)) = (value, break_expected.as_ref()) {
            if plan_coercion(&Ty::Unit, target).is_err() {
                self.diagnostics.push(Diagnostic::new(
                    format!("expected {:?}, found {:?}", target, Ty::Unit),
                    span.clone(),
                ));
                return Err(());
            }
        }

        let operand_diverged = if let Some(value) = value {
            self.check_expr(value, break_expected.as_ref())?
        } else {
            false
        };

        self.loop_stack.last_mut().unwrap().breaks.push(BreakValue {
            source: id,
            operand: value.map(|value| value.id),
            operand_diverged,
            span: span.clone(),
        });
        self.ctx.record_node_ty(id, Ty::Never);
        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }
        Ok(true)
    }

    fn check_continue(
        &mut self,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let Some(frame) = self.loop_stack.last() else {
            self.diagnostics
                .push(Diagnostic::new("continue outside a loop", span.clone()));
            return Err(());
        };
        if matches!(frame.phase, LoopPhase::Condition) {
            self.diagnostics.push(Diagnostic::new(
                "continue cannot target a loop from its while condition",
                span.clone(),
            ));
            return Err(());
        }

        self.ctx
            .record_node_jump(id, JumpTarget::Continue(frame.id));
        self.ctx.record_node_ty(id, Ty::Never);
        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }
        Ok(true)
    }

    fn check_if(
        &mut self,
        expr: &IfExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let condition_diverged = self.check_expr(&expr.condition, Some(&Ty::Bool))?;

        let mut arm_diverged = self.check_expr(
            &expr.then_branch,
            if expr.else_branch.is_some() {
                expected
            } else {
                Some(&Ty::Unit)
            },
        )?;
        if let Some(else_branch) = &expr.else_branch {
            arm_diverged &= self.check_expr(else_branch, expected)?;
        } else {
            arm_diverged = false;
        };

        let ty = if expr.else_branch.is_none() {
            Ty::Unit
        } else if let Some(expected) = expected {
            expected.clone()
        } else {
            if let Some(ty) = lub_coerce(vec![
                self.ctx
                    .node_value_ty(expr.then_branch.id)
                    .expect("type is resolved"),
                self.ctx
                    .node_value_ty(expr.else_branch.as_ref().unwrap().id)
                    .expect("type is resolved"),
            ]) {
                self.coerce_node(expr.then_branch.id, &ty, expr.then_branch.span.clone())?;
                self.coerce_node(
                    expr.else_branch.as_ref().unwrap().id,
                    &ty,
                    expr.else_branch.as_ref().unwrap().span.clone(),
                )?;
                ty
            } else {
                self.diagnostics.push(Diagnostic::new(
                    "incompatible type for if arms",
                    span.clone(),
                ));
                return Err(());
            }
        };

        self.ctx.record_node_ty(id, ty);
        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }

        Ok(condition_diverged || arm_diverged)
    }

    fn check_return(
        &mut self,
        value: Option<&Expr>,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        if let Some(value) = value {
            self.check_expr(
                value,
                Some(&self.current_function.as_ref().unwrap().return_ty.clone()),
            )?;
        } else {
            let return_ty = &self.current_function.as_ref().unwrap().return_ty;
            if plan_coercion(&Ty::Unit, return_ty).is_err() {
                self.diagnostics
                    .push(Diagnostic::new("incompatible return type", span.clone()));
                return Err(());
            };
        };

        self.ctx.record_node_jump(
            id,
            JumpTarget::Return(self.current_function.as_ref().unwrap().id),
        );
        self.ctx.record_node_ty(id, Ty::Never);
        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }
        Ok(true)
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
}

fn lub_coerce(types: Vec<&Ty>) -> Option<Ty> {
    let mut target = types.first()?;

    for (index, ty) in types.iter().enumerate().skip(1) {
        if plan_coercion(ty, target).is_ok() {
            continue;
        }

        if plan_coercion(target, ty).is_err()
            || types[..index]
                .iter()
                .any(|earlier| plan_coercion(earlier, ty).is_err())
        {
            return None;
        }

        target = ty;
    }

    Some((*target).clone())
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
