use super::super::super::Regs;
use crate::cpu::psr::Psr;

pub fn add_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result: u32;
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "add {a_res:e}, {b:e}",
            "lahf",
            "seto al",
            "ror al, 1",
            "shr ax, 1",
            "shr ah, 5",
            "shl eax, 22",
            a_res = inlateout(reg) a => result,
            b = in(reg) b,
            lateout("eax") flags,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & 0x0FFF_FFFF) | flags);
    result
}

pub fn cmn(regs: &mut Regs, a: u32, b: u32) {
    add_s(regs, a, b);
}

pub fn adc(regs: &Regs, a: u32, b: u32) -> u32 {
    let result;
    unsafe {
        core::arch::asm!(
            "bt {cpsr:e}, 29",
            "adc {a_res:e}, {b:e}",
            a_res = inlateout(reg) a => result,
            b = in(reg) b,
            cpsr = in(reg) regs.cpsr.raw(),
            options(pure, nomem, nostack),
        );
    }
    result
}

pub fn adc_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result: u32;
    let flags: u32;
    let cpsr = regs.cpsr.raw();
    unsafe {
        core::arch::asm!(
            "bt {cpsr:e}, 29",
            "adc {a_res:e}, {b:e}",
            "lahf",
            "seto al",
            "ror al, 1",
            "shr ax, 1",
            "shr ah, 5",
            "shl eax, 22",
            a_res = inlateout(reg) a => result,
            b = in(reg) b,
            cpsr = in(reg) cpsr,
            lateout("eax") flags,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((cpsr & 0x0FFF_FFFF) | flags);
    result
}

pub fn sub_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result: u32;
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "sub {a_res:e}, {b:e}",
            "cmc",
            "lahf",
            "seto al",
            "ror al, 1",
            "shr ax, 1",
            "shr ah, 5",
            "shl eax, 22",
            a_res = inlateout(reg) a => result,
            b = in(reg) b,
            lateout("eax") flags,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & 0x0FFF_FFFF) | flags);
    result
}

pub fn cmp(regs: &mut Regs, a: u32, b: u32) {
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "cmp {a:e}, {b:e}",
            "cmc",
            "lahf",
            "seto al",
            "ror al, 1",
            "shr ax, 1",
            "shr ah, 5",
            "shl eax, 22",
            a = in(reg) a,
            b = in(reg) b,
            lateout("eax") flags,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & 0x0FFF_FFFF) | flags);
}
