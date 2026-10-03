use antlr4_runtime::{Node, NodeKind, ParsedFile, Token};

use crate::{
    diagnostic::{CompileResult, Diagnostic, Span},
    frontend::ast::{
        ArrayExpr, ArrayTy, AssignExpr, AssignOp, AssociatedItem, AssociatedItemKind, BinaryExpr,
        BinaryOp, Binding, BlockExpr, CallExpr, CastExpr, ConstItem, ConstValue, ConstValueKind,
        Crate, Derive, DeriveKind, ElseBranch, Expr, ExprKind, ExprStmt, FieldExpr, Function,
        Ident, IfExpr, Impl, IndexExpr, IntLiteral, IntSuffix, Item, ItemKind, LetStmt,
        LiteralExpr, LoopExpr, MethodCallExpr, Param, Path, PathIdent, PathSegment, Receiver,
        RefTy, Stmt, StmtKind, Struct, StructExpr, StructExprField, StructField, TyAst, TyAstKind,
        UnaryExpr, UnaryOp,
    },
    generated::{
        rx_lexer::{
            AMP, AMP_ASSIGN, ANDAND, BREAK, CARET, CARET_ASSIGN, CLONE, CONTINUE, COPY, EQ, EQEQ,
            FALSE, GT, GT_SECOND, INTEGER_LITERAL, LE, LOOP, LPAREN, LT, MINUS, MINUS_ASSIGN, MUT,
            NE, NOT, OROR, PARTIAL_EQ, PERCENT, PERCENT_ASSIGN, PIPE, PIPE_ASSIGN, PLUS,
            PLUS_ASSIGN, RETURN, RPAREN, SELF_TYPE, SELF_VALUE, SEMI, SHL, SHL_ASSIGN, SLASH,
            SLASH_ASSIGN, STAR, STAR_ASSIGN, TRUE, WHILE,
        },
        rx_parser::{
            self, RULE_ARRAY_TYPE, RULE_ASSOCIATED_ITEM, RULE_BLOCK_EXPRESSION,
            RULE_CALL_ARGUMENTS, RULE_CONDITION_EXPRESSION, RULE_CONST_VALUE, RULE_DERIVE_NAME,
            RULE_DOT_SUFFIX, RULE_EXPRESSION, RULE_EXPRESSION_WITH_BLOCK, RULE_FUNCTION_PARAM,
            RULE_FUNCTION_PARAMETERS, RULE_GENERIC_ARG, RULE_GENERIC_ARGS, RULE_IDENTIFIER,
            RULE_IDENTIFIER_BINDING, RULE_IF_EXPRESSION, RULE_LET_STATEMENT, RULE_OUTER_ATTRIBUTE,
            RULE_PATH_EXPR_SEGMENT, RULE_PATH_IDENT_SEGMENT, RULE_PATH_IN_EXPRESSION,
            RULE_POSTFIX_SUFFIX, RULE_REFERENCE_TYPE, RULE_SELF_PARAM, RULE_STATEMENT,
            RULE_STATEMENT_EXPRESSION, RULE_STRUCT_EXPR_FIELD, RULE_STRUCT_EXPR_FIELDS,
            RULE_STRUCT_FIELD, RULE_TYPE_PATH, RULE_TYPE_PATH_SEGMENT, RULE_TYPE_REF,
        },
    },
};

pub fn build_ast(parsed: &ParsedFile) -> CompileResult<Crate> {
    build_crate(parsed.tree())
}

fn node_span(node: Node) -> Span {
    match node.kind() {
        NodeKind::Rule => {
            let rule = node.as_rule().unwrap();

            let start_token = rule.start().expect("rule must have start token");
            let stop_token = rule.stop().expect("rule must have stop token");

            let start_span = start_token.byte_span().expect("token must have byte span");
            let stop_span = stop_token.byte_span().expect("token must have byte span");

            if start_token.token_id().index() > stop_token.token_id().index() {
                start_span.start..start_span.start
            } else {
                start_span.start..stop_span.end
            }
        }
        NodeKind::Terminal => node
            .as_terminal()
            .unwrap()
            .symbol()
            .byte_span()
            .expect("token must have byte span"),
        NodeKind::Error => unreachable!("error node must not reach AST builder"),
    }
}

fn build_crate(node: Node) -> CompileResult<Crate> {
    let mut items = Vec::new();
    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            if rule.rule_index() == rx_parser::RULE_ITEM {
                let item = build_item(child)?;
                if let Some(item) = item {
                    items.push(item);
                }
            }
        }
    }
    Ok(Crate { items })
}

fn build_item(node: Node) -> CompileResult<Option<Item>> {
    let child = node
        .children()
        .next()
        .expect("item must have exactly one child");

    let rule = child.as_rule().expect("child of item must be a rule");

    let kind = match rule.rule_index() {
        rx_parser::RULE_USE_DECLARATION => {
            return Ok(None);
        }
        rx_parser::RULE_FUNCTION_DEFINITION => ItemKind::Function(build_function(child)?),
        rx_parser::RULE_STRUCT_DEFINITION => ItemKind::Struct(build_struct(child)?),
        rx_parser::RULE_CONSTANT_ITEM => ItemKind::Const(build_const_item(child)?),
        rx_parser::RULE_INHERENT_IMPL => ItemKind::Impl(build_impl(child)?),
        _ => unreachable!("node in build_item must be a kind of item"),
    };

    Ok(Some(Item {
        kind: kind,
        span: node_span(node),
    }))
}

