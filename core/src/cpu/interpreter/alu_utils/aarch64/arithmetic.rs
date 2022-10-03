use super::super::super::Regs;
use crate::cpu::psr::Psr;

pub fn add_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result: u32;
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "adds {result:w}, {a:w}, {b:w}",
            "mrs {flags:x}, nzcv",
            a = in(reg) a,
            b = in(reg) b,
            flags = lateout(reg) flags,
            result = lateout(reg) result,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0xF000_0000) | flags);
    result
}

pub fn cmn(regs: &mut Regs, a: u32, b: u32) {
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "cmn {a:w}, {b:w}",
            "mrs {flags:x}, nzcv",
            a = in(reg) a,
            b = in(reg) b,
            flags = lateout(reg) flags,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0xF000_0000) | flags);
}

pub fn adc(regs: &Regs, a: u32, b: u32) -> u32 {
    let result: u32;
    unsafe {
        core::arch::asm!(
            "rmif {flags:x}, #28, #2",
            "adc {result:w}, {a:w}, {b:w}",
            a = in(reg) a,
            b = in(reg) b,
            flags = in(reg) regs.cpsr.raw(),
            result = lateout(reg) result,
            options(pure, nomem, nostack),
        );
    }
    result
}

pub fn adc_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let mut flags = regs.cpsr.raw();
    let result: u32;
    unsafe {
        core::arch::asm!(
            "rmif {flags:x}, #28, #2",
            "adcs {result:w}, {a:w}, {b:w}",
            "mrs {flags:x}, nzcv",
            a = in(reg) a,
            b = in(reg) b,
            flags = inlateout(reg) flags,
            result = lateout(reg) result,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0xF000_0000) | flags);
    result
}

pub fn sub_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result: u32;
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "subs {result:w}, {a:w}, {b:w}",
            "mrs {flags:x}, nzcv",
            a = in(reg) a,
            b = in(reg) b,
            flags = lateout(reg) flags,
            result = lateout(reg) result,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0xF000_0000) | flags);
    result
}

pub fn cmp(regs: &mut Regs, a: u32, b: u32) {
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "cmp {a:w}, {b:w}",
            "mrs {flags:x}, nzcv",
            a = in(reg) a,
            b = in(reg) b,
            flags = lateout(reg) flags,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0xF000_0000) | flags);
}
