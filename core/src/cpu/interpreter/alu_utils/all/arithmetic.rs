use super::super::super::Regs;

pub fn add_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let (result, overflow) = (a as i32).overflowing_add(b as i32);
    regs.cpsr = regs
        .cpsr
        .with_carry(a > !b)
        .with_overflow(overflow)
        .with_negative(result >> 31 != 0)
        .with_zero(result == 0);
    result as u32
}

pub fn cmn(regs: &mut Regs, a: u32, b: u32) {
    add_s(regs, a, b);
}

pub fn adc(regs: &Regs, a: u32, b: u32) -> u32 {
    a.wrapping_add(b).wrapping_add(regs.cpsr.raw() >> 29 & 1)
}

pub fn adc_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let carry = regs.cpsr.raw() >> 29 & 1;
    let result = a as u64 + b as u64 + carry as u64;
    regs.cpsr = regs
        .cpsr
        .with_carry(result >> 32 != 0)
        .with_overflow(!(a ^ b) & (a ^ result as u32) & 1 << 31 != 0)
        .with_negative(result & 1 << 31 != 0)
        .with_zero(result == 0);
    result as u32
}

pub fn sub_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let (result, overflow) = (a as i32).overflowing_sub(b as i32);
    regs.cpsr = regs
        .cpsr
        .with_carry(a >= b)
        .with_overflow(overflow)
        .with_negative(result >> 31 != 0)
        .with_zero(result == 0);
    result as u32
}

pub fn cmp(regs: &mut Regs, a: u32, b: u32) {
    sub_s(regs, a, b);
}