fn build_function(node: Node) -> CompileResult<Function> {
    let mut name = None;
    let mut receiver = None;
    let mut params = Vec::new();
    let mut return_ty = None;
    let mut default_return_span = None;
    let mut body = None;
    let mut span = node_span(node);

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_IDENTIFIER => {
                    name = Some(Ident {
                        name: rule.text(),
                        span: node_span(child),
                    });
                }
                RULE_FUNCTION_PARAMETERS => {
                    for param in child.children() {
                        if let Some(param_type) = param.as_rule() {
                            match param_type.rule_index() {
                                RULE_SELF_PARAM => {
                                    receiver = Some(build_receiver(param)?);
                                }
                                RULE_FUNCTION_PARAM => {
                                    params.push(build_param(param)?);
                                }
                                _ => unreachable!("unexpected rule in functionParameters"),
                            }
                        }
                    }
                }
                RULE_TYPE_REF => return_ty = Some(build_ty(child)?),
                RULE_BLOCK_EXPRESSION => body = Some(build_block_expr(child)?),
                _ => {}
            }
        }
        if let Some(terminal) = child.as_terminal() {
            match terminal.symbol().token_type() {
                RPAREN => {
                    let span = terminal
                        .symbol()
                        .byte_span()
                        .expect("RPAREN must have byte span");

                    default_return_span = Some(Span {
                        start: span.end,
                        end: span.end,
                    });
                }
                _ => {}
            }
        }
    }

    Ok(Function {
        name: name.expect("function must have a name"),
        receiver: receiver,
        params: params,
        return_ty: return_ty.unwrap_or_else(|| TyAst {
            kind: super::ast::TyAstKind::Unit,
            span: default_return_span.expect("function must have RPAREN"),
        }),
        body: body.expect("function declaration must have a body"),
        span: span,
    })
}

fn build_receiver(node: Node) -> CompileResult<Receiver> {
    let mut is_ref = false;
    let mut is_mut = false;

    for child in node.children() {
        if let Some(terminal) = child.as_terminal() {
            match terminal.symbol().token_type() {
                AMP => is_ref = true,
                MUT => is_mut = true,
                _ => {}
            }
        }
    }

    Ok(Receiver {
        is_ref: is_ref,
        is_mut: is_mut,
        span: node_span(node),
    })
}

fn build_param(node: Node) -> CompileResult<Param> {
    let mut binding = None;
    let mut ty = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_IDENTIFIER_BINDING => binding = Some(build_binding(child)?),
                RULE_TYPE_REF => ty = Some(build_ty(child)?),
                _ => {}
            }
        }
    }

    Ok(Param {
        binding: binding.expect("parameter must have a binding"),
        ty: ty.expect("parameter must have a type"),
        span: node_span(node),
    })
}

fn build_binding(node: Node) -> CompileResult<Binding> {
    let mut is_mut = false;
    let mut name = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_IDENTIFIER => {
                    name = Some(Ident {
                        name: rule.text(),
                        span: node_span(child),
                    });
                }
                _ => {}
            }
        }
        if let Some(terminal) = child.as_terminal() {
            match terminal.symbol().token_type() {
                MUT => is_mut = true,
                _ => {}
            }
        }
    }

    Ok(Binding {
        is_mut: is_mut,
        name: name.expect("binding must have a name"),
        span: node_span(node),
    })
}

fn build_struct(node: Node) -> CompileResult<Struct> {
    let mut name = None;
    let mut derives = Vec::new();
    let mut fields = Vec::new();

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_IDENTIFIER => {
                    name = Some(Ident {
                        name: rule.text(),
                        span: node_span(child),
                    });
                }
                RULE_OUTER_ATTRIBUTE => {
                    for derive in child.children() {
                        if let Some(derive_rule) = derive.as_rule() {
                            match derive_rule.rule_index() {
                                RULE_DERIVE_NAME => derives.push(build_derive(derive)?),
                                _ => unreachable!("unexpected rule in outerAttribute"),
                            }
                        }
                    }
                }
                RULE_STRUCT_FIELD => fields.push(build_struct_field(child)?),
                _ => {}
            }
        }
    }

    Ok(Struct {
        name: name.expect("struct must have a name"),
        derives: derives,
        fields: fields,
        span: node_span(node),
    })
}

fn build_derive(node: Node) -> CompileResult<Derive> {
    let child: Node<'_> = node
        .children()
        .next()
        .expect("derive name must have exactly one child");

    let terminal = child
        .as_terminal()
        .expect("child of derive name must be a terminal");

    let kind = match terminal.symbol().token_type() {
        COPY => DeriveKind::Copy,
        CLONE => DeriveKind::Clone,
        PARTIAL_EQ => DeriveKind::PartialEq,
        EQ => DeriveKind::Eq,
        _ => unreachable!("unexpected token in deriveName"),
    };

    Ok(Derive {
        kind: kind,
        span: node_span(node),
    })
}

fn build_struct_field(node: Node) -> CompileResult<StructField> {
    let mut name = None;
    let mut ty = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_IDENTIFIER => {
                    name = Some(Ident {
                        name: rule.text(),
                        span: node_span(child),
                    });
                }
                RULE_TYPE_REF => ty = Some(build_ty(child)?),
                _ => {}
            }
        }
    }

    Ok(StructField {
        name: name.expect("struct field must have a name"),
        ty: ty.expect("struct field must have a type"),
        span: node_span(node),
    })
}

