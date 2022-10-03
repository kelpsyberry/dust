pub use super::super::shifts_common::*;

use super::super::super::Regs;
use crate::cpu::psr::Psr;

pub fn lsl_imm_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else {
        let result: u32;
        let mut cpsr = regs.cpsr.raw();
        unsafe {
            core::arch::asm!(
                "lsl {result:x}, {value:x}, {shift:x}",
                "lsr {scratch:x}, {result:x}, #32",
                "bfi {cpsr:w}, {scratch:w}, #29, #1",
                value = in(reg) value,
                shift = in(reg) shift,
                cpsr = inlateout(reg) cpsr,
                result = lateout(reg) result,
                scratch = lateout(reg) _,
                options(pure, nomem, nostack, preserves_flags),
            );
        }
        regs.cpsr = Psr::from_raw(cpsr);
        result
    }
}

pub fn lsl_reg(value: u32, shift: u8) -> u32 {
    let result: u32;
    unsafe {
        core::arch::asm!(
            "cmp {shift:w}, #32",
            "lsl {result:w}, {value:w}, {shift:w}",
            "csel {result:w}, {result:w}, wzr, lo",
            value = in(reg) value,
            shift = in(reg) shift,
            result = lateout(reg) result,
            options(pure, nomem, nostack),
        );
    }
    result
}

pub fn lsl_reg_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else if shift < 33 {
        let result: u32;
        let mut cpsr = regs.cpsr.raw();
        unsafe {
            core::arch::asm!(
                "lsl {result:x}, {value:x}, {shift:x}",
                "lsr {scratch:x}, {result:x}, #32",
                "bfi {cpsr:w}, {scratch:w}, #29, #1",
                value = in(reg) value,
                shift = in(reg) shift,
                cpsr = inlateout(reg) cpsr,
                result = lateout(reg) result,
                scratch = lateout(reg) _,
                options(pure, nomem, nostack, preserves_flags),
            );
        }
        regs.cpsr = Psr::from_raw(cpsr);
        result
    } else {
        regs.cpsr.set_carry(false);
        0
    }
}

pub fn lsr_imm_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    let result: u32;
    let mut cpsr = regs.cpsr.raw();
    unsafe {
        // NOTE: This code relies on `shift.wrapping_sub(1)` wrapping to 255 when `shift` is 0, so
        // that, when used as a shift amount with a 32-bit register, it wraps back to 31 + the
        // later lsr by 1, doing a 32-bit shift and exhibiting the correct behavior without
        // branching.
        core::arch::asm!(
            "lsr {result:w}, {value:w}, {shift:w}",
            "bfi {cpsr:w}, {result:w}, #29, #1",
            "lsr {result:w}, {result:w}, #1",
            value = in(reg) value,
            shift = in(reg) shift.wrapping_sub(1),
            cpsr = inlateout(reg) cpsr,
            result = lateout(reg) result,
            options(pure, nomem, nostack, preserves_flags),
        );
    }
    regs.cpsr = Psr::from_raw(cpsr);
    result
}

pub fn lsr_reg(value: u32, shift: u8) -> u32 {
    let result: u32;
    unsafe {
        core::arch::asm!(
            "cmp {shift:w}, #32",
            "lsr {result:w}, {value:w}, {shift:w}",
            "csel {result:w}, {result:w}, wzr, lo",
            value = in(reg) value,
            shift = in(reg) shift,
            result = lateout(reg) result,
            options(pure, nomem, nostack),
        );
    }
    result
}

pub fn lsr_reg_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else if shift < 33 {
        let result: u32;
        let mut cpsr = regs.cpsr.raw();
        unsafe {
            core::arch::asm!(
                "lsr {result:w}, {value:w}, {shift:w}",
                "bfi {cpsr:w}, {result:w}, #29, #1",
                "lsr {result:w}, {result:w}, #1",
                value = in(reg) value,
                shift = in(reg) shift - 1,
                cpsr = inlateout(reg) cpsr,
                result = lateout(reg) result,
                options(pure, nomem, nostack, preserves_flags),
            );
        }
        regs.cpsr = Psr::from_raw(cpsr);
        result
    } else {
        regs.cpsr.set_carry(false);
        0
    }
}

