use crate::frontend::ast::{
    ArrayExpr, AssociatedItem, AssociatedItemKind, Binding, BlockExpr, ConstItem, ConstValue,
    ConstValueKind, Crate, Derive, ElseBranch, Expr, ExprKind, ExprStmt, Function, Ident, Impl,
    Item, ItemKind, LetStmt, LoopExpr, Param, Path, PathIdent, PathSegment, Receiver, Stmt,
    StmtKind, Struct, StructExprField, StructField, TyAst, TyAstKind,
};

pub trait Visitor {
    fn visit_crate(&mut self, node: &Crate) {
        walk_crate(self, node);
    }

    fn visit_item(&mut self, node: &Item) {
        walk_item(self, node);
    }

    fn visit_function(&mut self, node: &Function) {
        walk_function(self, node);
    }

    fn visit_param(&mut self, node: &Param) {
        walk_param(self, node);
    }

    fn visit_struct(&mut self, node: &Struct) {
        walk_struct(self, node);
    }

    fn visit_struct_field(&mut self, node: &StructField) {
        walk_struct_field(self, node);
    }

    fn visit_const_item(&mut self, node: &ConstItem) {
        walk_const_item(self, node);
    }

    fn visit_const_value(&mut self, node: &ConstValue) {
        walk_const_value(self, node);
    }

    fn visit_impl(&mut self, node: &Impl) {
        walk_impl(self, node);
    }

    fn visit_associated_item(&mut self, node: &AssociatedItem) {
        walk_associated_item(self, node);
    }

    fn visit_stmt(&mut self, node: &Stmt) {
        walk_stmt(self, node);
    }

    fn visit_let_stmt(&mut self, node: &LetStmt) {
        walk_let_stmt(self, node);
    }

    fn visit_expr_stmt(&mut self, node: &ExprStmt) {
        walk_expr_stmt(self, node);
    }

    fn visit_ty(&mut self, node: &TyAst) {
        walk_ty(self, node);
    }

    fn visit_expr(&mut self, node: &Expr) {
        walk_expr(self, node);
    }

    fn visit_path(&mut self, node: &Path) {
        walk_path(self, node);
    }

    fn visit_path_segment(&mut self, node: &PathSegment) {
        walk_path_segment(self, node);
    }

    fn visit_block(&mut self, node: &BlockExpr) {
        walk_block(self, node);
    }

    fn visit_struct_expr_field(&mut self, node: &StructExprField) {
        walk_struct_expr_field(self, node);
    }
}

pub fn walk_crate<V: Visitor + ?Sized>(v: &mut V, krate: &Crate) {
    for item in &krate.items {
        v.visit_item(item);
    }
}

pub fn walk_item<V: Visitor + ?Sized>(v: &mut V, item: &Item) {
    match &item.kind {
        ItemKind::Function(f) => v.visit_function(f),
        ItemKind::Struct(s) => v.visit_struct(s),
        ItemKind::Const(c) => v.visit_const_item(c),
        ItemKind::Impl(i) => v.visit_impl(i),
    }
}

pub fn walk_function<V: Visitor + ?Sized>(v: &mut V, function: &Function) {
    for param in &function.params {
        v.visit_param(param);
    }
    v.visit_ty(&function.return_ty);
    v.visit_block(&function.body);
}

pub fn walk_param<V: Visitor + ?Sized>(v: &mut V, param: &Param) {
    v.visit_ty(&param.ty);
}

pub fn walk_struct<V: Visitor + ?Sized>(v: &mut V, structure: &Struct) {
    for field in &structure.fields {
        v.visit_struct_field(field);
    }
}

pub fn walk_struct_field<V: Visitor + ?Sized>(v: &mut V, field: &StructField) {
    v.visit_ty(&field.ty);
}

pub fn walk_const_item<V: Visitor + ?Sized>(v: &mut V, item: &ConstItem) {
    v.visit_ty(&item.ty);
    v.visit_const_value(&item.value);
}

pub fn walk_const_value<V: Visitor + ?Sized>(v: &mut V, value: &ConstValue) {
    match &value.kind {
        ConstValueKind::Int(_) | ConstValueKind::Bool(_) => {}
        ConstValueKind::Path(path) => v.visit_path(path),
        ConstValueKind::Neg(value) => v.visit_const_value(value),
    }
}

pub fn walk_impl<V: Visitor + ?Sized>(v: &mut V, implementation: &Impl) {
    v.visit_ty(&implementation.ty);
    for item in &implementation.items {
        v.visit_associated_item(item);
    }
}

pub fn walk_associated_item<V: Visitor + ?Sized>(v: &mut V, item: &AssociatedItem) {
    match &item.kind {
        AssociatedItemKind::ConstItem(c) => v.visit_const_item(c),
        AssociatedItemKind::Function(f) => v.visit_function(f),
    }
}