fn build_const_item(node: Node) -> CompileResult<ConstItem> {
    let mut name = None;
    let mut ty = None;
    let mut value = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_IDENTIFIER => {
                    name = Some(Ident {
                        name: rule.text(),
                        span: node_span(child),
                    });
                }
                RULE_TYPE_REF => ty = Some(build_ty(child)?),
                RULE_CONST_VALUE => value = Some(build_const_value(child)?),
                _ => {}
            }
        }
    }

    Ok(ConstItem {
        name: name.expect("constant item must have a name"),
        ty: ty.expect("constant item must have a type"),
        value: value.expect("constant item must have a value"),
        span: node_span(node),
    })
}

fn build_const_value(node: Node) -> CompileResult<ConstValue> {
    let mut children = node.children();
    let child = children.next().expect("constant value must have a child");

    let kind = if let Some(rule) = child.as_rule() {
        match rule.rule_index() {
            RULE_PATH_IN_EXPRESSION => ConstValueKind::Path(build_path(child)?),
            _ => unreachable!("unexpected rule in constant value"),
        }
    } else {
        let terminal = child
            .as_terminal()
            .expect("child of constant value must be a rule or terminal");

        match terminal.symbol().token_type() {
            INTEGER_LITERAL => ConstValueKind::Int(build_int_literal(child)?),
            TRUE => ConstValueKind::Bool(true),
            FALSE => ConstValueKind::Bool(false),
            MINUS => {
                let magnitude = children
                    .next()
                    .expect("negated constant must have a magnitude");

                ConstValueKind::Neg(Box::new(build_const_value(magnitude)?))
            }
            LPAREN => {
                let value = children
                    .next()
                    .expect("parenthesized constant must have a value");

                build_const_value(value)?.kind
            }
            _ => unreachable!("unexpected token in constant value"),
        }
    };

    Ok(ConstValue {
        kind: kind,
        span: node_span(node),
    })
}

fn build_int_literal(node: Node) -> CompileResult<IntLiteral> {
    let terminal = node
        .as_terminal()
        .expect("integer literal must be a terminal");

    let mut raw = terminal
        .symbol()
        .text()
        .expect("integer literal must have text")
        .to_string();

    let suffix = if raw.ends_with("i32") {
        raw.truncate(raw.len() - 3);
        Some(IntSuffix::I32)
    } else if raw.ends_with("u32") {
        raw.truncate(raw.len() - 3);
        Some(IntSuffix::U32)
    } else if raw.ends_with("isize") {
        raw.truncate(raw.len() - 5);
        Some(IntSuffix::ISize)
    } else if raw.ends_with("usize") {
        raw.truncate(raw.len() - 5);
        Some(IntSuffix::USize)
    } else {
        None
    };

    Ok(IntLiteral {
        raw: raw,
        suffix: suffix,
    })
}

fn build_impl(node: Node) -> CompileResult<Impl> {
    let mut ty = None;
    let mut items = Vec::new();

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_TYPE_REF => ty = Some(build_ty(child)?),
                RULE_ASSOCIATED_ITEM => items.push(build_associated_item(child)?),
                _ => {}
            }
        }
    }

    Ok(Impl {
        ty: ty.expect("impl must have a type"),
        items: items,
        span: node_span(node),
    })
}

fn build_associated_item(node: Node) -> CompileResult<AssociatedItem> {
    let child = node
        .children()
        .next()
        .expect("associated item must have exactly one child");

    let rule = child
        .as_rule()
        .expect("child of associated item must be a rule");

    let kind = match rule.rule_index() {
        rx_parser::RULE_CONSTANT_ITEM => AssociatedItemKind::ConstItem(build_const_item(child)?),
        rx_parser::RULE_FUNCTION_DEFINITION => AssociatedItemKind::Function(build_function(child)?),
        _ => unreachable!("node in build_associated_item must be a kind of associated item"),
    };

    Ok(AssociatedItem {
        kind: kind,
        span: node_span(node),
    })
}

fn build_stmt(node: Node) -> CompileResult<Option<Stmt>> {
    let child = node.children().next().expect("statement must have a child");

    if let Some(terminal) = child.as_terminal() {
        match terminal.symbol().token_type() {
            SEMI => return Ok(None),
            _ => unreachable!("unexpected token in statement"),
        }
    }

    let rule = child
        .as_rule()
        .expect("child of statement must be a rule or SEMI");

    let kind = match rule.rule_index() {
        RULE_LET_STATEMENT => StmtKind::Let(build_let_stmt(child)?),
        RULE_EXPRESSION_WITH_BLOCK | RULE_STATEMENT_EXPRESSION => {
            StmtKind::Expr(build_expr_stmt(node)?)
        }
        _ => unreachable!("unexpected rule in statement"),
    };

    Ok(Some(Stmt {
        kind: kind,
        span: node_span(node),
    }))
}

fn build_let_stmt(node: Node) -> CompileResult<LetStmt> {
    let mut binding = None;
    let mut ty = None;
    let mut expr = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_IDENTIFIER_BINDING => binding = Some(build_binding(child)?),
                RULE_TYPE_REF => ty = Some(build_ty(child)?),
                RULE_EXPRESSION => expr = Some(build_expr(child)?),
                _ => {}
            }
        }
    }

    Ok(LetStmt {
        binding: binding.expect("let statement must have a binding"),
        ty: ty,
        expr: expr.expect("let statement must have an expression"),
        span: node_span(node),
    })
}

