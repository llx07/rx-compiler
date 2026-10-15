use std::collections::{HashMap, HashSet};

use crate::{
    diagnostic::Diagnostic,
    frontend::ast::{
        AssociatedItemKind, AstNodeId, ConstItem, ConstValue, ConstValueKind, Crate, Expr,
        ExprKind, IntSuffix, ItemKind, LiteralExpr, Path, PathIdent, Struct, TyAstKind,
    },
    semantic::{
        ctx::{
            BuiltinType, DefId, FnSig, SemanticCtx, Ty, TypeDef, TypeId, TypedConst, TypedConstKind,
        },
        visitor::{Visitor, walk_expr, walk_function, walk_path, walk_struct, walk_struct_field},
    },
};

pub struct TypeResolver<'a> {
    ctx: &'a mut SemanticCtx,
    diagnostics: &'a mut Vec<Diagnostic>,

    const_items: HashMap<AstNodeId, &'a ConstItem>,

    resolving_consts: HashSet<AstNodeId>,
    resolving_structs: HashMap<TypeId, VisitState>,
}

enum VisitState {
    Visiting,
    Done,
}

fn parse_int_literal(raw: &str) -> Option<i64> {
    let (radix, digits) = if let Some(digits) = raw.strip_prefix("0b") {
        (2, digits)
    } else if let Some(digits) = raw.strip_prefix("0o") {
        (8, digits)
    } else if let Some(digits) = raw.strip_prefix("0x") {
        (16, digits)
    } else {
        (10, raw)
    };

    let digits: String = digits.chars().filter(|&c| c != '_').collect();
    i64::from_str_radix(&digits, radix).ok()
}

fn inline_struct_type(ty: &Ty) -> Option<TypeId> {
    match ty {
        Ty::Struct(id) => Some(*id),
        Ty::Array { elem, .. } => inline_struct_type(elem),
        _ => None,
    }
}

impl Visitor for TypeResolver<'_> {
    fn visit_struct(&mut self, node: &Struct) {
        walk_struct(self, node);

        let def_id = self
            .ctx
            .node_def(node.id)
            .expect("struct must be bound in def_collect");
        for field in &node.fields {
            let Some(ty) = self.ctx.node_ty(field.id).cloned() else {
                continue;
            };
            let def = def_id
                .as_struct_def_mut(self.ctx)
                .expect("struct definition must have struct fields");
            let field_id = def
                .fields
                .lookup(&field.name.name)
                .expect("collected field must exist");
            def.fields.get_mut(field_id).ty = Some(ty);
        }
    }

    fn visit_path(&mut self, node: &Path) {
        if let Err(diag) = self.check_expression_path_args(node) {
            self.diagnostics.push(diag);
        }
        walk_path(self, node);
    }

    fn visit_struct_field(&mut self, node: &crate::frontend::ast::StructField) {
        walk_struct_field(self, node);
        if let Some(ty) = self.ctx.node_ty(node.ty.id) {
            self.ctx.record_node_ty(node.id, ty.clone());
        }
    }

    fn visit_ty(&mut self, node: &crate::frontend::ast::TyAst) {
        if let Err(diag) = self.resolve_ty(node) {
            self.diagnostics.push(diag);
        }
    }

    fn visit_const_item(&mut self, node: &ConstItem) {
        if let Err(diag) = self.resolve_const_item(node) {
            self.diagnostics.push(diag);
        }
    }

    fn visit_const_value(&mut self, node: &ConstValue) {
        if let Err(diag) = self.resolve_const_value(node, &TypedConstKind::Usize) {
            self.diagnostics.push(diag);
        }
    }

    fn visit_expr(&mut self, node: &Expr) {
        if let ExprKind::Literal(literal) = &node.kind {
            let value = match literal {
                LiteralExpr::Int(literal) => parse_int_literal(&literal.raw),
                LiteralExpr::Bool(value) => Some(i64::from(*value)),
            };
            if let Some(value) = value {
                self.ctx.record_node_literal(node.id, value);
            } else {
                self.diagnostics.push(Diagnostic::new(
                    "integer literal is invalid or out of range for i64",
                    node.span.clone(),
                ));
            }
        }
        walk_expr(self, node);
    }

    fn visit_function(&mut self, node: &crate::frontend::ast::Function) {
        walk_function(self, node);

        let Some(params) = node
            .params
            .iter()
            .map(|param| self.ctx.node_ty(param.ty.id).cloned())
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };

        let Some(result) = self.ctx.node_ty(node.return_ty.id).cloned() else {
            return;
        };

        for (param, ty) in node.params.iter().zip(&params) {
            self.ctx.record_node_ty(param.id, ty.clone());
        }

        let def_id = self.ctx.node_def(node.id).expect("function is collected");
        let receiver = node.receiver.as_ref().map(|receiver| {
            let DefId::AssociatedValue { owner, .. } = def_id else {
                unreachable!("receiver must belong to an impl");
            };
            let ty = Ty::Struct(owner);
            let ty = if receiver.is_ref {
                Ty::Ref {
                    is_mut: receiver.is_mut,
                    inner: Box::new(ty),
                }
            } else {
                ty
            };
            self.ctx.record_node_ty(receiver.id, ty.clone());
            ty
        });

        let fn_def = def_id
            .as_function_def_mut(self.ctx)
            .expect("collected function must have a function definition");
        fn_def.sig = Some(FnSig {
            receiver,
            params,
            result,
        });
    }
}

