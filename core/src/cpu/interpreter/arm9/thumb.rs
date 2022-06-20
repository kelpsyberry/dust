mod branch;
use branch::*;
mod data;
use data::*;
mod mem;
use mem::*;
mod other;
use other::*;

use super::super::{
    common::{DpOpImm8Ty, DpOpRegTy, ShiftImmTy},
    Interpreter,
};
use crate::emu::Emu;

static INSTR_TABLE: [fn(&mut Emu<Interpreter>, u16); 0x400] =
    include!(concat!(env!("OUT_DIR"), "/interp_arm9_thumb.rs"));

#[inline]
pub fn handle_instr(emu: &mut Emu<Interpreter>, instr: u16) {
    INSTR_TABLE[(instr >> 6) as usize](emu, instr);
}
