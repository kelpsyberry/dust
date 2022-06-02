use super::super::{
    add_bus_cycles, add_cycles, apply_reg_interlock_1, apply_reg_interlocks_2, prefetch_thumb,
    reload_pipeline, write_reg_clear_interlock_ab, write_reg_interlock_ab,
};
use crate::{
    cpu::interpreter::{
        alu_utils::{arithmetic, bit_ops, shifts},
        common::{DpOpImm8Ty, DpOpRegTy, ShiftImmTy, StateSource},
        Engine,
    },
    emu::Emu,
    utils::schedule::RawTimestamp,
};

// TODO: Check shift by reg timings, they might be different from their equivalent ARM instructions'
//       ones (at the moment, they're assumed to be the same, which is to say 2 cycles, reading the
//       source register in the second cycle).

pub fn add_sub_reg_imm3<const SUB: bool, const IMM3: bool, const IS_MOV: bool>(
    emu: &mut Emu<Engine>,
    instr: u16,
) {
    let src_reg = (instr >> 3 & 7) as u8;
    let op3 = (instr >> 6 & 7) as u8;
    if IMM3 {
        apply_reg_interlock_1::<false>(emu, src_reg);
    } else {
        apply_reg_interlocks_2::<0, false>(emu, src_reg, op3);
    }
    add_bus_cycles(emu, 1);
    let src = reg!(emu.arm9, src_reg);
    let result = if IS_MOV {
        bit_ops::set_nz(&mut emu.arm9.engine_data.regs, src);
        emu.arm9.engine_data.regs.cpsr = emu
            .arm9
            .engine_data
            .regs
            .cpsr
            .with_carry(SUB)
            .with_overflow(false);
        src
    } else {
        let op = if IMM3 {
            op3 as u32
        } else {
            reg!(emu.arm9, op3)
        };
        if SUB {
            arithmetic::sub_s(&mut emu.arm9.engine_data.regs, src, op)
        } else {
            arithmetic::add_s(&mut emu.arm9.engine_data.regs, src, op)
        }
    };
    write_reg_clear_interlock_ab(emu, (instr & 7) as u8, result);
    prefetch_thumb::<true, true>(emu);
}

pub fn shift_imm<const SHIFT_TY: ShiftImmTy>(emu: &mut Emu<Engine>, instr: u16) {
    let src_reg = (instr >> 3 & 7) as u8;
    apply_reg_interlock_1::<false>(emu, src_reg);
    add_bus_cycles(emu, 1);
    let src = reg!(emu.arm9, src_reg);
    let shift = (instr >> 6 & 0x1F) as u8;
    let result = match SHIFT_TY {
        ShiftImmTy::Lsl => shifts::lsl_imm_s(&mut emu.arm9.engine_data.regs, src, shift),
        ShiftImmTy::Lsr => shifts::lsr_imm_s(&mut emu.arm9.engine_data.regs, src, shift),
        ShiftImmTy::Asr => shifts::asr_imm_s(&mut emu.arm9.engine_data.regs, src, shift),
    };
    bit_ops::set_nz(&mut emu.arm9.engine_data.regs, result);
    write_reg_clear_interlock_ab(emu, (instr & 7) as u8, result);
    prefetch_thumb::<true, true>(emu);
}

pub fn dp_op_imm8<const OP_TY: DpOpImm8Ty>(emu: &mut Emu<Engine>, instr: u16) {
    let src_dst_reg = (instr >> 8 & 7) as u8;
    if OP_TY != DpOpImm8Ty::Mov {
        apply_reg_interlock_1::<false>(emu, src_dst_reg);
    }
    let op = instr as u8 as u32;
    add_bus_cycles(emu, 1);
    let src = reg!(emu.arm9, src_dst_reg);
    match OP_TY {
        DpOpImm8Ty::Mov => {
            emu.arm9.engine_data.regs.cpsr = emu
                .arm9
                .engine_data
                .regs
                .cpsr
                .with_negative(false)
                .with_zero(op == 0);
            write_reg_clear_interlock_ab(emu, src_dst_reg, op);
        }
        DpOpImm8Ty::Cmp => arithmetic::cmp(&mut emu.arm9.engine_data.regs, src, op),
        DpOpImm8Ty::Add => {
            let result = arithmetic::add_s(&mut emu.arm9.engine_data.regs, src, op);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
        }
        DpOpImm8Ty::Sub => {
            let result = arithmetic::sub_s(&mut emu.arm9.engine_data.regs, src, op);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
        }
    }
    prefetch_thumb::<true, true>(emu);
}

