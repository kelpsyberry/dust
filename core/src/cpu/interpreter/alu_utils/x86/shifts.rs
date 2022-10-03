pub use super::super::shifts_common::*;

use super::super::super::Regs;
use crate::cpu::psr::Psr;

pub fn lsl_imm_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    let mut cpsr = regs.cpsr.raw();
    let result: u32;
    unsafe {
        core::arch::asm!(
            "btr {cpsr:e}, 29",
            "shl {value_res:e}, cl",
            "setc {carry_flag:l}",
            "shl {carry_flag:e}, 29",
            "or {cpsr:e}, {carry_flag:e}",
            value_res = inlateout(reg) value => result,
            cpsr = inout(reg) cpsr,
            carry_flag = lateout(reg) _,
            in("cl") shift,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw(cpsr);
    result
}

pub fn lsl_reg(value: u32, shift: u8) -> u32 {
    let result: u32;
    unsafe {
        core::arch::asm!(
            "shl {value_res:e}, cl",
            "xor {scratch:e}, {scratch:e}",
            "cmp cl, 32",
            "cmovae {value_res:e}, {scratch:e}",
            value_res = inout(reg) value => result,
            scratch = out(reg) _,
            in("cl") shift,
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
        let carry_flag: u8;
        unsafe {
            core::arch::asm!(
                "shl {value_res:e}, cl",
                "shl {value_res:e}, 1",
                "setc {carry_flag}",
                value_res = inlateout(reg) value => result,
                carry_flag = lateout(reg_byte) carry_flag,
                in("cl") shift - 1,
                options(pure, nomem, nostack),
            );
        }
        regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0x2000_0000) | (carry_flag as u32) << 29);
        result
    } else {
        regs.cpsr.set_carry(false);
        0
    }
}

fn lsr_1_32_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    let result: u32;
    let carry_flag: u8;
    unsafe {
        // NOTE: This code relies on `shift.wrapping_sub(1)` wrapping to 255 when `shift` is 0, so
        // that, when used as a shift amount with a 32-bit register, it wraps back to 31 + the
        // later shr by 1, doing a 32-bit shift and exhibiting the correct behavior without
        // branching.
        core::arch::asm!(
            "shr {value_res:e}, cl",
            "shr {value_res:e}, 1",
            "setc {carry_flag}",
            value_res = inlateout(reg) value => result,
            carry_flag = lateout(reg_byte) carry_flag,
            in("cl") shift.wrapping_sub(1),
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0x2000_0000) | (carry_flag as u32) << 29);
    result
}

pub fn lsr_imm_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    lsr_1_32_s(regs, value, shift)
}

pub fn lsr_reg(value: u32, shift: u8) -> u32 {
    let result: u32;
    unsafe {
        core::arch::asm!(
            "shr {value_res:e}, cl",
            "xor {scratch:e}, {scratch:e}",
            "cmp cl, 32",
            "cmovae {value_res:e}, {scratch:e}",
            value_res = inout(reg) value => result,
            scratch = out(reg) _,
            in("cl") shift,
            options(pure, nomem, nostack),
        );
    }
    result
}

pub fn lsr_reg_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else if shift < 33 {
        #[cfg(target_arch = "x86")]
        {
            lsr_1_32_s(regs, value, shift)
        }
        #[cfg(target_arch = "x86_64")]
        let result: u32;
        let carry_flag: u8;
        unsafe {
            core::arch::asm!(
                "shr {value_res:r}, cl",
                "setc {carry_flag}",
                value_res = inlateout(reg) value => result,
                carry_flag = lateout(reg_byte) carry_flag,
                in("cl") shift,
                options(pure, nomem, nostack),
            );
        }
        regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0x2000_0000) | (carry_flag as u32) << 29);
        result
    } else {
        regs.cpsr.set_carry(false);
        0
    }
}

