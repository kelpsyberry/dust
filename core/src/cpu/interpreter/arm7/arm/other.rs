use super::super::{
    super::Interpreter, enter_hle_irq, enter_hle_swi, handle_swi, handle_undefined,
    return_from_hle_irq, set_cpsr_update_control,
};
use crate::{
    cpu::{arm7::bus, bus::CpuAccess, hle_bios, psr::Psr},
    emu::Emu,
};
use core::intrinsics::unlikely;

pub fn nop(emu: &mut Emu<Interpreter>, _instr: u32) {
    inc_r15!(emu.arm7, 4);
}

pub fn mrs<const SPSR: bool>(emu: &mut Emu<Interpreter>, instr: u32) {
    let result = if SPSR {
        spsr!(emu.arm7).raw()
    } else {
        emu.arm7.engine_data.regs.cpsr.raw()
    };
    let dst_reg = instr >> 12 & 0xF;
    #[cfg(feature = "interp-r15-write-checks")]
    if unlikely(dst_reg == 15) {
        unimplemented!("mrs r15 write");
    }
    reg!(emu.arm7, dst_reg) = result;
    inc_r15!(emu.arm7, 4);
}

pub fn msr<const IMM: bool, const SPSR: bool>(emu: &mut Emu<Interpreter>, instr: u32) {
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
        mask |= 0x0000_00EF;
    }
    if SPSR {
        update_spsr!(emu.arm7, mask, value);
    } else {
        if mask & value & 0x20 != 0 {
            unimplemented!("msr CPSR T bit change");
        }
        set_cpsr_update_control(
            emu,
            Psr::from_raw((emu.arm7.engine_data.regs.cpsr.raw() & !mask) | (value & mask)),
        );
    }
    inc_r15!(emu.arm7, 4);
}

pub fn swi(emu: &mut Emu<Interpreter>, instr: u32) {
    handle_swi::<false>(emu, (instr >> 16) as u8);
}

pub fn undefined<const MAYBE_HLE_BIOS_CALL: bool>(emu: &mut Emu<Interpreter>, instr: u32) {
    if MAYBE_HLE_BIOS_CALL
        && instr & hle_bios::arm7::BIOS_CALL_INSTR_MASK == hle_bios::arm7::BIOS_CALL_INSTR
        && emu.arm7.hle_bios_enabled()
    {
        let function = instr as u8 & 0xF;
        match function {
            0 => {
                hle_bios::arm7::resume_intr_wait(emu);
                return;
            }
            1 => {
                enter_hle_irq::<false>(emu, reg!(emu.arm7, 14));
                return;
            }
            2 => {
                return_from_hle_irq(emu);
                return;
            }
            3 => {
                hle_bios::arm7::handle_undefined_instr(emu);
                return;
            }
            5 => {
                let return_addr = reg!(emu.arm7, 14);
                let number = bus::read_8::<CpuAccess, _>(emu, return_addr.wrapping_sub(2));
                enter_hle_swi::<false>(emu, number, return_addr);
                return;
            }
            _ => {}
        }
    }
    // TODO: Check timing, the ARM7TDMI manual is unclear
    handle_undefined::<false>(emu);
}

// TODO: Check what happens with cdp/ldc/stc P14,...

pub fn mcr(emu: &mut Emu<Interpreter>, instr: u32) {
    if instr >> 8 & 0xF == 14 {
        inc_r15!(emu.arm7, 4);
    } else {
        // TODO: Check timing, the ARM7TDMI manual is unclear
        handle_undefined::<false>(emu);
    }
}

pub fn mrc(emu: &mut Emu<Interpreter>, instr: u32) {
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

pub fn cdp(emu: &mut Emu<Interpreter>, _instr: u32) {
    handle_undefined::<false>(emu);
}

pub fn ldc(emu: &mut Emu<Interpreter>, _instr: u32) {
    handle_undefined::<false>(emu);
}

pub fn stc(emu: &mut Emu<Interpreter>, _instr: u32) {
    handle_undefined::<false>(emu);
}
