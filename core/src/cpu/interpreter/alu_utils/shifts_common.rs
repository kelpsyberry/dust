use super::super::Regs;

pub fn lsl_imm(value: u32, shift: u8) -> u32 {
    value << shift
}

pub fn lsr_imm(value: u32, shift: u8) -> u32 {
    value.wrapping_shr(shift.wrapping_sub(1) as u32) >> 1
}

pub fn asr_imm(value: u32, shift: u8) -> u32 {
    ((value as i32).wrapping_shr(shift.wrapping_sub(1) as u32) >> 1) as u32
}

pub fn asr_reg(value: u32, shift: u8) -> u32 {
    (value as i32 >> shift.min(31)) as u32
}

pub fn ror_imm(regs: &Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        super::shifts::rrx(regs, value)
    } else {
        value.rotate_right(shift as u32)
    }
}

pub fn ror_reg(value: u32, shift: u8) -> u32 {
    value.rotate_right(shift as u32)
}