pub fn asr_imm_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    let result: u32;
    let mut cpsr = regs.cpsr.raw();
    unsafe {
        // NOTE: This code relies on `shift.wrapping_sub(1)` wrapping to 255 when `shift` is 0, so
        // that, when used as a shift amount with a 32-bit register, it wraps back to 31 + the
        // later asr by 1, doing a 32-bit shift and exhibiting the correct behavior without
        // branching.
        core::arch::asm!(
            "asr {result:w}, {value:w}, {shift:w}",
            "bfi {cpsr:w}, {result:w}, #29, #1",
            "asr {result:w}, {result:w}, #1",
            value = in(reg) value,
            shift = in(reg) shift.wrapping_sub(1),
            cpsr = inlateout(reg) cpsr,
            result = lateout(reg) result,
            options(pure, nomem, nostack, preserves_flags),
        );
    }
    regs.cpsr = Psr::from_raw(cpsr);
    result
}

pub fn asr_reg_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else {
        let result: u32;
        let mut cpsr = regs.cpsr.raw();
        unsafe {
            if shift < 32 {
                core::arch::asm!(
                    "asr {result:w}, {value:w}, {shift:w}",
                    "bfi {cpsr:w}, {result:w}, #29, #1",
                    "asr {result:w}, {result:w}, #1",
                    value = in(reg) value,
                    shift = in(reg) shift - 1,
                    cpsr = inlateout(reg) cpsr,
                    result = lateout(reg) result,
                    options(pure, nomem, nostack, preserves_flags),
                );
            } else {
                core::arch::asm!(
                    "asr {result:w}, {value:w}, #31",
                    "bfi {cpsr:w}, {result:w}, #29, #1",
                    value = in(reg) value,
                    cpsr = inlateout(reg) cpsr,
                    result = lateout(reg) result,
                    options(pure, nomem, nostack, preserves_flags),
                );
            }
        }
        regs.cpsr = Psr::from_raw(cpsr);
        result
    }
}

pub fn rrx(regs: &Regs, value: u32) -> u32 {
    let result: u32;
    unsafe {
        core::arch::asm!(
            "lsl {result:w}, {cpsr:w}, #2",
            "bfxil {result:w}, {value:w}, #1, #31",
            value = in(reg) value,
            cpsr = in(reg) regs.cpsr.raw(),
            result = out(reg) result,
            options(pure, nomem, nostack, preserves_flags),
        );
    }
    result
}

fn rrx_s(regs: &mut Regs, value: u32) -> u32 {
    let result: u32;
    let mut cpsr = regs.cpsr.raw();
    unsafe {
        core::arch::asm!(
            "lsl {result:w}, {cpsr:w}, #2",
            "bfi {cpsr:w}, {value:w}, #29, #1",
            "bfxil {result:w}, {value:w}, #1, #31",
            value = in(reg) value,
            cpsr = inout(reg) cpsr,
            result = out(reg) result,
            options(pure, nomem, nostack, preserves_flags),
        );
    }
    regs.cpsr = Psr::from_raw(cpsr);
    result
}

fn ror_s_nonzero(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    let result: u32;
    let mut cpsr = regs.cpsr.raw();
    unsafe {
        core::arch::asm!(
            "ror {result:w}, {value:w}, {shift:w}",
            "bfi {cpsr:w}, {result:w}, #29, #1",
            "ror {result:w}, {result:w}, #1",
            value = in(reg) value,
            shift = in(reg) shift - 1,
            cpsr = inlateout(reg) cpsr,
            result = lateout(reg) result,
            options(pure, nomem, nostack, preserves_flags),
        );
    }
    regs.cpsr = Psr::from_raw(cpsr);
    result
}

pub fn ror_imm_s_no_rrx(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else {
        ror_s_nonzero(regs, value, shift)
    }
}

pub fn ror_imm_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        rrx_s(regs, value)
    } else {
        ror_s_nonzero(regs, value, shift)
    }
}

pub fn ror_reg_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else {
        ror_s_nonzero(regs, value, shift)
    }
}