fn build_expr_stmt(node: Node) -> CompileResult<ExprStmt> {
    let mut expr = None;
    let mut has_semi = false;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_EXPRESSION_WITH_BLOCK | RULE_STATEMENT_EXPRESSION => {
                    expr = Some(build_expr(child)?);
                }
                _ => {}
            }
        }
        if let Some(terminal) = child.as_terminal() {
            match terminal.symbol().token_type() {
                SEMI => has_semi = true,
                _ => {}
            }
        }
    }

    Ok(ExprStmt {
        expr: expr.expect("expression statement must have an expression"),
        has_semi: has_semi,
        span: node_span(node),
    })
}

fn build_ty(node: Node) -> CompileResult<TyAst> {
    let rule = node.as_rule().expect("type must be a rule");
    if rule.rule_index() == rx_parser::RULE_CLOSED_CAST_TYPE {
        let first = node
            .children()
            .next()
            .expect("closed cast type must have a child");
        if first
            .as_terminal()
            .is_some_and(|terminal| matches!(terminal.symbol().token_type(), AMP | ANDAND))
        {
            return Ok(TyAst {
                kind: TyAstKind::Ref(build_ref_ty(node)?),
                span: node_span(node),
            });
        }

        if let Some(ident) = rule.child_rule(RULE_PATH_IDENT_SEGMENT) {
            let mut segments: Vec<PathSegment> = Vec::new();
            for segment in rule.child_rules(RULE_TYPE_PATH_SEGMENT) {
                segments.push(build_path_segment(segment.node())?);
            }
            let mut segment = build_path_segment(node)?;
            segment.span = node_span(ident.node()).start..node_span(node).end;
            segments.push(segment);

            return Ok(TyAst {
                kind: TyAstKind::Path(Path {
                    segments: segments,
                    span: node_span(node),
                }),
                span: node_span(node),
            });
        }
    }

    let mut kind = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_TYPE_REF => kind = Some(build_ty(child)?.kind),
                RULE_TYPE_PATH => kind = Some(TyAstKind::Path(build_path(child)?)),
                RULE_REFERENCE_TYPE => kind = Some(TyAstKind::Ref(build_ref_ty(child)?)),
                RULE_ARRAY_TYPE => kind = Some(TyAstKind::Array(build_array_ty(child)?)),
                _ => unreachable!("unexpected rule in typeRef"),
            }
        }
    }

    Ok(TyAst {
        kind: kind.unwrap_or(TyAstKind::Unit),
        span: node_span(node),
    })
}

fn build_ref_ty(node: Node) -> CompileResult<RefTy> {
    let mut is_mut = false;
    let mut is_double_ref = false;
    let mut ty = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_TYPE_REF | rx_parser::RULE_CLOSED_CAST_TYPE => ty = Some(build_ty(child)?),
                _ => {}
            }
        }
        if let Some(terminal) = child.as_terminal() {
            match terminal.symbol().token_type() {
                MUT => is_mut = true,
                ANDAND => is_double_ref = true,
                _ => {}
            }
        }
    }

    let ref_ty = RefTy {
        is_mut: is_mut,
        ty: Box::new(ty.expect("reference type must have a type")),
    };

    if is_double_ref {
        let span = node_span(node);

        Ok(RefTy {
            is_mut: false,
            ty: Box::new(TyAst {
                kind: TyAstKind::Ref(ref_ty),
                span: span.start + 1..span.end,
            }),
        })
    } else {
        Ok(ref_ty)
    }
}

fn build_array_ty(node: Node) -> CompileResult<ArrayTy> {
    let mut ty = None;
    let mut len = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_TYPE_REF => ty = Some(build_ty(child)?),
                RULE_CONST_VALUE => len = Some(build_const_value(child)?),
                _ => {}
            }
        }
    }

    Ok(ArrayTy {
        ty: Box::new(ty.expect("array type must have an element type")),
        len: len.expect("array type must have a length"),
    })
}

fn build_path(node: Node) -> CompileResult<Path> {
    let mut segments = Vec::new();

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_TYPE_PATH_SEGMENT | RULE_PATH_EXPR_SEGMENT => {
                    segments.push(build_path_segment(child)?);
                }
                _ => unreachable!("unexpected rule in path"),
            }
        }
    }

    Ok(Path {
        segments: segments,
        span: node_span(node),
    })
}

fn build_path_ident(node: Node) -> CompileResult<PathIdent> {
    let child = node
        .children()
        .next()
        .expect("path identifier must have exactly one child");

    let ident = if let Some(rule) = child.as_rule() {
        match rule.rule_index() {
            RULE_IDENTIFIER => PathIdent::Ident(Ident {
                name: rule.text(),
                span: node_span(child),
            }),
            _ => unreachable!("unexpected rule in pathIdentSegment"),
        }
    } else {
        let terminal = child
            .as_terminal()
            .expect("child of path identifier must be a rule or terminal");

        match terminal.symbol().token_type() {
            SELF_VALUE => PathIdent::SelfValue,
            SELF_TYPE => PathIdent::SelfType,
            _ => unreachable!("unexpected token in pathIdentSegment"),
        }
    };

    Ok(ident)
}

