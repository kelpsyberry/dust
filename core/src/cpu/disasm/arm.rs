mod branch;
use branch::*;
mod data;
use data::*;
mod mem;
use mem::*;
mod other;
use other::*;

use super::{
    common::{DpOpTy, DpOperand, MiscAddressing, ShiftTy, WbAddressing, WbOffTy, COND_STRINGS},
    Context,
};

static INSTR_TABLE_ARM9_COND: [fn(&mut Context, u32, &'static str); 0x1000] =
    include!(concat!(env!("OUT_DIR"), "/disasm_arm9_arm_cond.rs"));

static INSTR_TABLE_ARM7_COND: [fn(&mut Context, u32, &'static str); 0x1000] =
    include!(concat!(env!("OUT_DIR"), "/disasm_arm7_arm.rs"));

static INSTR_TABLE_UNCOND: [fn(&mut Context, u32); 0x1000] =
    include!(concat!(env!("OUT_DIR"), "/disasm_arm9_arm_uncond.rs"));

#[inline]
pub(super) fn handle_instr<const ARM9: bool>(ctx: &mut Context, instr: u32) {
    let cond = (instr >> 28) as usize;
    let index = ((instr >> 16 & 0xFF0) | (instr >> 4 & 0xF)) as usize;
    if !ARM9 {
        INSTR_TABLE_ARM7_COND[index](ctx, instr, COND_STRINGS[cond as usize]);
    } else if cond != 0xF {
        INSTR_TABLE_ARM9_COND[index](ctx, instr, COND_STRINGS[cond as usize]);
    } else {
        INSTR_TABLE_UNCOND[index](ctx, instr);
    }
}
