use super::super::{super::Engine, handle_swi, handle_undefined, set_cpsr_update_control};
#[cfg(not(feature = "interp-pipeline"))]
use crate::cpu::{arm7::bus, bus::CpuAccess};
use crate::{cpu::psr::Cpsr, emu::Emu};
use core::intrinsics::unlikely;

pub fn nop(emu: &mut Emu<Engine>, _instr: u32) {
    inc_r15!(emu.arm7, 4);
}

pub fn mrs<const SPSR: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let result = if SPSR {
        spsr!(emu.arm7)
    } else {
        emu.arm7.engine_data.regs.cpsr.raw()
    };
    let dst_reg = instr >> 12 & 0xF;
    #[cfg(feature = "interp-r15-write-checks")]
    if unlikely(dst_reg == 15) {
        unimplemented!("MRS r15 write");
    }
    reg!(emu.arm7, dst_reg) = result;
    inc_r15!(emu.arm7, 4);
}

pub fn msr<const IMM: bool, const SPSR: bool>(emu: &mut Emu<Engine>, instr: u32) {
    // TODO: Can reserved bits be modified in the SPSRs?
    let value = if IMM {
        (instr & 0xFF).rotate_right(instr >> 7 & 0x1E)
    } else {
        reg!(emu.arm7, instr & 0xF)
    };
    let mut mask = 0;
    if instr & 1 << 19 != 0 {
        mask |= 0xF000_0000;
    }
    if emu.arm7.engine_data.regs.is_in_priv_mode() && instr & 1 << 16 != 0 {
        mask |= 0x0000_00FF;
    }
    if SPSR {
        update_spsr!(emu.arm7, false, mask, value);
    } else {
        if mask & value & 0x20 != 0 {
            unimplemented!("MSR CPSR T bit change");
        }
        set_cpsr_update_control(
            emu,
            Cpsr::from_raw::<false>(
                (emu.arm7.engine_data.regs.cpsr.raw() & !mask) | (value & mask),
            ),
        );
    }
    inc_r15!(emu.arm7, 4);
}

pub fn swi(emu: &mut Emu<Engine>, _instr: u32) {
    handle_swi::<false>(
        emu,
        #[cfg(feature = "debugger-hooks")]
        {
            (_instr >> 16) as u8
        },
    );
}

pub fn undefined(emu: &mut Emu<Engine>, _instr: u32) {
    // TODO: Check timing, the ARM7TDMI manual is unclear
    handle_undefined::<false>(emu);
}

// TODO: Check what happens with cdp/ldc/stc P14,...

pub fn mcr(emu: &mut Emu<Engine>, instr: u32) {
    if instr >> 8 & 0xF == 14 {
        inc_r15!(emu.arm7, 4);
    } else {
        // TODO: Check timing, the ARM7TDMI manual is unclear
        handle_undefined::<false>(emu);
    }
}

pub fn mrc(emu: &mut Emu<Engine>, instr: u32) {
    if instr >> 8 & 0xF == 14 {
        #[cfg(feature = "interp-pipeline")]
        let result = emu.arm7.engine_data.pipeline[1] as u32;
        #[cfg(not(feature = "interp-pipeline"))]
        let result = bus::read_32::<CpuAccess, _>(emu, reg!(emu.arm7, 15));
        inc_r15!(emu.arm7, 4);
        let dst_reg = instr >> 12 & 0xF;
        if unlikely(dst_reg == 15) {
            emu.arm7.engine_data.regs.cpsr.copy_nzcv(result);
        } else {
            reg!(emu.arm7, dst_reg) = result;
        }
    } else {
        // TODO: Check timing, the ARM7TDMI manual is unclear
        handle_undefined::<false>(emu);
    }
}

pub fn cdp(emu: &mut Emu<Engine>, _instr: u32) {
    handle_undefined::<false>(emu);
}

pub fn ldc(emu: &mut Emu<Engine>, _instr: u32) {
    handle_undefined::<false>(emu);
}

pub fn stc(emu: &mut Emu<Engine>, _instr: u32) {
    handle_undefined::<false>(emu);
}