fn build_path_segment(node: Node) -> CompileResult<PathSegment> {
    let mut name = None;
    let mut generic_args = Vec::new();

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_PATH_IDENT_SEGMENT => name = Some(build_path_ident(child)?),
                RULE_GENERIC_ARGS => {
                    for arg in rule.child_rules(RULE_GENERIC_ARG) {
                        if let Some(ty) = arg.child_rule(RULE_TYPE_REF) {
                            generic_args.push(build_ty(ty.node())?);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(PathSegment {
        name: name.expect("path segment must have a name"),
        generic_args: generic_args,
        span: node_span(node),
    })
}

fn build_expr(node: Node) -> CompileResult<Expr> {
    let rule = node.as_rule().expect("expression must be a rule");
    let mut children = node.children();

    if let Some(child) = children.next() {
        if children.next().is_none()
            && child.as_rule().is_some()
            && rule.rule_index() != RULE_PATH_IN_EXPRESSION
        {
            let mut expr = build_expr(child)?;
            expr.span = node_span(node);
            return Ok(expr);
        }
    }

    let kind = match rule.rule_index() {
        rx_parser::RULE_LITERAL_EXPRESSION => ExprKind::Literal(build_literal_expr(node)?),
        rx_parser::RULE_PATH_IN_EXPRESSION => ExprKind::Path(build_path(node)?),
        rx_parser::RULE_ARRAY_EXPRESSION => ExprKind::Array(build_array_expr(node)?),
        rx_parser::RULE_BLOCK_EXPRESSION => ExprKind::Block(build_block_expr(node)?),
        rx_parser::RULE_IF_EXPRESSION => ExprKind::If(build_if_expr(node)?),
        rx_parser::RULE_EXPRESSION_WITH_BLOCK => ExprKind::Loop(build_loop_expr(node)?),

        rx_parser::RULE_NON_BLOCK_PRIMARY
        | rx_parser::RULE_CONDITION_PRIMARY_WITHOUT_BARE_BLOCK => build_primary_expr(node)?,

        rx_parser::RULE_POSTFIX_EXPRESSION
        | rx_parser::RULE_CONDITION_POSTFIX_EXPRESSION
        | rx_parser::RULE_CONDITION_BREAK_POSTFIX_EXPRESSION
        | rx_parser::RULE_STATEMENT_POSTFIX_EXPRESSION => build_postfix_expr(node)?,

        rx_parser::RULE_UNARY_EXPRESSION
        | rx_parser::RULE_CONDITION_UNARY_EXPRESSION
        | rx_parser::RULE_CONDITION_BREAK_UNARY_EXPRESSION
        | rx_parser::RULE_STATEMENT_UNARY_EXPRESSION => ExprKind::Unary(build_unary_expr(node)?),

        rx_parser::RULE_CAST_EXPRESSION
        | rx_parser::RULE_CLOSED_CAST_EXPRESSION
        | rx_parser::RULE_CONDITION_CAST_EXPRESSION
        | rx_parser::RULE_CONDITION_CLOSED_CAST_EXPRESSION
        | rx_parser::RULE_CONDITION_BREAK_CAST_EXPRESSION
        | rx_parser::RULE_CONDITION_BREAK_CLOSED_CAST_EXPRESSION
        | rx_parser::RULE_STATEMENT_CAST_EXPRESSION
        | rx_parser::RULE_STATEMENT_CLOSED_CAST_EXPRESSION => {
            ExprKind::Cast(build_cast_expr(node)?)
        }

        rx_parser::RULE_ASSIGNMENT_EXPRESSION
        | rx_parser::RULE_CONDITION_ASSIGNMENT_EXPRESSION
        | rx_parser::RULE_CONDITION_BREAK_ASSIGNMENT_EXPRESSION
        | rx_parser::RULE_STATEMENT_ASSIGNMENT_EXPRESSION => {
            ExprKind::Assign(build_assignment_expr(node)?)
        }

        index if is_binary_rule(index) => ExprKind::Binary(build_binary_expr(node)?),
        _ => unreachable!("unexpected rule in expression"),
    };

    Ok(Expr {
        kind: kind,
        span: node_span(node),
    })
}

fn is_binary_rule(index: usize) -> bool {
    (rx_parser::RULE_LOGICAL_OR_EXPRESSION..=rx_parser::RULE_CLOSED_MULTIPLICATIVE_EXPRESSION)
        .contains(&index)
        || (rx_parser::RULE_CONDITION_LOGICAL_OR_EXPRESSION
            ..=rx_parser::RULE_CONDITION_CLOSED_MULTIPLICATIVE_EXPRESSION)
            .contains(&index)
        || (rx_parser::RULE_CONDITION_BREAK_LOGICAL_OR_EXPRESSION
            ..=rx_parser::RULE_CONDITION_BREAK_CLOSED_MULTIPLICATIVE_EXPRESSION)
            .contains(&index)
        || (rx_parser::RULE_STATEMENT_LOGICAL_OR_EXPRESSION
            ..=rx_parser::RULE_STATEMENT_CLOSED_MULTIPLICATIVE_EXPRESSION)
            .contains(&index)
}

fn build_primary_expr(node: Node) -> CompileResult<ExprKind> {
    let rule = node.as_rule().expect("primary expression must be a rule");
    let child = node
        .children()
        .next()
        .expect("primary expression must have a child");

    if let Some(child_rule) = child.as_rule() {
        let kind = match child_rule.rule_index() {
            rx_parser::RULE_PATH_IN_EXPRESSION => ExprKind::Struct(build_struct_expr(node)?),
            _ => unreachable!("unexpected rule in primary expression"),
        };
        return Ok(kind);
    }

    let terminal = child
        .as_terminal()
        .expect("child of primary expression must be a rule or terminal");

    let kind = match terminal.symbol().token_type() {
        LPAREN => {
            if let Some(expr) = rule.child_rule(RULE_EXPRESSION) {
                build_expr(expr.node())?.kind
            } else {
                ExprKind::Unit
            }
        }
        BREAK | RETURN => {
            let expr = rule
                .children()
                .find_map(|child| child.as_rule())
                .map(|expr| build_expr(expr.node()).map(Box::new))
                .transpose()?;

            if terminal.symbol().token_type() == BREAK {
                ExprKind::Break(expr)
            } else {
                ExprKind::Return(expr)
            }
        }
        CONTINUE => ExprKind::Continue,
        LOOP | WHILE => ExprKind::Loop(build_loop_expr(node)?),
        _ => unreachable!("unexpected token in primary expression"),
    };

    Ok(kind)
}

fn build_literal_expr(node: Node) -> CompileResult<LiteralExpr> {
    let child = node
        .children()
        .next()
        .expect("literal expression must have exactly one child");

    let terminal = child
        .as_terminal()
        .expect("child of literal expression must be a terminal");

    let literal = match terminal.symbol().token_type() {
        INTEGER_LITERAL => LiteralExpr::Int(build_int_literal(child)?),
        TRUE => LiteralExpr::Bool(true),
        FALSE => LiteralExpr::Bool(false),
        _ => unreachable!("unexpected token in literalExpression"),
    };

    Ok(literal)
}

fn build_block_expr(node: Node) -> CompileResult<BlockExpr> {
    let mut stmts = Vec::new();
    let mut tail = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_STATEMENT => {
                    if let Some(stmt) = build_stmt(child)? {
                        stmts.push(stmt);
                    }
                }
                RULE_STATEMENT_EXPRESSION => tail = Some(Box::new(build_expr(child)?)),
                _ => {}
            }
        }
    }

    if tail.is_none()
        && matches!(stmts.last(), Some(Stmt { kind: StmtKind::Expr(expr), .. }) if !expr.has_semi)
    {
        let stmt = stmts
            .pop()
            .expect("block must have a final expression statement");
        let StmtKind::Expr(expr) = stmt.kind else {
            unreachable!("final statement must be an expression");
        };
        tail = Some(Box::new(expr.expr));
    }

    Ok(BlockExpr {
        stmts: stmts,
        tail: tail,
        span: node_span(node),
    })
}

