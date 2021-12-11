use super::super::{
    super::{
        alu_utils::{arithmetic, bit_ops, shifts},
        common::{DpOpImm8Ty, DpOpRegTy, ShiftImmTy, StateSource},
        Engine,
    },
    add_cycles, multiply_cycles, reload_pipeline,
};
use crate::emu::Emu;

pub fn add_sub_reg_imm3<const SUB: bool, const IMM3: bool, const IS_MOV: bool>(
    emu: &mut Emu<Engine>,
    instr: u16,
) {
    let src = reg!(emu.arm7, instr >> 3 & 7);
    let result = if IS_MOV {
        bit_ops::set_nz(&mut emu.arm7.engine_data.regs, src);
        emu.arm7.engine_data.regs.cpsr = emu
            .arm7
            .engine_data
            .regs
            .cpsr
            .with_carry(SUB)
            .with_overflow(false);
        src
    } else {
        let op = if IMM3 {
            (instr >> 6 & 7) as u32
        } else {
            reg!(emu.arm7, instr >> 6 & 7)
        };
        if SUB {
            arithmetic::sub_s(&mut emu.arm7.engine_data.regs, src, op)
        } else {
            arithmetic::add_s(&mut emu.arm7.engine_data.regs, src, op)
        }
    };
    reg!(emu.arm7, instr & 7) = result;
    inc_r15!(emu.arm7, 2);
}

pub fn shift_imm<const SHIFT_TY: ShiftImmTy>(emu: &mut Emu<Engine>, instr: u16) {
    let src = reg!(emu.arm7, instr >> 3 & 7);
    let shift = (instr >> 6 & 0x1F) as u8;
    let result = match SHIFT_TY {
        ShiftImmTy::Lsl => shifts::lsl_imm_s(&mut emu.arm7.engine_data.regs, src, shift),
        ShiftImmTy::Lsr => shifts::lsr_imm_s(&mut emu.arm7.engine_data.regs, src, shift),
        ShiftImmTy::Asr => shifts::asr_imm_s(&mut emu.arm7.engine_data.regs, src, shift),
    };
    bit_ops::set_nz(&mut emu.arm7.engine_data.regs, result);
    reg!(emu.arm7, instr & 7) = result;
    inc_r15!(emu.arm7, 2);
}

pub fn dp_op_imm8<const OP_TY: DpOpImm8Ty>(emu: &mut Emu<Engine>, instr: u16) {
    let src_dst_reg = instr >> 8 & 7;
    let op = instr as u8 as u32;
    let src = reg!(emu.arm7, src_dst_reg);
    match OP_TY {
        DpOpImm8Ty::Mov => {
            emu.arm7.engine_data.regs.cpsr = emu
                .arm7
                .engine_data
                .regs
                .cpsr
                .with_negative(false)
                .with_zero(op == 0);
            reg!(emu.arm7, src_dst_reg) = op;
        }
        DpOpImm8Ty::Cmp => arithmetic::cmp(&mut emu.arm7.engine_data.regs, src, op),
        DpOpImm8Ty::Add => {
            let result = arithmetic::add_s(&mut emu.arm7.engine_data.regs, src, op);
            reg!(emu.arm7, src_dst_reg) = result;
        }
        DpOpImm8Ty::Sub => {
            let result = arithmetic::sub_s(&mut emu.arm7.engine_data.regs, src, op);
            reg!(emu.arm7, src_dst_reg) = result;
        }
    }
    inc_r15!(emu.arm7, 2);
}