impl<'a> TypeResolver<'a> {
    pub fn new(
        ast: &'a Crate,
        ctx: &'a mut SemanticCtx,
        diagnostics: &'a mut Vec<Diagnostic>,
    ) -> Self {
        let mut const_items = HashMap::new();
        for item in &ast.items {
            match &item.kind {
                ItemKind::Const(constant) => {
                    const_items.insert(constant.id, constant);
                }
                ItemKind::Impl(implementation) => {
                    for item in &implementation.items {
                        if let AssociatedItemKind::ConstItem(constant) = &item.kind {
                            const_items.insert(constant.id, constant);
                        }
                    }
                }
                _ => {}
            }
        }

        Self {
            ctx,
            diagnostics,
            const_items,
            resolving_consts: HashSet::new(),
            resolving_structs: HashMap::new(),
        }
    }

    fn check_expression_path_args(&self, path: &Path) -> Result<(), Diagnostic> {
        let (first, member) = match path.segments.as_slice() {
            [value] => {
                if !value.generic_args.is_empty() {
                    return Err(Diagnostic::new(
                        "value does not accept type arguments",
                        value.span.clone(),
                    ));
                }
                return Ok(());
            }
            [first, member] => (first, member),
            _ => return Ok(()), // Name resolution reports invalid path lengths.
        };

        if !member.generic_args.is_empty() {
            return Err(Diagnostic::new(
                "associated item does not accept type arguments",
                member.span.clone(),
            ));
        }

        let expected_args: Option<usize> = match &first.name {
            PathIdent::Ident(name) => {
                self.ctx
                    .lookup_type_def(&name.name)
                    .map(|id| match self.ctx.get_type_def(id) {
                        TypeDef::Builtin(def) => def.kind.type_arg_count(),
                        TypeDef::Struct(_) => 0,
                    })
            }
            PathIdent::SelfType => Some(0),
            PathIdent::SelfValue => None,
        };
        if let Some(expected_args) = expected_args {
            if first.generic_args.len() != expected_args {
                return Err(Diagnostic::new(
                    format!("Type requires {expected_args} type argument(s)"),
                    first.span.clone(),
                ));
            }
        }
        Ok(())
    }

