use super::super::reload_pipeline;
use crate::{
    cpu::interpreter::{common::StateSource, Interpreter},
    emu::Emu,
};

pub fn b<const LINK: bool>(emu: &mut Emu<Interpreter>, instr: u32) {
    let r15 = reg!(emu.arm7, 15);
    if LINK {
        reg!(emu.arm7, 14) = r15.wrapping_sub(4);
    }
    reg!(emu.arm7, 15) = r15.wrapping_add(((instr as i32) << 8 >> 6) as u32);
    reload_pipeline::<{ StateSource::Arm }>(emu);
}

pub fn bx(emu: &mut Emu<Interpreter>, instr: u32) {
    reg!(emu.arm7, 15) = reg!(emu.arm7, instr & 0xF);
    reload_pipeline::<{ StateSource::R15Bit0 }>(emu);
}
