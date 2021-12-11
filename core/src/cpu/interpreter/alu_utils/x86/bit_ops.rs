use super::super::super::Regs;
use crate::cpu::psr::Cpsr;

pub fn set_nz(regs: &mut Regs, value: u32) {
    tst(regs, value, value);
}

pub fn set_nz_64(regs: &mut Regs, value: u64) {
    unsafe {
        let flags: u32;
        asm!(
            "test {value:r}, {value:r}",
            "lahf",
            "and ax, 0xC000",
            "shl eax, 16",
            value = in(reg) value,
            lateout("eax") flags,
            options(pure, nomem, nostack),
        );
        regs.cpsr = Cpsr::from_raw_unchecked((regs.cpsr.raw() & !0xC000_0000) | flags);
    }
}

pub fn and_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    unsafe {
        let result: u32;
        let flags: u32;
        asm!(
            "and {a_res:e}, {b:e}",
            "lahf",
            "and ax, 0xC000",
            "shl eax, 16",
            a_res = inlateout(reg) a => result,
            b = in(reg) b,
            lateout("eax") flags,
            options(pure, nomem, nostack),
        );
        regs.cpsr = Cpsr::from_raw_unchecked((regs.cpsr.raw() & !0xC000_0000) | flags);
        result
    }
}

pub fn tst(regs: &mut Regs, a: u32, b: u32) {
    unsafe {
        let flags: u32;
        asm!(
            "test {a:e}, {b:e}",
            "lahf",
            "and ax, 0xC000",
            "shl eax, 16",
            a = in(reg) a,
            b = in(reg) b,
            lateout("eax") flags,
            options(pure, nomem, nostack),
        );
        regs.cpsr = Cpsr::from_raw_unchecked((regs.cpsr.raw() & !0xC000_0000) | flags);
    }
}

pub fn eor_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    unsafe {
        let result: u32;
        let flags: u32;
        asm!(
            "xor {a_res:e}, {b:e}",
            "lahf",
            "and ax, 0xC000",
            "shl eax, 16",
            a_res = inlateout(reg) a => result,
            b = in(reg) b,
            lateout("eax") flags,
            options(pure, nomem, nostack),
        );
        regs.cpsr = Cpsr::from_raw_unchecked((regs.cpsr.raw() & !0xC000_0000) | flags);
        result
    }
}

pub fn teq(regs: &mut Regs, a: u32, b: u32) {
    eor_s(regs, a, b);
}

pub fn orr_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    unsafe {
        let result: u32;
        let flags: u32;
        asm!(
            "or {a_res:e}, {b:e}",
            "lahf",
            "and ax, 0xC000",
            "shl eax, 16",
            a_res = inlateout(reg) a => result,
            b = in(reg) b,
            lateout("eax") flags,
            options(pure, nomem, nostack),
        );
        regs.cpsr = Cpsr::from_raw_unchecked((regs.cpsr.raw() & !0xC000_0000) | flags);
        result
    }
}

pub fn bic_s(regs: &mut Regs, a: u32, b: u32) -> u32 {
    unsafe {
        let result: u32;
        let flags: u32;
        #[cfg(target_feature = "bmi1")]
        asm!(
            "andn {res:e}, {b:e}, {a:e}",
            "lahf",
            "and ax, 0xC000",
            "shl eax, 16",
            a = in(reg) a,
            b = in(reg) b,
            res = lateout(reg) result,
            lateout("eax") flags,
            options(pure, nomem, nostack),
        );
        #[cfg(not(target_feature = "bmi1"))]
        asm!(
            "and {a_res:e}, {b:e}",
            "lahf",
            "and ax, 0xC000",
            "shl eax, 16",
            a_res = inlateout(reg) a => result,
            b = in(reg) !b,
            lateout("eax") flags,
            options(pure, nomem, nostack),
        );
        regs.cpsr = Cpsr::from_raw_unchecked((regs.cpsr.raw() & !0xC000_0000) | flags);
        result
    }
}
