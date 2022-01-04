use super::super::{
    add_bus_cycles, add_cycles, apply_reg_interlock_1, apply_reg_interlocks_2,
    apply_reg_interlocks_3, prefetch_arm, reload_pipeline, restore_spsr,
    write_reg_clear_interlock_ab, write_reg_interlock_ab,
};
use crate::{
    cpu::interpreter::{
        alu_utils::{arithmetic, bit_ops, shifts},
        common::{DpOpTy, DpOperand, ShiftTy, StateSource},
        Engine,
    },
    emu::Emu,
};
use core::intrinsics::{likely, unlikely};

// TODO: Check timing for r15 writes, the ARM9E-S manual says some operations take 4 bus cycles and
//       some others take 3, but at the moment only register-specified shifts take 4 cycles, and all
//       others take 3, regardless of the operation.

pub fn dp_op<const OP_TY: DpOpTy, const OPERAND: DpOperand, const SET_FLAGS: bool>(
    emu: &mut Emu<Engine>,
    instr: u32,
) {
    let src_reg = (instr >> 16 & 0xF) as u8;
    let (src, op) = match OPERAND {
        DpOperand::Imm => {
            if !OP_TY.is_unary() {
                apply_reg_interlock_1::<false>(emu, src_reg);
            }
            add_bus_cycles(emu, 1);
            let value = instr & 0xFF;
            let shift = instr >> 7 & 0x1E;
            // Don't calculate the shifter carry for arithmetic instructions, as they'll overwrite
            // the flag anyway
            let op = if SET_FLAGS && !OP_TY.sets_carry() {
                shifts::ror_imm_s_no_rrx(&mut emu.arm9.engine_data.regs, value, shift as u8)
            } else {
                value.rotate_right(shift)
            };
            let src = reg!(emu.arm9, src_reg);
            prefetch_arm::<true, true>(emu);
            (src, op)
        }
        DpOperand::Reg {
            shift_ty,
            shift_imm,
        } => {
            let op_reg = (instr & 0xF) as u8;
            if shift_imm {
                if OP_TY.is_unary() {
                    apply_reg_interlock_1::<false>(emu, op_reg);
                } else {
                    apply_reg_interlocks_2::<0, false>(emu, src_reg, op_reg);
                }
                add_bus_cycles(emu, 1);
                let shift = (instr >> 7 & 0x1F) as u8;
                let value = reg!(emu.arm9, op_reg);
                let op = if SET_FLAGS && !OP_TY.sets_carry() {
                    match shift_ty {
                        ShiftTy::Lsl => {
                            shifts::lsl_imm_s(&mut emu.arm9.engine_data.regs, value, shift)
                        }
                        ShiftTy::Lsr => {
                            shifts::lsr_imm_s(&mut emu.arm9.engine_data.regs, value, shift)
                        }
                        ShiftTy::Asr => {
                            shifts::asr_imm_s(&mut emu.arm9.engine_data.regs, value, shift)
                        }
                        ShiftTy::Ror => {
                            shifts::ror_imm_s(&mut emu.arm9.engine_data.regs, value, shift)
                        }
                    }
                } else {
                    match shift_ty {
                        ShiftTy::Lsl => shifts::lsl_imm(value, shift),
                        ShiftTy::Lsr => shifts::lsr_imm(value, shift),
                        ShiftTy::Asr => shifts::asr_imm(value, shift),
                        ShiftTy::Ror => shifts::ror_imm(&emu.arm9.engine_data.regs, value, shift),
                    }
                };
                let src = reg!(emu.arm9, src_reg);
                prefetch_arm::<true, true>(emu);
                (src, op)
            } else {
                let shift_reg = (instr >> 8 & 0xF) as u8;
                if OP_TY.is_unary() {
                    apply_reg_interlocks_2::<1, false>(emu, op_reg, shift_reg);
                } else {
                    apply_reg_interlocks_3::<1, false>(emu, src_reg, op_reg, shift_reg);
                }
                add_bus_cycles(emu, 2);
                let shift = reg!(emu.arm9, shift_reg) as u8;
                prefetch_arm::<true, true>(emu);
                add_cycles(emu, 1);
                let value = reg!(emu.arm9, op_reg);
                let op = if SET_FLAGS && !OP_TY.sets_carry() {
                    match shift_ty {
                        ShiftTy::Lsl => {
                            shifts::lsl_reg_s(&mut emu.arm9.engine_data.regs, value, shift)
                        }
                        ShiftTy::Lsr => {
                            shifts::lsr_reg_s(&mut emu.arm9.engine_data.regs, value, shift)
                        }
                        ShiftTy::Asr => {
                            shifts::asr_reg_s(&mut emu.arm9.engine_data.regs, value, shift)
                        }
                        ShiftTy::Ror => {
                            shifts::ror_reg_s(&mut emu.arm9.engine_data.regs, value, shift)
                        }
                    }
                } else {
                    match shift_ty {
                        ShiftTy::Lsl => shifts::lsl_reg(value, shift),
                        ShiftTy::Lsr => shifts::lsr_reg(value, shift),
                        ShiftTy::Asr => shifts::asr_reg(value, shift),
                        ShiftTy::Ror => shifts::ror_reg(value, shift),
                    }
                };
                (reg!(emu.arm9, src_reg), op)
            }
        }
    };

    let result = match OP_TY {
        DpOpTy::And => {
            if SET_FLAGS {
                bit_ops::and_s(&mut emu.arm9.engine_data.regs, src, op)
            } else {
                src & op
            }
        }
        DpOpTy::Eor => {
            if SET_FLAGS {
                bit_ops::eor_s(&mut emu.arm9.engine_data.regs, src, op)
            } else {
                src ^ op
            }
        }
        DpOpTy::Sub => {
            if SET_FLAGS {
                arithmetic::sub_s(&mut emu.arm9.engine_data.regs, src, op)
            } else {
                src.wrapping_sub(op)
            }
        }
        DpOpTy::Rsb => {
            if SET_FLAGS {
                arithmetic::sub_s(&mut emu.arm9.engine_data.regs, op, src)
            } else {
                op.wrapping_sub(src)
            }
        }
        DpOpTy::Add => {
            if SET_FLAGS {
                arithmetic::add_s(&mut emu.arm9.engine_data.regs, src, op)
            } else {
                src.wrapping_add(op)
            }
        }
        DpOpTy::Adc => {
            if SET_FLAGS {
                arithmetic::adc_s(&mut emu.arm9.engine_data.regs, src, op)
            } else {
                arithmetic::adc(&emu.arm9.engine_data.regs, src, op)
            }
        }
        DpOpTy::Sbc => {
            if SET_FLAGS {
                arithmetic::adc_s(&mut emu.arm9.engine_data.regs, src, !op)
            } else {
                arithmetic::adc(&emu.arm9.engine_data.regs, src, !op)
            }
        }
        DpOpTy::Rsc => {
            if SET_FLAGS {
                arithmetic::adc_s(&mut emu.arm9.engine_data.regs, op, !src)
            } else {
                arithmetic::adc(&emu.arm9.engine_data.regs, op, !src)
            }
        }
        DpOpTy::Tst => {
            bit_ops::tst(&mut emu.arm9.engine_data.regs, src, op);
            0
        }
        DpOpTy::Teq => {
            bit_ops::teq(&mut emu.arm9.engine_data.regs, src, op);
            0
        }
        DpOpTy::Cmp => {
            arithmetic::cmp(&mut emu.arm9.engine_data.regs, src, op);
            0
        }
        DpOpTy::Cmn => {
            arithmetic::cmn(&mut emu.arm9.engine_data.regs, src, op);
            0
        }
        DpOpTy::Orr => {
            if SET_FLAGS {
                bit_ops::orr_s(&mut emu.arm9.engine_data.regs, src, op)
            } else {
                src | op
            }
        }
        DpOpTy::Mov => {
            if SET_FLAGS {
                bit_ops::set_nz(&mut emu.arm9.engine_data.regs, op);
            }
            op
        }
        DpOpTy::Bic => {
            if SET_FLAGS {
                bit_ops::bic_s(&mut emu.arm9.engine_data.regs, src, op)
            } else {
                src & !op
            }
        }
        DpOpTy::Mvn => {
            let result = !op;
            if SET_FLAGS {
                bit_ops::set_nz(&mut emu.arm9.engine_data.regs, result);
            }
            result
        }
    };
    let dst_reg = (instr >> 12 & 0xF) as u8;
    if OP_TY.is_test() {
        if unlikely(cfg!(feature = "interp-r15-write-checks") && dst_reg == 15) {
            // If the operation is a test, r15 doesn't get written and the pipeline does not get
            // reloaded, but the SPSR is still restored
            restore_spsr(emu);
        }
    } else {
        write_reg_clear_interlock_ab(emu, dst_reg, result);
        if dst_reg == 15 {
            if !matches!(
                OPERAND,
                DpOperand::Reg {
                    shift_imm: false,
                    ..
                }
            ) {
                add_bus_cycles(emu, 1);
            }
            if SET_FLAGS {
                restore_spsr(emu);
                reload_pipeline::<{ StateSource::Cpsr }>(emu);
            } else {
                reload_pipeline::<{ StateSource::Arm }>(emu);
            }
        }
    }
}

