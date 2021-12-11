use super::super::super::Regs;

pub fn set_nz(regs: &mut Regs, value: u32) {
    regs.cpsr = regs
        .cpsr
        .with_negative(value >> 31 != 0)
        .with_zero(value == 0);
}

pub fn set_nz_64(regs: &mut Regs, value: u64) {
    regs.cpsr = regs
        .cpsr
        .with_negative(value >> 63 != 0)
        .with_zero(value == 0);
}

pub fn and_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result = a & b;
    set_nz(regs, result);
    result
}

pub fn tst(regs: &mut Regs, a: u32, b: u32) {
    and_s(regs, a, b);
}

pub fn eor_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result = a ^ b;
    set_nz(regs, result);
    result
}

pub fn teq(regs: &mut Regs, a: u32, b: u32) {
    eor_s(regs, a, b);
}

pub fn orr_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result = a | b;
    set_nz(regs, result);
    result
}

pub fn bic_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result = a & !b;
    set_nz(regs, result);
    result
}
