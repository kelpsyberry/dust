use super::super::super::Regs;
use crate::cpu::psr::Psr;

pub fn set_nz(regs: &mut Regs, value: u32) {
    tst(regs, value, value);
}

pub fn set_nz_64(regs: &mut Regs, value: u64) {
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "tst {value:x}, {value:x}",
            "mrs {flags:x}, nzcv",
            value = in(reg) value,
            flags = lateout(reg) flags,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0xC000_0000) | (flags & 0xC000_0000));
}

pub fn and_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result: u32;
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "ands {result:w}, {a:w}, {b:w}",
            "mrs {flags:x}, nzcv",
            a = in(reg) a,
            b = in(reg) b,
            result = lateout(reg) result,
            flags = lateout(reg) flags,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0xC000_0000) | (flags & 0xC000_0000));
    result
}

pub fn tst(regs: &mut Regs, a: u32, b: u32) {
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "tst {a:w}, {b:w}",
            "mrs {flags:x}, nzcv",
            a = in(reg) a,
            b = in(reg) b,
            flags = lateout(reg) flags,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0xC000_0000) | (flags & 0xC000_0000));
}

pub fn eor_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result: u32;
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "eor {result:w}, {a:w}, {b:w}",
            "tst {result:w}, {result:w}",
            "mrs {flags:x}, nzcv",
            a = in(reg) a,
            b = in(reg) b,
            result = lateout(reg) result,
            flags = lateout(reg) flags,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0xC000_0000) | (flags & 0xC000_0000));
    result
}

pub fn teq(regs: &mut Regs, a: u32, b: u32) {
    eor_s(regs, a, b);
}

pub fn orr_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result: u32;
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "orr {result:w}, {a:w}, {b:w}",
            "tst {result:w}, {result:w}",
            "mrs {flags:x}, nzcv",
            a = in(reg) a,
            b = in(reg) b,
            result = lateout(reg) result,
            flags = lateout(reg) flags,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0xC000_0000) | (flags & 0xC000_0000));
    result
}

pub fn bic_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    let result: u32;
    let flags: u32;
    unsafe {
        core::arch::asm!(
            "bics {result:w}, {a:w}, {b:w}",
            "mrs {flags:x}, nzcv",
            a = in(reg) a,
            b = in(reg) b,
            result = lateout(reg) result,
            flags = lateout(reg) flags,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0xC000_0000) | (flags & 0xC000_0000));
    result
}