pub fn clz(emu: &mut Emu<Engine>, instr: u32) {
    let src_reg = (instr & 0xF) as u8;
    apply_reg_interlock_1::<false>(emu, src_reg);
    add_bus_cycles(emu, 1);
    let result = reg!(emu.arm9, src_reg).leading_zeros();
    prefetch_arm::<true, true>(emu);
    let dst_reg = (instr >> 12 & 0xF) as u8;
    if likely(!cfg!(feature = "interp-r15-write-checks") || dst_reg != 15) {
        write_reg_clear_interlock_ab(emu, dst_reg, result);
    }
}

pub fn mul<const ACC: bool, const SET_FLAGS: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let src_reg = (instr & 0xF) as u8;
    let op_reg = (instr >> 8 & 0xF) as u8;
    let acc_reg = (instr >> 12 & 0xF) as u8;
    if ACC {
        apply_reg_interlocks_3::<0, true>(emu, src_reg, op_reg, acc_reg);
    } else {
        apply_reg_interlocks_2::<0, false>(emu, src_reg, op_reg);
    }
    add_bus_cycles(emu, 2);
    let mut result = reg!(emu.arm9, src_reg).wrapping_mul(reg!(emu.arm9, op_reg));
    prefetch_arm::<true, true>(emu);
    add_cycles(emu, if SET_FLAGS { 3 } else { 1 });
    if ACC {
        result = result.wrapping_add(reg!(emu.arm9, acc_reg));
    }
    if SET_FLAGS {
        bit_ops::set_nz(&mut emu.arm9.engine_data.regs, result);
    }
    let dst_reg = (instr >> 16 & 0xF) as u8;
    if likely(!cfg!(feature = "interp-r15-write-checks") || dst_reg != 15) {
        if SET_FLAGS {
            reg!(emu.arm9, dst_reg) = result;
        } else {
            write_reg_interlock_ab(emu, dst_reg, result, 1);
        }
    }
}

