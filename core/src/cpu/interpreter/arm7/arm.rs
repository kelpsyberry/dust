mod branch;
use branch::*;
mod data;
use data::*;
mod mem;
use mem::*;
mod other;
use other::*;

use super::super::{
    common::{DpOpTy, DpOperand, MiscAddressing, ShiftTy, WbAddressing, WbOffTy},
    Engine,
};
use crate::emu::Emu;

static INSTR_TABLE: [fn(&mut Emu<Engine>, u32); 0x1000] =
    include!(concat!(env!("OUT_DIR"), "/interp_arm7_arm.rs"));

#[inline]
pub fn handle_instr(emu: &mut Emu<Engine>, instr: u32) {
    if emu
        .arm7
        .engine_data
        .regs
        .cpsr
        .satisfies_condition((instr >> 28) as u8)
    {
        INSTR_TABLE[((instr >> 16 & 0xFF0) | (instr >> 4 & 0xF)) as usize](emu, instr);
    } else {
        inc_r15!(emu.arm7, 4);
    }
}
