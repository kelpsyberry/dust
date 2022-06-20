use super::super::{add_bus_cycles, apply_reg_interlock_1, prefetch_arm, reload_pipeline};
use crate::{
    cpu::interpreter::{common::StateSource, Interpreter},
    emu::Emu,
};

// NOTE: When linking, there's no need to clear previous interlocks for r14, as they will never
// last more than the amount of bus cycles taken by the branch.

pub fn b<const LINK: bool>(emu: &mut Emu<Interpreter>, instr: u32) {
    let r15 = reg!(emu.arm9, 15);
    if LINK {
        reg!(emu.arm9, 14) = r15.wrapping_sub(4);
    }
    let branch_addr = r15.wrapping_add(((instr as i32) << 8 >> 6) as u32);
    add_bus_cycles(emu, 2);
    prefetch_arm::<true, false>(emu);
    reg!(emu.arm9, 15) = branch_addr;
    reload_pipeline::<{ StateSource::Arm }>(emu);
}

pub fn bx<const LINK: bool>(emu: &mut Emu<Interpreter>, instr: u32) {
    let addr_reg = (instr & 0xF) as u8;
    let branch_addr = reg!(emu.arm9, addr_reg);
    apply_reg_interlock_1::<false>(emu, addr_reg);
    add_bus_cycles(emu, 2);
    if LINK {
        reg!(emu.arm9, 14) = reg!(emu.arm9, 15).wrapping_sub(4);
    }
    prefetch_arm::<true, false>(emu);
    reg!(emu.arm9, 15) = branch_addr;
    reload_pipeline::<{ StateSource::R15Bit0 }>(emu);
}

pub fn blx_imm(emu: &mut Emu<Interpreter>, instr: u32) {
    let r15 = reg!(emu.arm9, 15);
    reg!(emu.arm9, 14) = r15.wrapping_sub(4);
    let branch_addr = r15.wrapping_add(((instr as i32) << 8 >> 6) as u32 | (instr >> 23 & 2));
    add_bus_cycles(emu, 2);
    prefetch_arm::<true, false>(emu);
    emu.arm9.engine_data.regs.cpsr.set_thumb_state(true);
    #[cfg(feature = "accurate-pipeline")]
    {
        emu.arm9.engine_data.r15_increment = 2;
    }
    reg!(emu.arm9, 15) = branch_addr;
    reload_pipeline::<{ StateSource::Thumb }>(emu);
}