// NOTE: For long multiplies, the ARM9 first calculates the low half of the result, and then the
// high half; this presumably implies that:
// - If RdHi == RdLo, the high half of the result gets written to the register.
// - For UMLAL/SMLAL, RdLo is read first, and then RdHi, so only RdLo can cause an interlock in
//   the multiplication instruction.
// - Only RdHi can cause an interlock in subsequent instructions, RdLo is immediately available.

pub fn umull<const ACC: bool, const SET_FLAGS: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let src_reg = (instr & 0xF) as u8;
    let op_reg = (instr >> 8 & 0xF) as u8;
    let dst_acc_reg_low = (instr >> 12 & 0xF) as u8;
    if ACC {
        apply_reg_interlocks_3::<0, true>(emu, src_reg, op_reg, dst_acc_reg_low);
    } else {
        apply_reg_interlocks_2::<0, false>(emu, src_reg, op_reg);
    }
    add_bus_cycles(emu, 2);
    let mut result = (reg!(emu.arm9, src_reg) as u64).wrapping_mul(reg!(emu.arm9, op_reg) as u64);
    prefetch_arm::<true, true>(emu);
    add_cycles(emu, if SET_FLAGS { 4 } else { 2 });
    let dst_acc_reg_high = (instr >> 16 & 0xF) as u8;
    if ACC {
        result = result.wrapping_add(
            (reg!(emu.arm9, dst_acc_reg_high) as u64) << 32
                | reg!(emu.arm9, dst_acc_reg_low) as u64,
        );
    }
    if SET_FLAGS {
        bit_ops::set_nz_64(&mut emu.arm9.engine_data.regs, result);
    }
    if likely(!cfg!(feature = "interp-r15-write-checks") || dst_acc_reg_low != 15) {
        reg!(emu.arm9, dst_acc_reg_low) = result as u32;
    }
    if likely(!cfg!(feature = "interp-r15-write-checks") || dst_acc_reg_high != 15) {
        if SET_FLAGS {
            reg!(emu.arm9, dst_acc_reg_high) = (result >> 32) as u32;
        } else {
            write_reg_interlock_ab(emu, dst_acc_reg_high, (result >> 32) as u32, 1);
        }
    }
}

