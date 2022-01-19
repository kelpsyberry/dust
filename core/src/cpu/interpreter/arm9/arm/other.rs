use super::super::{
    add_bus_cycles, add_cycles, apply_reg_interlock_1, handle_prefetch_abort, handle_swi,
    handle_undefined, prefetch_arm, set_cpsr_update_control, write_reg_clear_interlock_ab,
    write_reg_interlock,
};
use crate::{
    cpu::{arm9::Arm9, interpreter::Engine, psr::Cpsr},
    emu::Emu,
};
use core::intrinsics::{likely, unlikely};

pub fn mrs<const SPSR: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let result = if SPSR {
        spsr!(emu.arm9)
    } else {
        emu.arm9.engine_data.regs.cpsr.raw()
    };
    let dst_reg = (instr >> 12 & 0xF) as u8;
    if likely(!cfg!(feature = "interp-r15-write-checks") || dst_reg != 15) {
        write_reg_clear_interlock_ab(emu, dst_reg, result);
    }
    add_bus_cycles(emu, 2);
    prefetch_arm::<true, true>(emu);
    add_cycles(emu, 1);
}

pub fn msr<const IMM: bool, const SPSR: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let value = if IMM {
        (instr & 0xFF).rotate_right(instr >> 7 & 0x1E)
    } else {
        let src_reg = (instr & 0xF) as u8;
        apply_reg_interlock_1::<false>(emu, src_reg);
        reg!(emu.arm9, src_reg)
    };
    add_bus_cycles(emu, 1);
    prefetch_arm::<true, true>(emu);
    let mut mask = 0;
    if instr & 1 << 19 != 0 {
        mask |= 0xF800_0000;
    }
    if emu.arm9.engine_data.regs.is_in_priv_mode() && instr & 1 << 16 != 0 {
        add_bus_cycles(emu, 1);
        add_cycles(emu, 2);
        mask |= 0x0000_00FF;
    }
    if SPSR {
        update_spsr!(emu.arm9, true, mask, value);
    } else {
        if mask & value & 0x20 != 0 {
            unimplemented!("MSR CPSR T bit change");
        }
        set_cpsr_update_control(
            emu,
            Cpsr::from_raw::<true>((emu.arm9.engine_data.regs.cpsr.raw() & !mask) | (value & mask)),
        );
    }
}

pub fn bkpt(emu: &mut Emu<Engine>, _instr: u32) {
    handle_prefetch_abort::<false>(emu);
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
    handle_undefined::<false>(emu);
}

// TODO: Confirm timing and interlocks, both in the undefined and CP15 cases (the ARM9E-S manual
//       says MRC/MCR are like LDR/STR)
// TODO: Confirm that MCRR/MRRC are undefined for CP15 (the ARM946E-S manual only mentions CDP, LDC
//       and STC), and check timings of undefined CDP/LDC/STC/MCRR/MRRC

pub fn mcr(emu: &mut Emu<Engine>, instr: u32) {
    if likely(emu.arm9.engine_data.regs.is_in_priv_mode() && instr >> 8 & 0xF == 15) {
        let src_reg = (instr >> 12 & 0xF) as u8;
        apply_reg_interlock_1::<false>(emu, src_reg);
        add_bus_cycles(emu, 1);
        prefetch_arm::<true, true>(emu);
        Arm9::write_cp15_reg(
            emu,
            (instr >> 21 & 7) as u8,
            (instr >> 16 & 0xF) as u8,
            (instr & 0xF) as u8,
            (instr >> 5 & 7) as u8,
            reg!(emu.arm9, src_reg),
        );
    } else {
        handle_undefined::<false>(emu);
    }
}

pub fn mrc(emu: &mut Emu<Engine>, instr: u32) {
    if likely(emu.arm9.engine_data.regs.is_in_priv_mode() && instr >> 8 & 0xF == 15) {
        prefetch_arm::<true, true>(emu);
        let result = Arm9::read_cp15_reg(
            emu,
            (instr >> 21 & 7) as u8,
            (instr >> 16 & 0xF) as u8,
            (instr & 0xF) as u8,
            (instr >> 5 & 7) as u8,
        );
        add_bus_cycles(emu, 1);
        let dst_reg = (instr >> 12 & 0xF) as u8;
        if unlikely(dst_reg == 15) {
            emu.arm9.engine_data.regs.cpsr.copy_nzcv(result);
        } else {
            write_reg_interlock(emu, dst_reg, result, 1, 1);
        }
    } else {
        handle_undefined::<false>(emu);
    }
}

pub fn mcrr(emu: &mut Emu<Engine>, _instr: u32) {
    handle_undefined::<false>(emu);
}

pub fn mrrc(emu: &mut Emu<Engine>, _instr: u32) {
    handle_undefined::<false>(emu);
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
