use std::fmt::Write;

use crate::ir::{
    BasicBlock, BinaryOp, CastOp, Constant, Function, InstId, Instruction, InstructionKind,
    IntPredicate, IrTy, Module, Operand, StructDef, StructId, Terminator,
};

fn format_ty(ty: &IrTy) -> String {
    match ty {
        IrTy::Void => "void".into(),
        IrTy::I1 => "i1".into(),
        IrTy::I8 => "i8".into(),
        IrTy::I32 => "i32".into(),
        IrTy::Ptr => "ptr".into(),
        IrTy::Array { element, len } => format!("[{len} x {}]", format_ty(element)),
        IrTy::Struct(id) => format!("%struct.{}", id.0),
    }
}

fn emit_struct(out: &mut String, id: StructId, structure: &StructDef) {
    writeln!(out, "; struct {}", structure.name).unwrap();
    let fields = structure
        .fields
        .as_ref()
        .expect("struct must be defined before emission")
        .iter()
        .map(format_ty)
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(out, "%struct.{} = type {{{fields}}}", id.0).unwrap();
}

fn format_operand(operand: Operand) -> String {
    match operand {
        Operand::Void => panic!("void cannot be printed as an operand"),
        Operand::Parameter(index) => format!("%p{index}"),
        Operand::Instruction(id) => format!("%v{}", id.0),
        Operand::Constant(constant) => match constant {
            Constant::I1(value) => value.to_string(),
            Constant::I8(value) => value.to_string(),
            Constant::I32(value) => value.to_string(),
            Constant::Null => "null".into(),
        },
    }
}

fn format_typed_operand(operand: Operand, function: &Function) -> String {
    format!(
        "{} {}",
        format_ty(operand.ty(function)),
        format_operand(operand),
    )
}

fn emit_terminal(out: &mut String, terminal: &Terminator, function: &Function) {
    match terminal {
        Terminator::Br(block) => writeln!(out, "  br label %bb{}", block.0).unwrap(),
        Terminator::CondBr {
            condition,
            then_block,
            else_block,
        } => writeln!(
            out,
            "  br {}, label %bb{}, label %bb{}",
            format_typed_operand(*condition, function),
            then_block.0,
            else_block.0,
        )
        .unwrap(),
        Terminator::Ret(Operand::Void) => writeln!(out, "  ret void").unwrap(),
        Terminator::Ret(value) => {
            writeln!(out, "  ret {}", format_typed_operand(*value, function),).unwrap()
        }
        Terminator::Unreachable => writeln!(out, "  unreachable").unwrap(),
    }
}