pub fn smull<const ACC: bool, const SET_FLAGS: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let src_reg = (instr & 0xF) as u8;
    let op_reg = (instr >> 8 & 0xF) as u8;
    let dst_acc_reg_low = (instr >> 12 & 0xF) as u8;
    if ACC {
        apply_reg_interlocks_3::<0, true>(emu, src_reg, op_reg, dst_acc_reg_low);
    } else {
        apply_reg_interlocks_2::<0, false>(emu, src_reg, op_reg);
    }
    add_bus_cycles(emu, 2);
    let mut result = (reg!(emu.arm9, src_reg) as i32 as i64)
        .wrapping_mul(reg!(emu.arm9, op_reg) as i32 as i64) as u64;
    prefetch_arm::<true, true>(emu);
    add_cycles(emu, if SET_FLAGS { 4 } else { 2 });
    let dst_acc_reg_high = (instr >> 16 & 0xF) as u8;
    if ACC {
        result = result.wrapping_add(
            (reg!(emu.arm9, dst_acc_reg_high) as u64) << 32
                | reg!(emu.arm9, dst_acc_reg_low) as u64,
        );
    }
    if SET_FLAGS {
        bit_ops::set_nz_64(&mut emu.arm9.engine_data.regs, result);
    }
    if likely(!cfg!(feature = "interp-r15-write-checks") || dst_acc_reg_low != 15) {
        reg!(emu.arm9, dst_acc_reg_low) = result as u32;
    }
    if likely(!cfg!(feature = "interp-r15-write-checks") || dst_acc_reg_high != 15) {
        if SET_FLAGS {
            reg!(emu.arm9, dst_acc_reg_high) = (result >> 32) as u32;
        } else {
            write_reg_interlock_ab(emu, dst_acc_reg_high, (result >> 32) as u32, 1);
        }
    }
}

pub fn smulxy<const ACC: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let src_reg = (instr & 0xF) as u8;
    let op_reg = (instr >> 8 & 0xF) as u8;
    let acc_reg = (instr >> 12 & 0xF) as u8;
    if ACC {
        apply_reg_interlocks_3::<0, true>(emu, src_reg, op_reg, acc_reg);
    } else {
        apply_reg_interlocks_2::<0, false>(emu, src_reg, op_reg);
    }
    add_bus_cycles(emu, 1);
    let src = (reg!(emu.arm9, src_reg) >> (instr >> 1 & 0x10)) as i16 as i32;
    let op = (reg!(emu.arm9, op_reg) >> (instr >> 2 & 0x10)) as i16 as i32;
    let mut result = src * op;
    prefetch_arm::<true, true>(emu);
    if ACC {
        let (new_res, overflow) = result.overflowing_add(reg!(emu.arm9, acc_reg) as i32);
        result = new_res;
        if overflow {
            emu.arm9.engine_data.regs.cpsr.set_sticky_overflow(true);
        }
    }
    let dst_reg = (instr >> 16 & 0xF) as u8;
    if likely(!cfg!(feature = "interp-r15-write-checks") || dst_reg != 15) {
        write_reg_interlock_ab(emu, dst_reg, result as u32, 1);
    }
}