    fn resolve_ty(&mut self, node: &crate::frontend::ast::TyAst) -> Result<Ty, Diagnostic> {
        let ty = match &node.kind {
            TyAstKind::Unit => Ty::Unit,
            TyAstKind::Ref(reference) => {
                let inner = self.resolve_ty(&reference.ty)?;
                Ty::Ref {
                    is_mut: reference.is_mut,
                    inner: Box::new(inner),
                }
            }
            TyAstKind::Array(array) => {
                let elem = self.resolve_ty(&array.ty)?;
                let len = self
                    .resolve_const_value(&array.len, &TypedConstKind::Usize)?
                    .value as usize;
                Ty::Array {
                    elem: Box::new(elem),
                    len,
                }
            }
            TyAstKind::Path(path) => {
                let [segment] = path.segments.as_slice() else {
                    return Err(Diagnostic::new("Unknown type", path.span.clone()));
                };
                let DefId::Type(type_id) = self
                    .ctx
                    .node_def(node.id)
                    .ok_or_else(|| Diagnostic::new("Unknown type", path.span.clone()))?
                else {
                    return Err(Diagnostic::new("Unknown type", path.span.clone()));
                };

                let builtin = match self.ctx.get_type_def(type_id) {
                    TypeDef::Builtin(def) => Some(def.kind),
                    TypeDef::Struct(_) => None,
                };
                let expected_args = match builtin {
                    Some(kind) => kind.type_arg_count(),
                    None => 0,
                };
                if segment.generic_args.len() != expected_args {
                    return Err(Diagnostic::new(
                        format!("Type requires {expected_args} type argument(s)"),
                        segment.span.clone(),
                    ));
                }

                match builtin {
                    Some(BuiltinType::I32) => Ty::I32,
                    Some(BuiltinType::U32) => Ty::U32,
                    Some(BuiltinType::ISize) => Ty::ISize,
                    Some(BuiltinType::USize) => Ty::USize,
                    Some(BuiltinType::Bool) => Ty::Bool,
                    Some(BuiltinType::Box) => {
                        Ty::Box(Box::new(self.resolve_ty(&segment.generic_args[0])?))
                    }
                    Some(BuiltinType::Vec) => {
                        Ty::Vec(Box::new(self.resolve_ty(&segment.generic_args[0])?))
                    }
                    None => Ty::Struct(type_id),
                }
            }
        };
        self.ctx.record_node_ty(node.id, ty.clone());
        Ok(ty)
    }

    fn resolve_const_value(
        &mut self,
        node: &crate::frontend::ast::ConstValue,
        expected: &TypedConstKind,
    ) -> Result<TypedConst, Diagnostic> {
        let resolved = match &node.kind {
            ConstValueKind::Int(literal) => {
                if matches!(expected, TypedConstKind::Bool) {
                    return Err(Diagnostic::new(
                        "expected a constant bool, got an integer",
                        node.span.clone(),
                    ));
                }

                let expected_suffix = match expected {
                    TypedConstKind::I32 => IntSuffix::I32,
                    TypedConstKind::U32 => IntSuffix::U32,
                    TypedConstKind::Isize => IntSuffix::ISize,
                    TypedConstKind::Usize => IntSuffix::USize,
                    TypedConstKind::Bool => unreachable!(),
                };

                if literal
                    .suffix
                    .as_ref()
                    .is_some_and(|suffix| *suffix != expected_suffix)
                {
                    return Err(Diagnostic::new(
                        "type mismatch for integer constant",
                        node.span.clone(),
                    ));
                }

                let value = parse_int_literal(&literal.raw).ok_or_else(|| {
                    Diagnostic::new(
                        "integer literal is invalid or out of range for i64",
                        node.span.clone(),
                    )
                })?;

                Ok(TypedConst {
                    kind: *expected,
                    value,
                })
            }
            ConstValueKind::Bool(value) => {
                if !matches!(expected, TypedConstKind::Bool) {
                    return Err(Diagnostic::new(
                        "expected an integer constant, got a bool",
                        node.span.clone(),
                    ));
                }
                Ok(TypedConst {
                    kind: TypedConstKind::Bool,
                    value: i64::from(*value),
                })
            }
            ConstValueKind::Neg(inner) => {
                if !matches!(expected, TypedConstKind::I32 | TypedConstKind::Isize) {
                    return Err(Diagnostic::new(
                        "negation requires a signed integer constant",
                        node.span.clone(),
                    ));
                }
                Ok(TypedConst {
                    kind: *expected,
                    value: -self.resolve_const_value(inner, expected)?.value,
                })
            }
            ConstValueKind::Path(path) => {
                self.check_expression_path_args(path)?;
                // Name resolution binds the ConstValue node, not its nested Path node.
                let def = self.ctx.node_def(node.id).ok_or_else(|| {
                    Diagnostic::new("unresolved constant path", path.span.clone())
                })?;

                let value_def = def
                    .as_value_def(self.ctx)
                    .ok_or_else(|| Diagnostic::new("path must be a constant", path.span.clone()))?;
                if !value_def.is_const() {
                    return Err(Diagnostic::new(
                        "path must be a constant",
                        path.span.clone(),
                    ));
                }
                let ast_node_id = value_def.ast_node_id.ok_or_else(|| {
                    Diagnostic::new("constant has no AST node", path.span.clone())
                })?;
                let item = *self.const_items.get(&ast_node_id).ok_or_else(|| {
                    Diagnostic::new("constant AST node not found", path.span.clone())
                })?;
                let resolved = self.resolve_const_item(item)?;
                if resolved.kind != *expected {
                    return Err(Diagnostic::new(
                        "type mismatch for constant path",
                        path.span.clone(),
                    ));
                }
                Ok(resolved)
            }
        }?;
        self.ctx.record_node_const(node.id, resolved);
        Ok(resolved)
    }