fn build_array_expr(node: Node) -> CompileResult<ArrayExpr> {
    let mut elements = Vec::new();
    let mut len = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_EXPRESSION => elements.push(build_expr(child)?),
                RULE_CONST_VALUE => len = Some(build_const_value(child)?),
                _ => {}
            }
        }
    }

    if let Some(len) = len {
        Ok(ArrayExpr::Repeat {
            value: Box::new(elements.pop().expect("repeat array must have a value")),
            len: len,
        })
    } else {
        Ok(ArrayExpr::Elements(elements))
    }
}

fn build_struct_expr(node: Node) -> CompileResult<StructExpr> {
    let mut path = None;
    let mut fields = Vec::new();

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_PATH_IN_EXPRESSION => path = Some(build_path(child)?),
                RULE_STRUCT_EXPR_FIELDS => {
                    for field in rule.child_rules(RULE_STRUCT_EXPR_FIELD) {
                        fields.push(build_struct_expr_field(field.node())?);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(StructExpr {
        path: path.expect("struct expression must have a path"),
        fields: fields,
    })
}

fn build_struct_expr_field(node: Node) -> CompileResult<StructExprField> {
    let mut name = None;
    let mut value = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_IDENTIFIER => {
                    name = Some(Ident {
                        name: rule.text(),
                        span: node_span(child),
                    });
                }
                RULE_EXPRESSION => value = Some(build_expr(child)?),
                _ => {}
            }
        }
    }

    Ok(StructExprField {
        name: name.expect("struct expression field must have a name"),
        value: value.expect("struct expression field must have a value"),
    })
}

fn build_loop_expr(node: Node) -> CompileResult<LoopExpr> {
    let child = node
        .children()
        .next()
        .expect("loop expression must have a keyword");

    let terminal = child
        .as_terminal()
        .expect("first child of loop expression must be a terminal");

    let mut condition = None;
    let mut body = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_CONDITION_EXPRESSION => condition = Some(build_expr(child)?),
                RULE_BLOCK_EXPRESSION => body = Some(build_block_expr(child)?),
                _ => {}
            }
        }
    }

    let body = body.expect("loop expression must have a body");

    let expr = match terminal.symbol().token_type() {
        LOOP => LoopExpr::Loop { body: body },
        WHILE => LoopExpr::While {
            condition: Box::new(condition.expect("while expression must have a condition")),
            body: body,
        },
        _ => unreachable!("unexpected token in loop expression"),
    };

    Ok(expr)
}

