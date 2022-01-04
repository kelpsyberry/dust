mod branch;
use branch::*;
mod data;
use data::*;
mod mem;
use mem::*;
mod other;
use other::*;

use super::{add_bus_cycles, prefetch_arm};
use crate::{
    cpu::interpreter::{
        common::{DpOpTy, DpOperand, MiscAddressing, ShiftTy, WbAddressing, WbOffTy},
        Engine,
    },
    emu::Emu,
};

static INSTR_TABLE_COND: [fn(&mut Emu<Engine>, u32); 0x1000] =
    include!(concat!(env!("OUT_DIR"), "/interp_arm9_arm_cond.rs"));

static INSTR_TABLE_UNCOND: [fn(&mut Emu<Engine>, u32); 0x1000] =
    include!(concat!(env!("OUT_DIR"), "/interp_arm9_arm_uncond.rs"));

#[inline]
pub fn handle_instr(emu: &mut Emu<Engine>, instr: u32) {
    let cond = (instr >> 28) as u8;
    let index = ((instr >> 16 & 0xFF0) | (instr >> 4 & 0xF)) as usize;
    if cond == 0xF {
        INSTR_TABLE_UNCOND[index](emu, instr);
    } else if emu.arm9.engine_data.regs.cpsr.satisfies_condition(cond) {
        INSTR_TABLE_COND[index](emu, instr);
    } else {
        add_bus_cycles(emu, 1);
        prefetch_arm::<true, true>(emu);
    }
}
