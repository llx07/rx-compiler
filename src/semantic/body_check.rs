use std::collections::HashSet;

use crate::{
    diagnostic::{Diagnostic, Span},
    frontend::ast::*,
    semantic::{visitor::Visitor, *},
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
                let adjustment = match plan_ordering(lhs_ty, rhs_ty) {
                    Ok(adjustment) => adjustment,
                    Err(message) => {
                        self.diagnostics
                            .push(Diagnostic::new(message, span.clone()));
                        return Err(());
                    }
                };
                if let Some(adjustment) = adjustment {
                    self.ctx.record_node_adjustment(expr.rhs.id, adjustment);
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
        expr: &CallExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let callee = &expr.callee;
        let not_callable = || Diagnostic::new("invalid call", callee.span.clone());
        let ExprKind::Path(path) = &callee.kind else {
            self.diagnostics.push(not_callable());
            return Err(());
        };

        let def_id = self.ctx.node_def(callee.id);
        let call = if let Some(function) = def_id.and_then(|id| id.as_function_def(self.ctx)) {
            ResolvedCall {
                target: CallTarget::User(def_id.expect("function is resolved")),
                sig: function
                    .sig
                    .clone()
                    .expect("function signature is resolved"),
            }
        } else {
            // builtin function / clone
            if def_id.is_some_and(|id: DefId| {
                !id.as_value_def(self.ctx)
                    .is_some_and(|value| value.kind == ValueDefKind::BuiltinFunction)
            }) {
                self.diagnostics.push(not_callable());
                return Err(());
            }

            let (owner_ty, name) = match path.segments.as_slice() {
                [segment] => {
                    let PathIdent::Ident(name) = &segment.name else {
                        self.diagnostics.push(not_callable());
                        return Err(());
                    };
                    (None, name.name.as_str())
                }
                [owner, member] => {
                    let owner_id = match self.ctx.node_def(owner.id) {
                        Some(DefId::Type(owner)) => owner,
                        _ => unreachable!("associated owner type is resolved"),
                    };
                    let type_arg = owner.generic_args.first().map(|arg| {
                        self.ctx
                            .node_ty(arg.id)
                            .expect("type argument is resolved")
                            .clone()
                    });
                    let owner_ty = match self.ctx.get_type_def(owner_id) {
                        TypeDef::Struct(_) => Ty::Struct(owner_id),
                        TypeDef::Builtin(def) => match def.kind {
                            BuiltinType::I32 => Ty::I32,
                            BuiltinType::U32 => Ty::U32,
                            BuiltinType::ISize => Ty::ISize,
                            BuiltinType::USize => Ty::USize,
                            BuiltinType::Bool => Ty::Bool,
                            BuiltinType::Box => Ty::Box(Box::new(
                                type_arg.clone().expect("Box has a type argument"),
                            )),
                            BuiltinType::Vec => Ty::Vec(Box::new(
                                type_arg.clone().expect("Vec has a type argument"),
                            )),
                        },
                    };
                    let PathIdent::Ident(name) = &member.name else {
                        self.diagnostics.push(not_callable());
                        return Err(());
                    };
                    (Some(owner_ty), name.name.as_str())
                }
                _ => {
                    self.diagnostics.push(not_callable());
                    return Err(());
                }
            };

            self.lookup_builtin_call(owner_ty.as_ref(), name)
                .ok_or_else(|| {
                    self.diagnostics.push(not_callable());
                })?
        };

        // Explicit associated calls pass the receiver as an ordinary argument.
        let params = call.sig.receiver.iter().chain(&call.sig.params);
        let param_count = params.clone().count();
        if expr.args.len() != param_count {
            self.diagnostics.push(Diagnostic::new(
                format!(
                    "expected {param_count} argument(s), found {}",
                    expr.args.len()
                ),
                span.clone(),
            ));
            return Err(());
        }

        let mut diverged = false;
        for (arg, param) in expr.args.iter().zip(params) {
            diverged |= self.check_expr(arg, Some(param))?;
        }

        self.ctx.record_node_ty(id, call.sig.result.clone());
        self.ctx.record_node_call(id, call);
        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }
        Ok(diverged)
    }

    fn lookup_builtin_call(&self, owner: Option<&Ty>, name: &str) -> Option<ResolvedCall> {
        let reference = |ty: &Ty, is_mut| Ty::Ref {
            is_mut,
            inner: Box::new(ty.clone()),
        };
        let (op, type_arg, receiver, params, result) = match (owner, name) {
            (None, "get_i32") => (BuiltinOp::GetI32, None, None, vec![], Ty::I32),
            (None, "print_i32") => (BuiltinOp::PrintI32, None, None, vec![Ty::I32], Ty::Unit),
            (None, "println_i32") => (BuiltinOp::PrintlnI32, None, None, vec![Ty::I32], Ty::Unit),
            (Some(ty), "clone") if self.ctx.implements_trait(ty, TraitKind::Clone) => (
                BuiltinOp::Clone,
                Some(ty.clone()),
                Some(reference(ty, false)),
                vec![],
                ty.clone(),
            ),
            (Some(ty @ Ty::Array { .. }), "len") => (
                BuiltinOp::ArrayLen,
                Some(ty.clone()),
                Some(reference(ty, false)),
                vec![],
                Ty::USize,
            ),
            (Some(ty @ Ty::Box(inner)), "new") => (
                BuiltinOp::BoxNew,
                Some(inner.as_ref().clone()),
                None,
                vec![inner.as_ref().clone()],
                ty.clone(),
            ),
            (Some(ty @ Ty::Vec(inner)), name) => {
                let (op, receiver, params, result) = match name {
                    "new" => (BuiltinOp::VecNew, None, vec![], ty.clone()),
                    "len" => (
                        BuiltinOp::VecLen,
                        Some(reference(ty, false)),
                        vec![],
                        Ty::USize,
                    ),
                    "is_empty" => (
                        BuiltinOp::VecIsEmpty,
                        Some(reference(ty, false)),
                        vec![],
                        Ty::Bool,
                    ),
                    "push" => (
                        BuiltinOp::VecPush,
                        Some(reference(ty, true)),
                        vec![inner.as_ref().clone()],
                        Ty::Unit,
                    ),
                    "remove" => (
                        BuiltinOp::VecRemove,
                        Some(reference(ty, true)),
                        vec![Ty::USize],
                        inner.as_ref().clone(),
                    ),
                    _ => return None,
                };
                (op, Some(inner.as_ref().clone()), receiver, params, result)
            }
            _ => return None,
        };
        Some(ResolvedCall {
            target: CallTarget::Builtin { op, type_arg },
            sig: FnSig {
                receiver,
                params,
                result,
            },
        })
    }

    fn lookup_method(&self, receiver: &Ty, name: &str) -> Option<ResolvedCall> {
        let owner = match receiver {
            Ty::Ref { inner, .. } => inner.as_ref(),
            ty => ty,
        };

        if let Ty::Struct(owner_id) = owner {
            let members = self.ctx.get_type_def(*owner_id).associated_values();
            if let Some(value) = members.lookup(name) {
                let def = DefId::AssociatedValue {
                    owner: *owner_id,
                    value,
                };
                if let Some(function) = def.as_function_def(self.ctx) {
                    let sig = function.sig.as_ref().expect("method signature is resolved");
                    if sig.receiver.as_ref() == Some(receiver) {
                        return Some(ResolvedCall {
                            target: CallTarget::User(def),
                            sig: sig.clone(),
                        });
                    }
                }
            }
        }

        self.lookup_builtin_call(Some(owner), name)
            .filter(|call| call.sig.receiver.as_ref() == Some(receiver))
    }

    fn check_method_call(
        &mut self,
        expr: &MethodCallExpr,
        id: AstNodeId,
        span: &Span,
        expected: Option<&Ty>,
    ) -> Result<bool, ()> {
        let PathIdent::Ident(name) = &expr.method.name else {
            self.diagnostics.push(Diagnostic::new(
                "invalid method name",
                expr.method.span.clone(),
            ));
            return Err(());
        };

        let mut diverged = self.check_expr(&expr.receiver, None)?;
        let mut receiver_ty = self
            .ctx
            .node_value_ty(expr.receiver.id)
            .expect("receiver type is resolved")
            .clone();
        let mut place = self.ctx.node_place(expr.receiver.id).unwrap_or(PlaceInfo {
            writable: true,
            mutable_access_blocked: false,
        });
        let mut steps = Vec::new();

        let mut selected = 'lookup: loop {
            // Search T, &T, &mut T before dereferencing to the next type.
            for borrow in [None, Some(false), Some(true)] {
                let target = match borrow {
                    None => receiver_ty.clone(),
                    Some(is_mut) => Ty::Ref {
                        is_mut,
                        inner: Box::new(receiver_ty.clone()),
                    },
                };
                let Some(call) = self.lookup_method(&target, &name.name) else {
                    continue;
                };

                // Mutability is checked only after selecting a method.
                let mutable_receiver = matches!(&target, Ty::Ref { is_mut: true, .. });
                if mutable_receiver
                    && (place.mutable_access_blocked || (borrow == Some(true) && !place.writable))
                {
                    self.diagnostics.push(Diagnostic::new(
                        "cannot mutably borrow an immutable receiver",
                        expr.receiver.span.clone(),
                    ));
                    return Err(());
                }

                if let Some(is_mut) = borrow {
                    steps.push(Adjustment::Borrow { is_mut });
                }
                break 'lookup MethodCandidate {
                    call,
                    receiver_adjustment: UseAdjustment { target, steps },
                };
            }

            match receiver_ty {
                Ty::Ref { is_mut, inner } => {
                    place.mutable_access_blocked |= !is_mut;
                    place.writable = is_mut && !place.mutable_access_blocked;
                    receiver_ty = *inner;
                }
                Ty::Box(inner) => receiver_ty = *inner,
                _ => {
                    self.diagnostics.push(Diagnostic::new(
                        format!("no matching method {}", name.name),
                        expr.method.span.clone(),
                    ));
                    return Err(());
                }
            }
            steps.push(Adjustment::Deref);
        };

        if self.ctx.node_place(expr.receiver.id).is_none()
            && matches!(
                selected.receiver_adjustment.steps.as_slice(),
                [Adjustment::Borrow { .. }]
            )
        {
            self.ctx.materialize_value(expr.receiver.id);
            let mut steps = self
                .ctx
                .node_adjustment(expr.receiver.id)
                .expect("materialized receiver has an adjustment")
                .steps
                .clone();
            steps.append(&mut selected.receiver_adjustment.steps);
            selected.receiver_adjustment.steps = steps;
        }
        self.ctx
            .record_node_adjustment(expr.receiver.id, selected.receiver_adjustment);

        let call = selected.call;
        if expr.args.len() != call.sig.params.len() {
            self.diagnostics.push(Diagnostic::new(
                format!(
                    "expected {} argument(s), found {}",
                    call.sig.params.len(),
                    expr.args.len()
                ),
                span.clone(),
            ));
            return Err(());
        }
        for (arg, param) in expr.args.iter().zip(&call.sig.params) {
            diverged |= self.check_expr(arg, Some(param))?;
        }

        self.ctx.record_node_ty(id, call.sig.result.clone());
        self.ctx.record_node_call(id, call);
        if let Some(expected) = expected {
            self.coerce_node(id, expected, span.clone())?;
        }
        Ok(diverged)
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

/// Ordering preserves reference shape, except for an outer &mut T on the right
/// becoming &T. General coercions would also allow unsupported deref coercions.
fn plan_ordering(lhs: &Ty, rhs: &Ty) -> Result<Option<UseAdjustment>, String> {
    let mut scalar = lhs;
    while let Ty::Ref { inner, .. } = scalar {
        scalar = inner;
    }
    if matches!(scalar, Ty::I32 | Ty::U32 | Ty::ISize | Ty::USize | Ty::Bool) {
        if lhs == rhs {
            return Ok(None);
        }
        if let (
            Ty::Ref {
                is_mut: false,
                inner: lhs_inner,
            },
            Ty::Ref {
                is_mut: true,
                inner: rhs_inner,
            },
        ) = (lhs, rhs)
            && lhs_inner == rhs_inner
        {
            return Ok(Some(UseAdjustment {
                target: lhs.clone(),
                steps: vec![Adjustment::Deref, Adjustment::Borrow { is_mut: false }],
            }));
        }
    }
    Err(format!("cannot compare between {:?} and {:?}", lhs, rhs))
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
    if from == to || *from == Ty::Never {
        return Ok(UseAdjustment {
            target: to.clone(),
            steps: Vec::new(),
        });
    };

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