fn emit_inst(
    out: &mut String,
    id: InstId,
    inst: &Instruction,
    module: &Module,
    function: &Function,
) {
    write!(out, "  ").unwrap();
    if inst.result_ty().is_some() {
        write!(out, "%v{} = ", id.0).unwrap();
    }
    match &inst.kind {
        InstructionKind::Alloca { ty, align } => {
            write!(out, "alloca {}, align {align}", format_ty(ty)).unwrap();
        }
        InstructionKind::Load { ty, ptr, align } => {
            write!(
                out,
                "load {}, ptr {}, align {align}",
                format_ty(ty),
                format_operand(*ptr)
            )
            .unwrap();
        }
        InstructionKind::Store { value, ptr, align } => {
            write!(
                out,
                "store {}, ptr {}, align {align}",
                format_typed_operand(*value, function),
                format_operand(*ptr)
            )
            .unwrap();
        }
        InstructionKind::GetElementPtr {
            source_ty,
            ptr,
            indices,
        } => {
            write!(
                out,
                "getelementptr {}, ptr {}",
                format_ty(source_ty),
                format_operand(*ptr)
            )
            .unwrap();
            for index in indices {
                write!(out, ", {}", format_typed_operand(*index, function)).unwrap();
            }
        }
        InstructionKind::Binary { op, lhs, rhs } => {
            let opcode = match op {
                BinaryOp::Add => "add",
                BinaryOp::Sub => "sub",
                BinaryOp::Mul => "mul",
                BinaryOp::SDiv => "sdiv",
                BinaryOp::UDiv => "udiv",
                BinaryOp::SRem => "srem",
                BinaryOp::URem => "urem",
                BinaryOp::And => "and",
                BinaryOp::Or => "or",
                BinaryOp::Xor => "xor",
                BinaryOp::Shl => "shl",
                BinaryOp::AShr => "ashr",
                BinaryOp::LShr => "lshr",
            };
            write!(
                out,
                "{opcode} {}, {}",
                format_typed_operand(*lhs, function),
                format_operand(*rhs)
            )
            .unwrap();
        }
        InstructionKind::ICmp {
            predicate,
            lhs,
            rhs,
        } => {
            let predicate = match predicate {
                IntPredicate::Eq => "eq",
                IntPredicate::Ne => "ne",
                IntPredicate::Slt => "slt",
                IntPredicate::Sle => "sle",
                IntPredicate::Sgt => "sgt",
                IntPredicate::Sge => "sge",
                IntPredicate::Ult => "ult",
                IntPredicate::Ule => "ule",
                IntPredicate::Ugt => "ugt",
                IntPredicate::Uge => "uge",
            };
            write!(
                out,
                "icmp {predicate} {}, {}",
                format_typed_operand(*lhs, function),
                format_operand(*rhs)
            )
            .unwrap();
        }
        InstructionKind::Cast { op, value, to } => {
            let opcode = match op {
                CastOp::ZExt => "zext",
                CastOp::Trunc => "trunc",
            };
            write!(
                out,
                "{opcode} {} to {}",
                format_typed_operand(*value, function),
                format_ty(to)
            )
            .unwrap();
        }
        InstructionKind::Call {
            function: callee,
            args,
        } => {
            let callee = &module.functions[callee.0 as usize];
            let args = args
                .iter()
                .map(|arg| format_typed_operand(*arg, function))
                .collect::<Vec<_>>()
                .join(", ");
            write!(
                out,
                "call {} @{}({args})",
                format_ty(&callee.signature.result),
                callee.name
            )
            .unwrap();
        }
    }
    writeln!(out).unwrap();
}

fn emit_block(
    out: &mut String,
    id: usize,
    block: &BasicBlock,
    module: &Module,
    function: &Function,
) {
    writeln!(out, "bb{id}:").unwrap();
    let body = function.body.as_ref().expect("function must have a body");
    for id in &block.instructions {
        emit_inst(
            out,
            *id,
            &body.instructions[id.0 as usize],
            module,
            function,
        );
    }
    emit_terminal(
        out,
        block
            .terminator
            .as_ref()
            .expect("block must have a terminator"),
        function,
    );
}

fn emit_function(out: &mut String, module: &Module, function: &Function) {
    let params = function
        .signature
        .params
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            if function.body.is_some() {
                format!("{} %p{index}", format_ty(ty))
            } else {
                format_ty(ty)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(
        out,
        "{} {} @{}({params}){}",
        if function.body.is_some() {
            "define"
        } else {
            "declare"
        },
        format_ty(&function.signature.result),
        function.name,
        if function.body.is_some() { " {" } else { "" },
    )
    .unwrap();

    let Some(body) = &function.body else {
        return;
    };

    for (index, block) in body.blocks.iter().enumerate() {
        emit_block(out, index, block, module, function);
    }

    writeln!(out, "}}").unwrap();
}

pub fn emit_module(module: &Module) -> String {
    let mut text = String::new();

    writeln!(text, "target triple = \"riscv32-unknown-elf\"").unwrap();
    writeln!(
        text,
        "target datalayout = \"e-m:e-p:32:32-i64:64-n32-S128\""
    )
    .unwrap();

    for (index, structure) in module.structs.iter().enumerate() {
        emit_struct(&mut text, StructId(index as u32), structure);
        writeln!(text).unwrap();
    }

    for function in &module.functions {
        emit_function(&mut text, module, function);
        writeln!(text).unwrap();
    }

    text
}
