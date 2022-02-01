use super::super::{
    add_bus_cycles, apply_reg_interlock_1, handle_undefined, prefetch_thumb, reload_pipeline,
};
use crate::{
    cpu::interpreter::{common::StateSource, Engine},
    emu::Emu,
};
use core::intrinsics::unlikely;

pub fn b(emu: &mut Emu<Engine>, instr: u16) {
    let branch_addr = reg!(emu.arm9, 15).wrapping_add(((instr as i32) << 21 >> 20) as u32);
    add_bus_cycles(emu, 2);
    prefetch_thumb::<true, false>(emu);
    reg!(emu.arm9, 15) = branch_addr;
    reload_pipeline::<{ StateSource::Thumb }>(emu);
}

pub fn b_cond<const COND: u8>(emu: &mut Emu<Engine>, instr: u16) {
    prefetch_thumb::<true, false>(emu);
    if !emu.arm9.engine_data.regs.cpsr.satisfies_condition(COND) {
        inc_r15!(emu.arm9, 2);
        return add_bus_cycles(emu, 1);
    }
    add_bus_cycles(emu, 2);
    let branch_addr = reg!(emu.arm9, 15).wrapping_add((instr as i8 as i32 as u32) << 1);
    reg!(emu.arm9, 15) = branch_addr;
    reload_pipeline::<{ StateSource::Thumb }>(emu);
}

pub fn bx<const LINK: bool>(emu: &mut Emu<Engine>, instr: u16) {
    let addr_reg = (instr >> 3 & 0xF) as u8;
    apply_reg_interlock_1::<false>(emu, addr_reg);
    add_bus_cycles(emu, 2);
    prefetch_thumb::<true, false>(emu);
    let branch_addr = reg!(emu.arm9, addr_reg);
    if LINK {
        reg!(emu.arm9, 14) = reg!(emu.arm9, 15).wrapping_sub(1);
    }
    reg!(emu.arm9, 15) = branch_addr;
    reload_pipeline::<{ StateSource::R15Bit0 }>(emu);
}

pub fn bl_prefix(emu: &mut Emu<Engine>, instr: u16) {
    reg!(emu.arm9, 14) = reg!(emu.arm9, 15).wrapping_add(((instr as i32) << 21 >> 9) as u32);
    add_bus_cycles(emu, 1);
    prefetch_thumb::<true, true>(emu);
}

pub fn bl_suffix<const EXCHANGE: bool>(emu: &mut Emu<Engine>, instr: u16) {
    if unlikely(EXCHANGE && instr & 1 != 0) {
        return handle_undefined::<true>(emu);
    }
    add_bus_cycles(emu, 2);
    prefetch_thumb::<true, false>(emu);
    let branch_addr = reg!(emu.arm9, 14).wrapping_add(((instr & 0x7FF) << 1) as u32);
    reg!(emu.arm9, 14) = reg!(emu.arm9, 15).wrapping_sub(1);
    reg!(emu.arm9, 15) = branch_addr;
    if EXCHANGE {
        emu.arm9.engine_data.regs.cpsr.set_thumb_state(false);
        #[cfg(feature = "accurate-pipeline")]
        {
            emu.arm9.engine_data.r15_increment = 4;
        }
        reload_pipeline::<{ StateSource::Arm }>(emu);
    } else {
        reload_pipeline::<{ StateSource::Thumb }>(emu);
    }
}
