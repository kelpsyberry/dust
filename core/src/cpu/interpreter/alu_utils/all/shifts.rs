pub use super::super::shifts_common::*;

use super::super::super::Regs;

fn lsl_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    regs.cpsr.set_carry(value & 1 << (32 - shift) != 0);
    value << shift
}

pub fn lsl_imm_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else {
        lsl_s(regs, value, shift)
    }
}

pub fn lsl_reg(value: u32, shift: u8) -> u32 {
    if shift < 32 {
        value << shift
    } else {
        0
    }
}

pub fn lsl_reg_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else if shift < 33 {
        lsl_s(regs, value << (shift - 1), 1)
    } else {
        regs.cpsr.set_carry(false);
        0
    }
}

fn lsr_1_s(regs: &mut Regs, value: u32) -> u32 {
    regs.cpsr.set_carry(value & 1 != 0);
    value >> 1
}

pub fn lsr_imm_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    lsr_1_s(regs, value.wrapping_shr(shift.wrapping_sub(1) as u32))
}

pub fn lsr_reg(value: u32, shift: u8) -> u32 {
    if shift < 32 {
        value >> shift
    } else {
        0
    }
}

pub fn lsr_reg_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else if shift < 33 {
        lsr_1_s(regs, value >> (shift - 1))
    } else {
        regs.cpsr.set_carry(false);
        0
    }
}

fn asr_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    regs.cpsr.set_carry(value & 1 << (shift - 1) != 0);
    (value as i32 >> shift) as u32
}

pub fn asr_imm_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    asr_s(
        regs,
        (value as i32).wrapping_shr(shift.wrapping_sub(1) as u32) as u32,
        1,
    )
}

pub fn asr_reg_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else if shift < 32 {
        asr_s(regs, value, shift)
    } else {
        let result = ((value as i32) >> 31) as u32;
        regs.cpsr.set_carry(result != 0);
        result
    }
}

pub fn rrx(regs: &Regs, value: u32) -> u32 {
    (regs.cpsr.raw() << 2 & 0x8000_0000) | value >> 1
}

fn rrx_s(regs: &mut Regs, value: u32) -> u32 {
    let result = rrx(regs, value);
    regs.cpsr.set_carry(value & 1 != 0);
    result
}

pub fn ror_imm_s_no_rrx(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else {
        regs.cpsr.set_carry(value & 1 << (shift - 1) != 0);
        value.rotate_right(shift as u32)
    }
}

pub fn ror_imm_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        rrx_s(regs, value)
    } else {
        regs.cpsr.set_carry(value & 1 << (shift - 1) != 0);
        value.rotate_right(shift as u32)
    }
}

pub fn ror_reg_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else {
        regs.cpsr
            .set_carry(value & 1_u32.wrapping_shl((shift - 1) as u32) != 0);
        value.rotate_right(shift as u32)
    }
}