pub fn asr_imm_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    let result: u32;
    let carry_flag: u8;
    unsafe {
        // NOTE: This code relies on `shift.wrapping_sub(1)` wrapping to 255 when `shift` is 0, so
        // that, when used as a shift amount with a 32-bit register, it wraps back to 31 + the
        // later sar by 1, doing a 32-bit shift and exhibiting the correct behavior without
        // branching.
        core::arch::asm!(
            "sar {value_res:e}, cl",
            "sar {value_res:e}, 1",
            "setc {carry_flag}",
            value_res = inlateout(reg) value => result,
            carry_flag = lateout(reg_byte) carry_flag,
            in("cl") shift.wrapping_sub(1),
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0x2000_0000) | (carry_flag as u32) << 29);
    result
}

pub fn asr_reg_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else {
        let result: u32;
        let carry_flag: u8;
        unsafe {
            if shift < 32 {
                core::arch::asm!(
                    "sar {value_res:e}, cl",
                    "setc {carry_flag}",
                    value_res = inlateout(reg) value => result,
                    carry_flag = lateout(reg_byte) carry_flag,
                    in("cl") shift,
                    options(pure, nomem, nostack),
                );
            } else {
                core::arch::asm!(
                    "sar {value_res:e}, 31",
                    "mov {carry_flag}, {value_res:l}",
                    "and {carry_flag}, 1",
                    value_res = inlateout(reg) value => result,
                    carry_flag = lateout(reg_byte) carry_flag,
                    options(pure, nomem, nostack),
                );
            }
        }
        regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0x2000_0000) | (carry_flag as u32) << 29);
        result
    }
}

pub fn rrx(regs: &Regs, value: u32) -> u32 {
    let result: u32;
    unsafe {
        core::arch::asm!(
            "bt {cpsr:e}, 29",
            "rcr {value_res:e}, 1",
            value_res = inlateout(reg) value => result,
            cpsr = in(reg) regs.cpsr.raw(),
            options(pure, nomem, nostack),
        );
    }
    result
}

fn rrx_s(regs: &mut Regs, value: u32) -> u32 {
    let mut cpsr = regs.cpsr.raw();
    let result: u32;
    let carry_flag: u8;
    unsafe {
        core::arch::asm!(
            "btr {cpsr:e}, 29",
            "rcr {value_res:e}, 1",
            "setc {carry_flag}",
            value_res = inlateout(reg) value => result,
            cpsr = inout(reg) cpsr,
            carry_flag = lateout(reg_byte) carry_flag,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw(cpsr | (carry_flag as u32) << 29);
    result
}

fn ror_imm_s_nonzero(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    let result: u32;
    let carry_flag: u8;
    unsafe {
        core::arch::asm!(
            "ror {value_res:e}, cl",
            "setc {carry_flag}",
            value_res = inlateout(reg) value => result,
            carry_flag = lateout(reg_byte) carry_flag,
            in("cl") shift,
            options(pure, nomem, nostack),
        );
    }
    regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0x2000_0000) | (carry_flag as u32) << 29);
    result
}

pub fn ror_imm_s_no_rrx(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else {
        ror_imm_s_nonzero(regs, value, shift)
    }
}

pub fn ror_imm_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        rrx_s(regs, value)
    } else {
        ror_imm_s_nonzero(regs, value, shift)
    }
}

pub fn ror_reg_s(regs: &mut Regs, value: u32, shift: u8) -> u32 {
    if shift == 0 {
        value
    } else {
        let result: u32;
        let carry_flag: u8;
        unsafe {
            core::arch::asm!(
                "bt {value_res:e}, 31",
                "ror {value_res:e}, cl",
                "setc {carry_flag}",
                value_res = inlateout(reg) value => result,
                carry_flag = lateout(reg_byte) carry_flag,
                in("cl") shift,
                options(pure, nomem, nostack),
            );
        }
        regs.cpsr = Psr::from_raw((regs.cpsr.raw() & !0x2000_0000) | (carry_flag as u32) << 29);
        result
    }
}