fn build_if_expr(node: Node) -> CompileResult<IfExpr> {
    let mut condition = None;
    let mut then_branch = None;
    let mut else_branch = None;

    for child in node.children() {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_CONDITION_EXPRESSION => condition = Some(build_expr(child)?),
                RULE_BLOCK_EXPRESSION => {
                    let block = build_block_expr(child)?;
                    if then_branch.is_none() {
                        then_branch = Some(block);
                    } else {
                        else_branch = Some(ElseBranch::BlockExpr(block));
                    }
                }
                RULE_IF_EXPRESSION => {
                    else_branch = Some(ElseBranch::IfExpr(Box::new(build_if_expr(child)?)));
                }
                _ => {}
            }
        }
    }

    Ok(IfExpr {
        condition: Box::new(condition.expect("if expression must have a condition")),
        then_branch: then_branch.expect("if expression must have a then branch"),
        else_branch: else_branch,
        span: node_span(node),
    })
}

fn build_postfix_expr(node: Node) -> CompileResult<ExprKind> {
    let mut children = node.children();
    let base = children
        .next()
        .expect("postfix expression must have a base");

    let mut expr = build_expr(base)?;
    for suffix in children {
        expr = build_postfix_suffix(suffix, expr)?;
    }

    Ok(expr.kind)
}

fn build_postfix_suffix(node: Node, base: Expr) -> CompileResult<Expr> {
    let span = base.span.start..node_span(node).end;
    let rule = node.as_rule().expect("postfix suffix must be a rule");

    let suffix = if rule.rule_index() == RULE_POSTFIX_SUFFIX {
        let child = node
            .children()
            .next()
            .expect("postfix suffix must have a child");

        if child.as_rule().is_some() {
            child
        } else {
            node
        }
    } else {
        // for dotSuffix in statementPostfixExpression
        node
    };

    let rule = suffix.as_rule().expect("postfix suffix must be a rule");

    let kind = match rule.rule_index() {
        RULE_CALL_ARGUMENTS => ExprKind::Call(CallExpr {
            callee: Box::new(base),
            args: build_call_args(suffix)?,
        }),
        RULE_POSTFIX_SUFFIX => {
            let index = rule
                .child_rule(RULE_EXPRESSION)
                .expect("index suffix must have an expression");

            ExprKind::Index(IndexExpr {
                base: Box::new(base),
                index: Box::new(build_expr(index.node())?),
            })
        }
        RULE_DOT_SUFFIX => {
            if let Some(method) = rule.child_rule(RULE_PATH_EXPR_SEGMENT) {
                let args = rule
                    .child_rule(RULE_CALL_ARGUMENTS)
                    .expect("method call must have arguments");

                let method = build_path_segment(method.node())?;
                if let Some(ty) = method.generic_args.first() {
                    return Err(Diagnostic::new(
                        "method calls do not support type arguments",
                        ty.span.clone(),
                    ));
                }

                ExprKind::MethodCall(MethodCallExpr {
                    receiver: Box::new(base),
                    method: method,
                    args: build_call_args(args.node())?,
                })
            } else {
                let field = rule
                    .child_rule(RULE_IDENTIFIER)
                    .expect("field access must have an identifier");

                ExprKind::Field(FieldExpr {
                    base: Box::new(base),
                    field: Ident {
                        name: field.text(),
                        span: node_span(field.node()),
                    },
                })
            }
        }
        _ => unreachable!("unexpected rule in postfix suffix"),
    };

    Ok(Expr {
        kind: kind,
        span: span,
    })
}

fn build_call_args(node: Node) -> CompileResult<Vec<Expr>> {
    let rule = node.as_rule().expect("call arguments must be a rule");
    let mut args = Vec::new();

    for arg in rule.child_rules(RULE_EXPRESSION) {
        args.push(build_expr(arg.node())?);
    }

    Ok(args)
}

fn build_unary_expr(node: Node) -> CompileResult<UnaryExpr> {
    let mut children = node.children();
    let operator = children
        .next()
        .expect("unary expression must have an operator");
    let operand = children
        .next()
        .expect("unary expression must have an operand");

    let mut tokens = operator.children();
    let token = tokens.next().expect("unary operator must have a token");
    let terminal = token
        .as_terminal()
        .expect("child of unary operator must be a terminal");
    let token_type = terminal.symbol().token_type();
    let is_mut = tokens.any(|child| {
        child
            .as_terminal()
            .is_some_and(|terminal| terminal.symbol().token_type() == MUT)
    });

    let op = match token_type {
        MINUS => UnaryOp::Neg,
        NOT => UnaryOp::Not,
        STAR => UnaryOp::Deref,
        AMP | ANDAND if is_mut => UnaryOp::BorrowMut,
        AMP | ANDAND => UnaryOp::Borrow,
        _ => unreachable!("unexpected token in unary operator"),
    };

    let expr = UnaryExpr {
        op: op,
        operand: Box::new(build_expr(operand)?),
    };

    if token_type == ANDAND {
        let span = node_span(node);

        Ok(UnaryExpr {
            op: UnaryOp::Borrow,
            operand: Box::new(Expr {
                kind: ExprKind::Unary(expr),
                span: span.start + 1..span.end,
            }),
        })
    } else {
        Ok(expr)
    }
}