pub fn dp_op_reg<const OP_TY: DpOpRegTy>(emu: &mut Emu<Engine>, instr: u16) {
    let src_dst_reg = instr & 7;
    let src = reg!(emu.arm7, src_dst_reg);
    let op = reg!(emu.arm7, instr >> 3 & 7);
    match OP_TY {
        DpOpRegTy::And => {
            let result = bit_ops::and_s(&mut emu.arm7.engine_data.regs, src, op);
            reg!(emu.arm7, src_dst_reg) = result;
        }
        DpOpRegTy::Eor => {
            let result = bit_ops::eor_s(&mut emu.arm7.engine_data.regs, src, op);
            reg!(emu.arm7, src_dst_reg) = result;
        }
        DpOpRegTy::Lsl => {
            let result = shifts::lsl_reg_s(&mut emu.arm7.engine_data.regs, src, op as u8);
            bit_ops::set_nz(&mut emu.arm7.engine_data.regs, result);
            reg!(emu.arm7, src_dst_reg) = result;
            add_cycles(emu, 1);
            emu.arm7.engine_data.prefetch_nseq = true;
        }
        DpOpRegTy::Lsr => {
            let result = shifts::lsr_reg_s(&mut emu.arm7.engine_data.regs, src, op as u8);
            bit_ops::set_nz(&mut emu.arm7.engine_data.regs, result);
            reg!(emu.arm7, src_dst_reg) = result;
            add_cycles(emu, 1);
            emu.arm7.engine_data.prefetch_nseq = true;
        }
        DpOpRegTy::Asr => {
            let result = shifts::asr_reg_s(&mut emu.arm7.engine_data.regs, src, op as u8);
            bit_ops::set_nz(&mut emu.arm7.engine_data.regs, result);
            reg!(emu.arm7, src_dst_reg) = result;
            add_cycles(emu, 1);
            emu.arm7.engine_data.prefetch_nseq = true;
        }
        DpOpRegTy::Adc => {
            let result = arithmetic::adc_s(&mut emu.arm7.engine_data.regs, src, op);
            reg!(emu.arm7, src_dst_reg) = result;
        }
        DpOpRegTy::Sbc => {
            let result = arithmetic::adc_s(&mut emu.arm7.engine_data.regs, src, !op);
            reg!(emu.arm7, src_dst_reg) = result;
        }
        DpOpRegTy::Ror => {
            let result = shifts::ror_reg_s(&mut emu.arm7.engine_data.regs, src, op as u8);
            bit_ops::set_nz(&mut emu.arm7.engine_data.regs, result);
            reg!(emu.arm7, src_dst_reg) = result;
            add_cycles(emu, 1);
            emu.arm7.engine_data.prefetch_nseq = true;
        }
        DpOpRegTy::Tst => bit_ops::tst(&mut emu.arm7.engine_data.regs, src, op),
        DpOpRegTy::Neg => {
            let result = arithmetic::sub_s(&mut emu.arm7.engine_data.regs, 0, op);
            reg!(emu.arm7, src_dst_reg) = result;
        }
        DpOpRegTy::Cmp => arithmetic::cmp(&mut emu.arm7.engine_data.regs, src, op),
        DpOpRegTy::Cmn => arithmetic::cmn(&mut emu.arm7.engine_data.regs, src, op),
        DpOpRegTy::Orr => {
            let result = bit_ops::orr_s(&mut emu.arm7.engine_data.regs, src, op);
            reg!(emu.arm7, src_dst_reg) = result;
        }
        DpOpRegTy::Mul => {
            let result = src.wrapping_mul(op);
            // TODO: What's the value of the carry flag?
            bit_ops::set_nz(&mut emu.arm7.engine_data.regs, result);
            reg!(emu.arm7, src_dst_reg) = result;
            add_cycles(emu, multiply_cycles(src));
            emu.arm7.engine_data.prefetch_nseq = true;
        }
        DpOpRegTy::Bic => {
            let result = bit_ops::bic_s(&mut emu.arm7.engine_data.regs, src, op);
            reg!(emu.arm7, src_dst_reg) = result;
        }
        DpOpRegTy::Mvn => {
            let result = !op;
            bit_ops::set_nz(&mut emu.arm7.engine_data.regs, result);
            reg!(emu.arm7, src_dst_reg) = result;
        }
    }
    inc_r15!(emu.arm7, 2);
}

pub fn add_special(emu: &mut Emu<Engine>, instr: u16) {
    let src_dst_reg = (instr & 7) | (instr >> 4 & 8);
    let result = reg!(emu.arm7, src_dst_reg).wrapping_add(reg!(emu.arm7, instr >> 3 & 0xF));
    reg!(emu.arm7, src_dst_reg) = result;
    if src_dst_reg == 15 {
        reload_pipeline::<{ StateSource::Thumb }>(emu);
    } else {
        inc_r15!(emu.arm7, 2);
    }
}

pub fn cmp_special(emu: &mut Emu<Engine>, instr: u16) {
    let src = reg!(emu.arm7, (instr & 7) | (instr >> 4 & 8));
    let op = reg!(emu.arm7, instr >> 3 & 0xF);
    arithmetic::cmp(&mut emu.arm7.engine_data.regs, src, op);
    inc_r15!(emu.arm7, 2);
}

pub fn mov_special(emu: &mut Emu<Engine>, instr: u16) {
    let dst_reg = (instr & 7) | (instr >> 4 & 8);
    let op = reg!(emu.arm7, instr >> 3 & 0xF);
    reg!(emu.arm7, dst_reg) = op;
    if dst_reg == 15 {
        reload_pipeline::<{ StateSource::Thumb }>(emu);
    } else {
        inc_r15!(emu.arm7, 2);
    }
}

pub fn add_pc_sp_imm8<const SP: bool>(emu: &mut Emu<Engine>, instr: u16) {
    let src = if SP {
        reg!(emu.arm7, 13)
    } else {
        reg!(emu.arm7, 15) & !3
    };
    let result = src.wrapping_add(((instr & 0xFF) << 2) as u32);
    reg!(emu.arm7, instr >> 8 & 7) = result;
    inc_r15!(emu.arm7, 2);
}

pub fn add_sub_sp_imm7<const SUB: bool>(emu: &mut Emu<Engine>, instr: u16) {
    let src = reg!(emu.arm7, 13);
    let op = ((instr & 0x7F) << 2) as u32;
    let result = if SUB {
        src.wrapping_sub(op)
    } else {
        src.wrapping_add(op)
    };
    reg!(emu.arm7, 13) = result;
    inc_r15!(emu.arm7, 2);
}
