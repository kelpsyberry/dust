use super::super::{
    common::{DpOpImm8Ty, DpOpRegTy, DpOpSpecialTy, ShiftImmTy},
    Context,
};

pub(super) fn add_sub_reg_imm3<const SUB: bool, const IMM3: bool>(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let dst_reg = instr & 7;
    let src_reg = instr >> 3 & 7;
    let op3 = instr >> 6 & 7;
    let add_sub = if SUB { "sub" } else { "add" };
    let opcode = if IMM3 {
        format!("{} r{}, r{}, #{}", add_sub, dst_reg, src_reg, op3)
    } else {
        format!("{} r{}, r{}, r{}", add_sub, dst_reg, src_reg, op3)
    };
    if IMM3 && op3 == 0 {
        ctx.next_instr.opcode = format!("mov r{}, r{}", dst_reg, src_reg);
        ctx.next_instr.comment = opcode;
    } else {
        ctx.next_instr.opcode = opcode;
    }
}

pub(super) fn shift_imm<const SHIFT_TY: ShiftImmTy>(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let dst_reg = instr & 7;
    let src_reg = instr >> 3 & 7;
    let mut shift = instr >> 6 & 0x1F;
    if SHIFT_TY != ShiftImmTy::Lsl && shift == 0 {
        shift = 32;
    }
    ctx.next_instr.opcode = format!(
        "{} r{}, r{}, #{}",
        match SHIFT_TY {
            ShiftImmTy::Lsl => "lsl",
            ShiftImmTy::Lsr => "lsr",
            ShiftImmTy::Asr => "asr",
        },
        dst_reg,
        src_reg,
        shift,
    );
}

pub(super) fn dp_op_imm8<const OP_TY: DpOpImm8Ty>(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let src_dst_reg = instr >> 8 & 7;
    let op = instr & 0xFF;
    ctx.next_instr.opcode = format!(
        "{} r{}, #{:#04X}",
        match OP_TY {
            DpOpImm8Ty::Mov => "mov",
            DpOpImm8Ty::Cmp => "cmp",
            DpOpImm8Ty::Add => "add",
            DpOpImm8Ty::Sub => "sub",
        },
        src_dst_reg,
        op
    );
}

pub(super) fn dp_op_reg<const OP_TY: DpOpRegTy>(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let src_dst_reg = instr & 7;
    let op_reg = instr >> 3 & 7;
    ctx.next_instr.opcode = format!(
        "{} r{}, r{}",
        match OP_TY {
            DpOpRegTy::And => "and",
            DpOpRegTy::Eor => "eor",
            DpOpRegTy::Lsl => "lsl",
            DpOpRegTy::Lsr => "lsr",
            DpOpRegTy::Asr => "asr",
            DpOpRegTy::Adc => "adc",
            DpOpRegTy::Sbc => "sbc",
            DpOpRegTy::Ror => "ror",
            DpOpRegTy::Tst => "tst",
            DpOpRegTy::Neg => "neg",
            DpOpRegTy::Cmp => "cmp",
            DpOpRegTy::Cmn => "cmn",
            DpOpRegTy::Orr => "orr",
            DpOpRegTy::Mul => "mul",
            DpOpRegTy::Bic => "bic",
            DpOpRegTy::Mvn => "mvn",
        },
        src_dst_reg,
        op_reg
    );
}

pub(super) fn dp_op_special<const OP_TY: DpOpSpecialTy>(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let src_dst_reg = (instr & 7) | (instr >> 4 & 8);
    let op_reg = instr >> 3 & 0xF;
    ctx.next_instr.opcode = format!(
        "{} r{}, r{}",
        match OP_TY {
            DpOpSpecialTy::Add => "add",
            DpOpSpecialTy::Cmp => "cmp",
            DpOpSpecialTy::Mov => "mov",
        },
        src_dst_reg,
        op_reg
    );
}

pub(super) fn add_pc_sp_imm8<const SP: bool>(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let op = (instr & 0xFF) << 2;
    ctx.next_instr.opcode = format!("add r{}, #{:#05X}", if SP { 13 } else { 15 }, op);
}

pub(super) fn add_sub_sp_imm7<const SUB: bool>(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let op = (instr & 0x7F) << 2;
    ctx.next_instr.opcode = format!("{} r13, #{:#05X}", if SUB { "sub" } else { "add" }, op);
}