fn build_cast_expr(node: Node) -> CompileResult<CastExpr> {
    let mut children = node.children();
    let operand = children
        .next()
        .expect("cast expression must have an operand");
    let mut expr = build_expr(operand)?;

    for child in children {
        if let Some(rule) = child.as_rule() {
            match rule.rule_index() {
                RULE_TYPE_REF | rx_parser::RULE_CLOSED_CAST_TYPE => {
                    let ty = build_ty(child)?;
                    let span = expr.span.start..ty.span.end;
                    expr = Expr {
                        kind: ExprKind::Cast(CastExpr {
                            expr: Box::new(expr),
                            ty: ty,
                        }),
                        span: span,
                    };
                }
                _ => unreachable!("unexpected rule in cast expression"),
            }
        }
    }

    let ExprKind::Cast(expr) = expr.kind else {
        unreachable!("cast expression must have a target type");
    };
    Ok(expr)
}

fn build_assignment_expr(node: Node) -> CompileResult<AssignExpr> {
    let mut children = node.children();
    let lhs = children
        .next()
        .expect("assignment expression must have a left operand");
    let operator = children
        .next()
        .expect("assignment expression must have an operator");
    let rhs = children
        .next()
        .expect("assignment expression must have a right operand");

    let token = operator
        .children()
        .next()
        .expect("assignment operator must have a child");
    let op = if let Some(rule) = token.as_rule() {
        match rule.rule_index() {
            rx_parser::RULE_EQUALS_SIGN => AssignOp::Assign,
            _ => unreachable!("unexpected rule in assignment operator"),
        }
    } else {
        let terminal = token
            .as_terminal()
            .expect("child of assignment operator must be a rule or terminal");
        match terminal.symbol().token_type() {
            PLUS_ASSIGN => AssignOp::AddAssign,
            MINUS_ASSIGN => AssignOp::SubAssign,
            STAR_ASSIGN => AssignOp::MulAssign,
            SLASH_ASSIGN => AssignOp::DivAssign,
            PERCENT_ASSIGN => AssignOp::RemAssign,
            AMP_ASSIGN => AssignOp::BitAndAssign,
            PIPE_ASSIGN => AssignOp::BitOrAssign,
            CARET_ASSIGN => AssignOp::BitXorAssign,
            SHL_ASSIGN => AssignOp::ShlAssign,
            GT => AssignOp::ShrAssign,
            _ => unreachable!("unexpected token in assignment operator"),
        }
    };

    Ok(AssignExpr {
        op: op,
        lhs: Box::new(build_expr(lhs)?),
        rhs: Box::new(build_expr(rhs)?),
    })
}

fn build_binary_expr(node: Node) -> CompileResult<BinaryExpr> {
    let mut children = node.children();
    let lhs = children
        .next()
        .expect("binary expression must have a left operand");
    let mut expr = build_expr(lhs)?;

    while let Some(operator) = children.next() {
        let op = build_binary_op(operator);
        let rhs = children
            .next()
            .expect("binary expression must have a right operand");
        let rhs = build_expr(rhs)?;
        let span = expr.span.start..rhs.span.end;
        expr = Expr {
            kind: ExprKind::Binary(BinaryExpr {
                op: op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            }),
            span: span,
        };
    }

    let ExprKind::Binary(expr) = expr.kind else {
        unreachable!("binary expression must have an operator");
    };
    Ok(expr)
}

fn build_binary_op(node: Node) -> BinaryOp {
    if let Some(rule) = node.as_rule() {
        match rule.rule_index() {
            rx_parser::RULE_SHIFT_RIGHT => return BinaryOp::Shr,
            rx_parser::RULE_COMPARISON_EXCEPT_LT => {
                let child = node
                    .children()
                    .next()
                    .expect("comparison operator must have a child");
                // In comparisonExceptLt, a leading GT/GT_SECOND terminal can only
                // belong to >=. Plain > is wrapped in genericClose.
                if child.as_terminal().is_some_and(|terminal| {
                    matches!(terminal.symbol().token_type(), GT | GT_SECOND)
                }) {
                    return BinaryOp::Ge;
                }
                return build_binary_op(child);
            }

            rx_parser::RULE_ADDITIVE_OPERATOR
            | rx_parser::RULE_MULTIPLICATIVE_OPERATOR
            | rx_parser::RULE_GENERIC_CLOSE => {
                let child = node
                    .children()
                    .next()
                    .expect("binary operator must have a token");
                return build_binary_op(child);
            }
            _ => unreachable!("unexpected rule in binary operator"),
        }
    }

    let terminal = node
        .as_terminal()
        .expect("binary operator must be a rule or terminal");
    match terminal.symbol().token_type() {
        PLUS => BinaryOp::Add,
        MINUS => BinaryOp::Sub,
        STAR => BinaryOp::Mul,
        SLASH => BinaryOp::Div,
        PERCENT => BinaryOp::Rem,
        AMP => BinaryOp::BitAnd,
        PIPE => BinaryOp::BitOr,
        CARET => BinaryOp::BitXor,
        SHL => BinaryOp::Shl,
        EQEQ => BinaryOp::Eq,
        NE => BinaryOp::Ne,
        LT => BinaryOp::Lt,
        LE => BinaryOp::Le,
        GT | GT_SECOND => BinaryOp::Gt,
        ANDAND => BinaryOp::And,
        OROR => BinaryOp::Or,
        _ => unreachable!("unexpected token in binary operator"),
    }
}
