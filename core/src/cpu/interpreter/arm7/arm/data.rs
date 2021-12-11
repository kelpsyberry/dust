use super::super::{
    super::{
        alu_utils::{arithmetic, bit_ops, shifts},
        common::{DpOpTy, DpOperand, ShiftTy, StateSource},
        Engine,
    },
    add_cycles, multiply_cycles, reload_pipeline, restore_spsr,
};
use crate::{emu::Emu, utils::schedule::RawTimestamp};
use core::intrinsics::unlikely;

pub fn dp_op<const OP_TY: DpOpTy, const OPERAND: DpOperand, const SET_FLAGS: bool>(
    emu: &mut Emu<Engine>,
    instr: u32,
) {
    let (src, op) = match OPERAND {
        DpOperand::Imm => {
            let value = instr & 0xFF;
            let shift = instr >> 7 & 0x1E;
            // Don't calculate the shifter carry for arithmetic instructions, as they'll overwrite
            // the flag anyway
            let op = if SET_FLAGS && !OP_TY.sets_carry() {
                shifts::ror_imm_s_no_rrx(&mut emu.arm7.engine_data.regs, value, shift as u8)
            } else {
                value.rotate_right(shift)
            };
            let src = reg!(emu.arm7, instr >> 16 & 0xF);
            inc_r15!(emu.arm7, 4);
            (src, op)
        }
        DpOperand::Reg {
            shift_ty,
            shift_imm,
        } => {
            let op_reg = instr & 0xF;
            if shift_imm {
                let shift = (instr >> 7 & 0x1F) as u8;
                let value = reg!(emu.arm7, op_reg);
                let op = if SET_FLAGS && !OP_TY.sets_carry() {
                    match shift_ty {
                        ShiftTy::Lsl => {
                            shifts::lsl_imm_s(&mut emu.arm7.engine_data.regs, value, shift)
                        }
                        ShiftTy::Lsr => {
                            shifts::lsr_imm_s(&mut emu.arm7.engine_data.regs, value, shift)
                        }
                        ShiftTy::Asr => {
                            shifts::asr_imm_s(&mut emu.arm7.engine_data.regs, value, shift)
                        }
                        ShiftTy::Ror => {
                            shifts::ror_imm_s(&mut emu.arm7.engine_data.regs, value, shift)
                        }
                    }
                } else {
                    match shift_ty {
                        ShiftTy::Lsl => shifts::lsl_imm(value, shift),
                        ShiftTy::Lsr => shifts::lsr_imm(value, shift),
                        ShiftTy::Asr => shifts::asr_imm(value, shift),
                        ShiftTy::Ror => shifts::ror_imm(&emu.arm7.engine_data.regs, value, shift),
                    }
                };
                let src = reg!(emu.arm7, instr >> 16 & 0xF);
                inc_r15!(emu.arm7, 4);
                (src, op)
            } else {
                let shift = reg!(emu.arm7, instr >> 8 & 0xF) as u8;
                inc_r15!(emu.arm7, 4);
                add_cycles(emu, 1);
                emu.arm7.engine_data.prefetch_nseq = true;
                let value = reg!(emu.arm7, op_reg);
                let op = if SET_FLAGS && !OP_TY.sets_carry() {
                    match shift_ty {
                        ShiftTy::Lsl => {
                            shifts::lsl_reg_s(&mut emu.arm7.engine_data.regs, value, shift)
                        }
                        ShiftTy::Lsr => {
                            shifts::lsr_reg_s(&mut emu.arm7.engine_data.regs, value, shift)
                        }
                        ShiftTy::Asr => {
                            shifts::asr_reg_s(&mut emu.arm7.engine_data.regs, value, shift)
                        }
                        ShiftTy::Ror => {
                            shifts::ror_reg_s(&mut emu.arm7.engine_data.regs, value, shift)
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
                (reg!(emu.arm7, instr >> 16 & 0xF), op)
            }
        }
    };

    let result = match OP_TY {
        DpOpTy::And => {
            if SET_FLAGS {
                bit_ops::and_s(&mut emu.arm7.engine_data.regs, src, op)
            } else {
                src & op
            }
        }
        DpOpTy::Eor => {
            if SET_FLAGS {
                bit_ops::eor_s(&mut emu.arm7.engine_data.regs, src, op)
            } else {
                src ^ op
            }
        }
        DpOpTy::Sub => {
            if SET_FLAGS {
                arithmetic::sub_s(&mut emu.arm7.engine_data.regs, src, op)
            } else {
                src.wrapping_sub(op)
            }
        }
        DpOpTy::Rsb => {
            if SET_FLAGS {
                arithmetic::sub_s(&mut emu.arm7.engine_data.regs, op, src)
            } else {
                op.wrapping_sub(src)
            }
        }
        DpOpTy::Add => {
            if SET_FLAGS {
                arithmetic::add_s(&mut emu.arm7.engine_data.regs, src, op)
            } else {
                src.wrapping_add(op)
            }
        }
        DpOpTy::Adc => {
            if SET_FLAGS {
                arithmetic::adc_s(&mut emu.arm7.engine_data.regs, src, op)
            } else {
                arithmetic::adc(&emu.arm7.engine_data.regs, src, op)
            }
        }
        DpOpTy::Sbc => {
            if SET_FLAGS {
                arithmetic::adc_s(&mut emu.arm7.engine_data.regs, src, !op)
            } else {
                arithmetic::adc(&emu.arm7.engine_data.regs, src, !op)
            }
        }
        DpOpTy::Rsc => {
            if SET_FLAGS {
                arithmetic::adc_s(&mut emu.arm7.engine_data.regs, op, !src)
            } else {
                arithmetic::adc(&emu.arm7.engine_data.regs, op, !src)
            }
        }
        DpOpTy::Tst => {
            bit_ops::tst(&mut emu.arm7.engine_data.regs, src, op);
            0
        }
        DpOpTy::Teq => {
            bit_ops::teq(&mut emu.arm7.engine_data.regs, src, op);
            0
        }
        DpOpTy::Cmp => {
            arithmetic::cmp(&mut emu.arm7.engine_data.regs, src, op);
            0
        }
        DpOpTy::Cmn => {
            arithmetic::cmn(&mut emu.arm7.engine_data.regs, src, op);
            0
        }
        DpOpTy::Orr => {
            if SET_FLAGS {
                bit_ops::orr_s(&mut emu.arm7.engine_data.regs, src, op)
            } else {
                src | op
            }
        }
        DpOpTy::Mov => {
            if SET_FLAGS {
                bit_ops::set_nz(&mut emu.arm7.engine_data.regs, op);
            }
            op
        }
        DpOpTy::Bic => {
            if SET_FLAGS {
                bit_ops::bic_s(&mut emu.arm7.engine_data.regs, src, op)
            } else {
                src & !op
            }
        }
        DpOpTy::Mvn => {
            let result = !op;
            if SET_FLAGS {
                bit_ops::set_nz(&mut emu.arm7.engine_data.regs, result);
            }
            result
        }
    };
    let dst_reg = instr >> 12 & 0xF;
    if OP_TY.is_test() {
        if unlikely(cfg!(feature = "interp-r15-write-checks") && dst_reg == 15) {
            // If the operation is a test, r15 doesn't get written and the pipeline does not get
            // reloaded, but the SPSR is still restored
            restore_spsr(emu);
        }
    } else {
        reg!(emu.arm7, dst_reg) = result;
        if dst_reg == 15 {
            if SET_FLAGS {
                restore_spsr(emu);
                reload_pipeline::<{ StateSource::Cpsr }>(emu);
            } else {
                reload_pipeline::<{ StateSource::Arm }>(emu);
            }
        }
    }
}

pub fn mul<const ACC: bool, const SET_FLAGS: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let src = reg!(emu.arm7, instr & 0xF);
    let op = reg!(emu.arm7, instr >> 8 & 0xF);
    let dst_reg = instr >> 16 & 0xF;
    let mut result = src.wrapping_mul(op);
    inc_r15!(emu.arm7, 4);
    if ACC {
        result = result.wrapping_add(reg!(emu.arm7, instr >> 12 & 0xF));
    }
    if SET_FLAGS {
        // TODO: What's the value of the carry flag?
        bit_ops::set_nz(&mut emu.arm7.engine_data.regs, result);
    }
    #[cfg(feature = "interp-r15-write-checks")]
    if unlikely(dst_reg == 15) {
        unimplemented!("{} r15 write", if ACC { "MLA" } else { "MUL" });
    }
    reg!(emu.arm7, dst_reg) = result;
    add_cycles(emu, multiply_cycles(op) + ACC as RawTimestamp);
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn umull<const ACC: bool, const SET_FLAGS: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let src = reg!(emu.arm7, instr & 0xF);
    let op = reg!(emu.arm7, instr >> 8 & 0xF);
    let dst_acc_reg_low = instr >> 12 & 0xF;
    let dst_acc_reg_high = instr >> 16 & 0xF;
    let mut result = (src as u64).wrapping_mul(op as u64);
    inc_r15!(emu.arm7, 4);
    if ACC {
        result = result.wrapping_add(
            (reg!(emu.arm7, dst_acc_reg_high) as u64) << 32
                | reg!(emu.arm7, dst_acc_reg_low) as u64,
        );
    }
    if SET_FLAGS {
        // TODO: What's the value of the carry flag?
        bit_ops::set_nz_64(&mut emu.arm7.engine_data.regs, result);
    }
    #[cfg(feature = "interp-r15-write-checks")]
    if unlikely(dst_acc_reg_low == 15 || dst_acc_reg_high == 15) {
        unimplemented!("U{}L r15 write", if ACC { "MLA" } else { "MUL" });
    }
    // NOTE: The order of operations here is important, as if hi == lo, hi has precedence
    reg!(emu.arm7, dst_acc_reg_low) = result as u32;
    reg!(emu.arm7, dst_acc_reg_high) = (result >> 32) as u32;
    add_cycles(emu, multiply_cycles(op) + 1 + ACC as RawTimestamp);
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn smull<const ACC: bool, const SET_FLAGS: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let src = reg!(emu.arm7, instr & 0xF);
    let op = reg!(emu.arm7, instr >> 8 & 0xF);
    let dst_acc_reg_low = instr >> 12 & 0xF;
    let dst_acc_reg_high = instr >> 16 & 0xF;
    let mut result = (src as i32 as i64).wrapping_mul(op as i32 as i64) as u64;
    inc_r15!(emu.arm7, 4);
    if ACC {
        result = result.wrapping_add(
            (reg!(emu.arm7, dst_acc_reg_high) as u64) << 32
                | reg!(emu.arm7, dst_acc_reg_low) as u64,
        );
    }
    if SET_FLAGS {
        // TODO: What's the value of the carry flag?
        bit_ops::set_nz_64(&mut emu.arm7.engine_data.regs, result);
    }
    #[cfg(feature = "interp-r15-write-checks")]
    if unlikely(dst_acc_reg_low == 15 || dst_acc_reg_high == 15) {
        unimplemented!("S{}L r15 write", if ACC { "MLA" } else { "MUL" });
    }
    // NOTE: The order of operations here is important, as if hi == lo, hi has precedence
    reg!(emu.arm7, dst_acc_reg_low) = result as u32;
    reg!(emu.arm7, dst_acc_reg_high) = (result >> 32) as u32;
    add_cycles(emu, multiply_cycles(op) + 1 + ACC as RawTimestamp);
    emu.arm7.engine_data.prefetch_nseq = true;
}