pub fn smulwy<const ACC: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let src_reg = (instr & 0xF) as u8;
    let op_reg = (instr >> 8 & 0xF) as u8;
    let acc_reg = (instr >> 12 & 0xF) as u8;
    if ACC {
        apply_reg_interlocks_3::<0, true>(emu, src_reg, op_reg, acc_reg);
    } else {
        apply_reg_interlocks_2::<0, false>(emu, src_reg, op_reg);
    }
    add_bus_cycles(emu, 1);
    let src = reg!(emu.arm9, src_reg) as i32 as i64;
    let op = (reg!(emu.arm9, op_reg) >> (instr >> 2 & 0x10)) as i16 as i64;
    let mut result = ((src * op) >> 16) as i32;
    prefetch_arm::<true, true>(emu);
    if ACC {
        let (new_res, overflow) = result.overflowing_add(reg!(emu.arm9, acc_reg) as i32);
        result = new_res;
        if overflow {
            emu.arm9.engine_data.regs.cpsr.set_sticky_overflow(true);
        }
    }
    let dst_reg = (instr >> 16 & 0xF) as u8;
    if likely(!cfg!(feature = "interp-r15-write-checks") || dst_reg != 15) {
        write_reg_interlock_ab(emu, dst_reg, result as u32, 1);
    }
}

pub fn smlalxy(emu: &mut Emu<Engine>, instr: u32) {
    let src_reg = (instr & 0xF) as u8;
    let op_reg = (instr >> 8 & 0xF) as u8;
    let dst_acc_reg_low = (instr >> 12 & 0xF) as u8;
    apply_reg_interlocks_3::<0, true>(emu, src_reg, op_reg, dst_acc_reg_low);
    add_bus_cycles(emu, 2);
    let src = (reg!(emu.arm9, src_reg) >> (instr >> 1 & 0x10)) as i16 as i32;
    let op = (reg!(emu.arm9, op_reg) >> (instr >> 2 & 0x10)) as i16 as i32;
    let mut result = (src * op) as u64;
    prefetch_arm::<true, true>(emu);
    let dst_acc_reg_high = (instr >> 16 & 0xF) as u8;
    result = result.wrapping_add(
        (reg!(emu.arm9, dst_acc_reg_high) as u64) << 32 | reg!(emu.arm9, dst_acc_reg_low) as u64,
    );
    if likely(!cfg!(feature = "interp-r15-write-checks") || dst_acc_reg_low != 15) {
        reg!(emu.arm9, dst_acc_reg_low) = result as u32;
    }
    if likely(!cfg!(feature = "interp-r15-write-checks") || dst_acc_reg_high != 15) {
        write_reg_interlock_ab(emu, dst_acc_reg_high, (result >> 32) as u32, 1);
    }
}

pub fn qaddsub<const SUB: bool, const DOUBLED: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let src_reg = (instr & 0xF) as u8;
    let op_reg = (instr >> 16 & 0xF) as u8;
    apply_reg_interlocks_2::<0, false>(emu, src_reg, op_reg);
    add_bus_cycles(emu, 1);
    let src = reg!(emu.arm9, src_reg) as i32;
    let mut op = reg!(emu.arm9, op_reg) as i32;
    if DOUBLED {
        if (op ^ op << 1) & 1 << 31 == 0 {
            op <<= 1;
        } else {
            op = if op < 0 { i32::MIN } else { i32::MAX };
            emu.arm9.engine_data.regs.cpsr.set_sticky_overflow(true);
        }
    }
    let (mut result, overflow) = if SUB {
        src.overflowing_sub(op)
    } else {
        src.overflowing_add(op)
    };
    if overflow {
        result = if result < 0 { i32::MAX } else { i32::MIN };
        emu.arm9.engine_data.regs.cpsr.set_sticky_overflow(true);
    }
    prefetch_arm::<true, true>(emu);
    let dst_reg = (instr >> 12 & 0xF) as u8;
    if likely(!cfg!(feature = "interp-r15-write-checks") || dst_reg != 15) {
        write_reg_interlock_ab(emu, dst_reg, result as u32, 1);
    }
}