    pub fn resolve_const_item(&mut self, item: &ConstItem) -> Result<TypedConst, Diagnostic> {
        if let Some(value) = self.ctx.node_const(item.id) {
            return Ok(value);
        }
        if !self.resolving_consts.insert(item.id) {
            return Err(Diagnostic::new(
                "cyclic constant dependency",
                item.span.clone(),
            ));
        }

        let result = (|| {
            let ty = self.resolve_ty(&item.ty)?;
            let kind = match ty {
                Ty::I32 => TypedConstKind::I32,
                Ty::U32 => TypedConstKind::U32,
                Ty::ISize => TypedConstKind::Isize,
                Ty::USize => TypedConstKind::Usize,
                Ty::Bool => TypedConstKind::Bool,
                _ => {
                    return Err(Diagnostic::new(
                        "constant type must be an integer or bool",
                        item.ty.span.clone(),
                    ));
                }
            };
            self.ctx.record_node_ty(item.id, ty);
            self.resolve_const_value(&item.value, &kind)
        })();

        self.resolving_consts.remove(&item.id);
        if let Ok(value) = result {
            self.ctx.record_node_const(item.id, value);
        }
        result
    }

    fn check_recursive_structs_impl(&mut self, type_id: TypeId) {
        if self
            .resolving_structs
            .get(&type_id)
            .is_some_and(|x| matches!(x, VisitState::Done))
        {
            return;
        }
        if self
            .resolving_structs
            .get(&type_id)
            .is_some_and(|x| matches!(x, VisitState::Visiting))
        {
            let TypeDef::Struct(def) = self.ctx.get_type_def(type_id) else {
                unreachable!("recursive layout checks must target structs");
            };
            self.diagnostics.push(Diagnostic::new(
                "recursive struct definition",
                def.span.clone(),
            ));
            return;
        }
        self.resolving_structs.insert(type_id, VisitState::Visiting);

        let TypeDef::Struct(def) = self.ctx.get_type_def(type_id) else {
            unreachable!("recursive layout checks must target structs");
        };
        let targets: Vec<_> = def
            .fields
            .iter()
            .filter_map(|(_, field)| {
                let ty = field.ty.as_ref().expect("field is resolved");
                inline_struct_type(ty)
            })
            .collect();

        for target in targets {
            self.check_recursive_structs_impl(target);
        }

        self.resolving_structs.insert(type_id, VisitState::Done);
    }

    fn check_recursive_structs(&mut self) {
        let ids: Vec<_> = self
            .ctx
            .iter_types()
            .filter_map(|(id, def)| matches!(def, TypeDef::Struct(_)).then_some(id))
            .collect();
        for id in ids {
            self.check_recursive_structs_impl(id);
        }
    }
}

pub fn type_resolve(ast: &Crate, ctx: &mut SemanticCtx, diagnostics: &mut Vec<Diagnostic>) {
    let mut type_res = TypeResolver::new(ast, ctx, diagnostics);
    type_res.visit_crate(ast);

    if !type_res.diagnostics.is_empty() {
        return;
    }

    type_res.check_recursive_structs();
}