pub fn dp_op_reg<const OP_TY: DpOpRegTy>(emu: &mut Emu<Engine>, instr: u16) {
    let src_dst_reg = (instr & 7) as u8;
    let op_reg = (instr >> 3 & 7) as u8;
    let src = reg!(emu.arm9, src_dst_reg);
    let op = reg!(emu.arm9, op_reg);
    if OP_TY.is_unary() {
        apply_reg_interlock_1::<false>(emu, op_reg);
    } else if OP_TY.is_shift() {
        apply_reg_interlocks_2::<1, false>(emu, src_dst_reg, op_reg);
    } else {
        apply_reg_interlocks_2::<0, false>(emu, src_dst_reg, op_reg);
    }
    add_bus_cycles(
        emu,
        (1 + (OP_TY.is_shift() || OP_TY == DpOpRegTy::Mul) as u8) as RawTimestamp,
    );
    match OP_TY {
        DpOpRegTy::And => {
            let result = bit_ops::and_s(&mut emu.arm9.engine_data.regs, src, op);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
            prefetch_thumb::<true, true>(emu);
        }
        DpOpRegTy::Eor => {
            let result = bit_ops::eor_s(&mut emu.arm9.engine_data.regs, src, op);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
            prefetch_thumb::<true, true>(emu);
        }
        DpOpRegTy::Lsl => {
            let result = shifts::lsl_reg_s(&mut emu.arm9.engine_data.regs, src, op as u8);
            bit_ops::set_nz(&mut emu.arm9.engine_data.regs, result);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
            prefetch_thumb::<true, true>(emu);
            add_cycles(emu, 1);
        }
        DpOpRegTy::Lsr => {
            let result = shifts::lsr_reg_s(&mut emu.arm9.engine_data.regs, src, op as u8);
            bit_ops::set_nz(&mut emu.arm9.engine_data.regs, result);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
            prefetch_thumb::<true, true>(emu);
            add_cycles(emu, 1);
        }
        DpOpRegTy::Asr => {
            let result = shifts::asr_reg_s(&mut emu.arm9.engine_data.regs, src, op as u8);
            bit_ops::set_nz(&mut emu.arm9.engine_data.regs, result);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
            prefetch_thumb::<true, true>(emu);
            add_cycles(emu, 1);
        }
        DpOpRegTy::Adc => {
            let result = arithmetic::adc_s(&mut emu.arm9.engine_data.regs, src, op);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
            prefetch_thumb::<true, true>(emu);
        }
        DpOpRegTy::Sbc => {
            let result = arithmetic::adc_s(&mut emu.arm9.engine_data.regs, src, !op);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
            prefetch_thumb::<true, true>(emu);
        }
        DpOpRegTy::Ror => {
            let result = shifts::ror_reg_s(&mut emu.arm9.engine_data.regs, src, op as u8);
            bit_ops::set_nz(&mut emu.arm9.engine_data.regs, result);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
            prefetch_thumb::<true, true>(emu);
            add_cycles(emu, 1);
        }
        DpOpRegTy::Tst => {
            bit_ops::tst(&mut emu.arm9.engine_data.regs, src, op);
            prefetch_thumb::<true, true>(emu);
        }
        DpOpRegTy::Neg => {
            let result = arithmetic::sub_s(&mut emu.arm9.engine_data.regs, 0, op);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result as u32);
            prefetch_thumb::<true, true>(emu);
        }
        DpOpRegTy::Cmp => {
            arithmetic::cmp(&mut emu.arm9.engine_data.regs, src, op);
            prefetch_thumb::<true, true>(emu);
        }
        DpOpRegTy::Cmn => {
            arithmetic::cmn(&mut emu.arm9.engine_data.regs, src, op);
            prefetch_thumb::<true, true>(emu);
        }
        DpOpRegTy::Orr => {
            let result = bit_ops::orr_s(&mut emu.arm9.engine_data.regs, src, op);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
            prefetch_thumb::<true, true>(emu);
        }
        DpOpRegTy::Mul => {
            let result = src.wrapping_mul(op);
            bit_ops::set_nz(&mut emu.arm9.engine_data.regs, result);
            write_reg_interlock_ab(emu, src_dst_reg, result, 1);
            prefetch_thumb::<true, true>(emu);
            add_cycles(emu, 1);
        }
        DpOpRegTy::Bic => {
            let result = bit_ops::bic_s(&mut emu.arm9.engine_data.regs, src, op);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
            prefetch_thumb::<true, true>(emu);
        }
        DpOpRegTy::Mvn => {
            let result = !op;
            bit_ops::set_nz(&mut emu.arm9.engine_data.regs, result);
            write_reg_clear_interlock_ab(emu, src_dst_reg, result);
            prefetch_thumb::<true, true>(emu);
        }
    }
}