pub fn walk_stmt<V: Visitor + ?Sized>(v: &mut V, stmt: &Stmt) {
    match &stmt.kind {
        StmtKind::Let(stmt) => v.visit_let_stmt(stmt),
        StmtKind::Expr(stmt) => v.visit_expr_stmt(stmt),
    }
}

pub fn walk_let_stmt<V: Visitor + ?Sized>(v: &mut V, stmt: &LetStmt) {
    if let Some(ty) = &stmt.ty {
        v.visit_ty(ty);
    }
    v.visit_expr(&stmt.expr);
}

pub fn walk_expr_stmt<V: Visitor + ?Sized>(v: &mut V, stmt: &ExprStmt) {
    v.visit_expr(&stmt.expr);
}

pub fn walk_ty<V: Visitor + ?Sized>(v: &mut V, ty: &TyAst) {
    match &ty.kind {
        TyAstKind::Path(path) => v.visit_path(path),
        TyAstKind::Unit => {}
        TyAstKind::Ref(reference) => v.visit_ty(&reference.ty),
        TyAstKind::Array(array) => {
            v.visit_ty(&array.ty);
            v.visit_const_value(&array.len);
        }
    }
}

pub fn walk_path<V: Visitor + ?Sized>(v: &mut V, path: &Path) {
    for segment in &path.segments {
        v.visit_path_segment(segment);
    }
}

pub fn walk_path_segment<V: Visitor + ?Sized>(v: &mut V, segment: &PathSegment) {
    for ty in &segment.generic_args {
        v.visit_ty(ty);
    }
}

pub fn walk_block<V: Visitor + ?Sized>(v: &mut V, block: &BlockExpr) {
    for stmt in &block.stmts {
        v.visit_stmt(stmt);
    }
    if let Some(tail) = &block.tail {
        v.visit_expr(tail);
    }
}

pub fn walk_struct_expr_field<V: Visitor + ?Sized>(v: &mut V, field: &StructExprField) {
    v.visit_expr(&field.value);
}

pub fn walk_expr<V: Visitor + ?Sized>(v: &mut V, expr: &Expr) {
    match &expr.kind {
        ExprKind::Literal(_) | ExprKind::Unit | ExprKind::Continue => {}
        ExprKind::Path(path) => v.visit_path(path),
        ExprKind::Block(block) => v.visit_block(block),
        ExprKind::Unary(unary) => v.visit_expr(&unary.operand),
        ExprKind::Binary(binary) => {
            v.visit_expr(&binary.lhs);
            v.visit_expr(&binary.rhs);
        }
        ExprKind::Assign(assign) => {
            v.visit_expr(&assign.lhs);
            v.visit_expr(&assign.rhs);
        }
        ExprKind::Cast(cast) => {
            v.visit_expr(&cast.expr);
            v.visit_ty(&cast.ty);
        }
        ExprKind::Array(array) => match array {
            ArrayExpr::Elements(elements) => {
                for element in elements {
                    v.visit_expr(element);
                }
            }
            ArrayExpr::Repeat { value, len } => {
                v.visit_expr(value);
                v.visit_const_value(len);
            }
        },
        ExprKind::Index(index) => {
            v.visit_expr(&index.base);
            v.visit_expr(&index.index);
        }
        ExprKind::Struct(structure) => {
            v.visit_path(&structure.path);
            for field in &structure.fields {
                v.visit_struct_expr_field(field);
            }
        }
        ExprKind::Call(call) => {
            v.visit_expr(&call.callee);
            for arg in &call.args {
                v.visit_expr(arg);
            }
        }
        ExprKind::MethodCall(call) => {
            v.visit_expr(&call.receiver);
            v.visit_path_segment(&call.method);
            for arg in &call.args {
                v.visit_expr(arg);
            }
        }
        ExprKind::Field(field) => {
            v.visit_expr(&field.base);
        }
        ExprKind::Loop(loop_expr) => match loop_expr {
            LoopExpr::Loop { body } => v.visit_block(body),
            LoopExpr::While { condition, body } => {
                v.visit_expr(condition);
                v.visit_block(body);
            }
        },
        ExprKind::Break(value) | ExprKind::Return(value) => {
            if let Some(value) = value {
                v.visit_expr(value);
            }
        }
        ExprKind::If(if_expr) => {
            v.visit_expr(&if_expr.condition);
            v.visit_block(&if_expr.then_branch);
            if let Some(branch) = &if_expr.else_branch {
                match branch {
                    ElseBranch::BlockExpr(block) => v.visit_block(block),
                    ElseBranch::IfExpr(expr) => v.visit_expr(expr),
                }
            }
        }
    }
}
