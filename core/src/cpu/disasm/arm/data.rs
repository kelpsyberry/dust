use super::super::{
    common::{DpOpTy, DpOperand, ShiftTy},
    Context,
};
use core::fmt::Write;

pub(super) fn dp_op<const OP_TY: DpOpTy, const OPERAND: DpOperand, const SET_FLAGS: bool>(
    ctx: &mut Context,
    instr: u32,
    cond: &'static str,
) {
    let dst_reg = instr >> 12 & 0xF;
    let src_reg = instr >> 16 & 0xF;
    ctx.next_instr.opcode = format!(
        "{}{}{} ",
        match OP_TY {
            DpOpTy::And => "and",
            DpOpTy::Eor => "eor",
            DpOpTy::Sub => "sub",
            DpOpTy::Rsb => "rsb",
            DpOpTy::Add => "add",
            DpOpTy::Adc => "adc",
            DpOpTy::Sbc => "sbc",
            DpOpTy::Rsc => "rsc",
            DpOpTy::Tst => "tst",
            DpOpTy::Teq => "teq",
            DpOpTy::Cmp => "cmp",
            DpOpTy::Cmn => "cmn",
            DpOpTy::Orr => "orr",
            DpOpTy::Mov => "mov",
            DpOpTy::Bic => "bic",
            DpOpTy::Mvn => "mvn",
        },
        cond,
        if SET_FLAGS { "s" } else { "" }
    );
    if !OP_TY.is_test() {
        let _ = write!(ctx.next_instr.opcode, "r{}, ", dst_reg);
    }
    if !OP_TY.is_unary() {
        let _ = write!(ctx.next_instr.opcode, "r{}, ", src_reg);
    }
    match OPERAND {
        DpOperand::Imm => {
            let value = instr & 0xFF;
            let shift = instr >> 7 & 0x1E;
            let _ = write!(ctx.next_instr.opcode, "#{:#04X}, {}", value, shift);
        }
        DpOperand::Reg {
            shift_ty,
            shift_imm,
        } => {
            let op_reg = instr & 0xF;
            let _ = write!(ctx.next_instr.opcode, "r{}, ", op_reg);
            if shift_imm {
                let mut shift = instr >> 7 & 0x1F;
                if matches!(shift_ty, ShiftTy::Lsr | ShiftTy::Asr) && shift == 0 {
                    shift = 32;
                }
                let _ = match shift_ty {
                    ShiftTy::Lsl => write!(ctx.next_instr.opcode, "lsl #{}", shift),
                    ShiftTy::Lsr => write!(ctx.next_instr.opcode, "lsr #{}", shift),
                    ShiftTy::Asr => write!(ctx.next_instr.opcode, "asr #{}", shift),
                    ShiftTy::Ror => {
                        if shift == 0 {
                            write!(ctx.next_instr.opcode, "rrx")
                        } else {
                            write!(ctx.next_instr.opcode, "ror #{}", shift)
                        }
                    }
                };
            } else {
                let shift_reg = instr >> 8 & 0xF;
                let _ = write!(
                    ctx.next_instr.opcode,
                    "{} r{}",
                    match shift_ty {
                        ShiftTy::Lsl => "lsl",
                        ShiftTy::Lsr => "lsr",
                        ShiftTy::Asr => "asr",
                        ShiftTy::Ror => "ror",
                    },
                    shift_reg
                );
            }
        }
    }
    if OP_TY.is_test() && dst_reg != 0 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn clz(ctx: &mut Context, instr: u32, cond: &'static str) {
    let dst_reg = instr >> 12 & 0xF;
    let src_reg = instr & 0xF;
    ctx.next_instr.opcode = format!("clz{} r{}, r{}", cond, dst_reg, src_reg);
    if src_reg == 15 || dst_reg == 15 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn mul<const ACC: bool, const SET_FLAGS: bool>(
    ctx: &mut Context,
    instr: u32,
    cond: &'static str,
) {
    let dst_reg = instr >> 16 & 0xF;
    let src_reg = instr & 0xF;
    let op_reg = instr >> 8 & 0xF;
    let acc_reg = instr >> 12 & 0xF;
    ctx.next_instr.opcode = format!(
        "{}{}{} r{}, r{}, r{}",
        if ACC { "mla" } else { "mul" },
        cond,
        if SET_FLAGS { "s" } else { "" },
        dst_reg,
        src_reg,
        op_reg
    );
    if ACC {
        let _ = write!(ctx.next_instr.opcode, ", r{}", acc_reg);
    }
    if (!ACC && acc_reg != 0) || dst_reg == 15 || src_reg == 15 || op_reg == 15 || acc_reg == 15 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn umull_smull<const SIGNED: bool, const ACC: bool, const SET_FLAGS: bool>(
    ctx: &mut Context,
    instr: u32,
    cond: &'static str,
) {
    let dst_acc_reg_low = instr >> 12 & 0xF;
    let dst_acc_reg_high = instr >> 16 & 0xF;
    let src_reg = instr & 0xF;
    let op_reg = instr >> 8 & 0xF;
    ctx.next_instr.opcode = format!(
        "{}{}l{}{} r{}, r{}, r{}, r{}",
        if SIGNED { 's' } else { 'u' },
        if ACC { "mla" } else { "mul" },
        cond,
        if SET_FLAGS { "s" } else { "" },
        dst_acc_reg_low,
        dst_acc_reg_high,
        src_reg,
        op_reg
    );
    if dst_acc_reg_low == 15 || dst_acc_reg_high == 15 || src_reg == 15 || op_reg == 15 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn smulxy<const ACC: bool>(ctx: &mut Context, instr: u32, cond: &'static str) {
    let dst_reg = instr >> 16 & 0xF;
    let src_reg = instr & 0xF;
    let op_reg = instr >> 8 & 0xF;
    let acc_reg = instr >> 12 & 0xF;
    ctx.next_instr.opcode = format!(
        "s{}{}{}{} r{}, r{}, r{}",
        if ACC { "mla" } else { "mul" },
        if instr & 1 << 5 == 0 { 'b' } else { 't' },
        if instr & 1 << 6 == 0 { 'b' } else { 't' },
        cond,
        dst_reg,
        src_reg,
        op_reg
    );
    if ACC {
        let _ = write!(ctx.next_instr.opcode, ", r{}", acc_reg);
    }
    if (!ACC && acc_reg != 0) || dst_reg == 15 || src_reg == 15 || op_reg == 15 || acc_reg == 15 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn smulwy<const ACC: bool>(ctx: &mut Context, instr: u32, cond: &'static str) {
    let dst_reg = instr >> 16 & 0xF;
    let src_reg = instr & 0xF;
    let op_reg = instr >> 8 & 0xF;
    let acc_reg = instr >> 12 & 0xF;
    ctx.next_instr.opcode = format!(
        "s{}w{}{} r{}, r{}, r{}",
        if ACC { "mla" } else { "mul" },
        if instr & 1 << 6 == 0 { 'b' } else { 't' },
        cond,
        dst_reg,
        src_reg,
        op_reg
    );
    if ACC {
        let _ = write!(ctx.next_instr.opcode, ", r{}", acc_reg);
    }
    if (!ACC && acc_reg != 0) || dst_reg == 15 || src_reg == 15 || op_reg == 15 || acc_reg == 15 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn smlalxy(ctx: &mut Context, instr: u32, cond: &'static str) {
    let dst_acc_reg_low = instr >> 12 & 0xF;
    let dst_acc_reg_high = instr >> 16 & 0xF;
    let src_reg = instr & 0xF;
    let op_reg = instr >> 8 & 0xF;
    ctx.next_instr.opcode = format!(
        "smlal{}{}{} r{}, r{}, r{}, r{}",
        if instr & 1 << 5 == 0 { 'b' } else { 't' },
        if instr & 1 << 6 == 0 { 'b' } else { 't' },
        cond,
        dst_acc_reg_low,
        dst_acc_reg_high,
        src_reg,
        op_reg
    );
    if dst_acc_reg_low == 15 || dst_acc_reg_high == 15 || src_reg == 15 || op_reg == 15 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn qaddsub<const SUB: bool, const DOUBLED: bool>(
    ctx: &mut Context,
    instr: u32,
    cond: &'static str,
) {
    let dst_reg = instr >> 12 & 0xF;
    let src_reg = instr & 0xF;
    let op_reg = instr >> 16 & 0xF;
    ctx.next_instr.opcode = format!(
        "q{}{}{} r{}, r{}, r{}",
        if DOUBLED { "d" } else { "" },
        if SUB { "sub" } else { "add" },
        cond,
        dst_reg,
        src_reg,
        op_reg
    );
    if dst_reg == 15 || src_reg == 15 || op_reg == 15 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}
