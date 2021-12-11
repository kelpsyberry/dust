mod branch;
use branch::*;
mod data;
use data::*;
mod mem;
use mem::*;
mod other;
use other::*;

use super::{
    common::{DpOpImm8Ty, DpOpRegTy, DpOpSpecialTy, ShiftImmTy},
    Context,
};

static INSTR_TABLE_ARM7: [fn(&mut Context, u16); 0x400] =
    include!(concat!(env!("OUT_DIR"), "/disasm_arm7_thumb.rs"));

static INSTR_TABLE_ARM9: [fn(&mut Context, u16); 0x400] =
    include!(concat!(env!("OUT_DIR"), "/disasm_arm9_thumb.rs"));

#[inline]
pub(super) fn handle_instr<const ARM9: bool>(ctx: &mut Context, instr: u16) {
    let index = (instr >> 6) as usize;
    (if ARM9 {
        INSTR_TABLE_ARM9[index]
    } else {
        INSTR_TABLE_ARM7[index]
    })(ctx, instr);
}
