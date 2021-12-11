use super::super::{
    super::{common::StateSource, Engine},
    reload_pipeline,
};
use crate::emu::Emu;

pub fn b<const LINK: bool>(emu: &mut Emu<Engine>, instr: u32) {
    let r15 = reg!(emu.arm7, 15);
    if LINK {
        reg!(emu.arm7, 14) = r15.wrapping_sub(4);
    }
    let branch_addr = r15.wrapping_add(((instr as i32) << 8 >> 6) as u32);
    reg!(emu.arm7, 15) = branch_addr;
    reload_pipeline::<{ StateSource::Arm }>(emu);
}

pub fn bx(emu: &mut Emu<Engine>, instr: u32) {
    let branch_addr = reg!(emu.arm7, instr & 0xF);
    reg!(emu.arm7, 15) = branch_addr;
    reload_pipeline::<{ StateSource::R15Bit0 }>(emu);
}