pub fn add_special(emu: &mut Emu<Engine>, instr: u16) {
    let src_dst_reg = ((instr & 7) | (instr >> 4 & 8)) as u8;
    let op_reg = (instr >> 3 & 0xF) as u8;
    apply_reg_interlocks_2::<0, false>(emu, src_dst_reg, op_reg);
    let result = reg!(emu.arm9, src_dst_reg).wrapping_add(reg!(emu.arm9, op_reg));
    prefetch_thumb::<true, true>(emu);
    write_reg_clear_interlock_ab(emu, src_dst_reg, result);
    if src_dst_reg == 15 {
        add_bus_cycles(emu, 2);
        reload_pipeline::<{ StateSource::Thumb }>(emu);
    } else {
        add_bus_cycles(emu, 1);
    }
}

pub fn cmp_special(emu: &mut Emu<Engine>, instr: u16) {
    let src_reg = ((instr & 7) | (instr >> 4 & 8)) as u8;
    let op_reg = (instr >> 3 & 0xF) as u8;
    apply_reg_interlocks_2::<0, false>(emu, src_reg, op_reg);
    add_bus_cycles(emu, 1);
    let (src, op) = (reg!(emu.arm9, src_reg), reg!(emu.arm9, op_reg));
    arithmetic::cmp(&mut emu.arm9.engine_data.regs, src, op);
    prefetch_thumb::<true, true>(emu);
}

pub fn mov_special(emu: &mut Emu<Engine>, instr: u16) {
    let op_reg = (instr >> 3 & 0xF) as u8;
    apply_reg_interlock_1::<false>(emu, op_reg);
    let op = reg!(emu.arm9, op_reg);
    prefetch_thumb::<true, true>(emu);
    let dst_reg = ((instr & 7) | (instr >> 4 & 8)) as u8;
    write_reg_clear_interlock_ab(emu, dst_reg, op);
    if dst_reg == 15 {
        add_bus_cycles(emu, 2);
        reload_pipeline::<{ StateSource::Thumb }>(emu);
    } else {
        add_bus_cycles(emu, 1);
    }
}

pub fn add_pc_sp_imm8<const SP: bool>(emu: &mut Emu<Engine>, instr: u16) {
    let src = if SP {
        reg!(emu.arm9, 13)
    } else {
        reg!(emu.arm9, 15) & !3
    };
    let result = src.wrapping_add(((instr & 0xFF) << 2) as u32);
    write_reg_clear_interlock_ab(emu, (instr >> 8 & 7) as u8, result);
    add_bus_cycles(emu, 1);
    prefetch_thumb::<true, true>(emu);
}

pub fn add_sub_sp_imm7<const SUB: bool>(emu: &mut Emu<Engine>, instr: u16) {
    let src = reg!(emu.arm9, 13);
    let op = ((instr & 0x7F) << 2) as u32;
    let result = if SUB {
        src.wrapping_sub(op)
    } else {
        src.wrapping_add(op)
    };
    reg!(emu.arm9, 13) = result;
    add_bus_cycles(emu, 1);
    prefetch_thumb::<true, true>(emu);
}
