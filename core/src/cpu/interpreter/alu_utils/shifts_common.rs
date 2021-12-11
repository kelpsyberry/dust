pub fn lsl_imm(value: u32, shift: u8) -> u32 {
    value << shift
}

pub fn lsr_imm(value: u32, shift: u8) -> u32 {
    value.wrapping_shr(shift.wrapping_sub(1) as u32) >> 1
}

pub fn asr_imm(value: u32, shift: u8) -> u32 {
    ((value as i32).wrapping_shr(shift.wrapping_sub(1) as u32) >> 1) as u32
}

pub fn ror_reg(value: u32, shift: u8) -> u32 {
    value.rotate_right(shift as u32)
}
