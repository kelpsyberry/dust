use super::super::{common::COND_STRINGS, Context};

pub(super) fn b(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let branch_addr = ctx.pc.wrapping_add(((instr as i32) << 21 >> 20) as u32);
    ctx.next_instr.opcode = format!("b #{branch_addr:#010X}");
}

pub(super) fn b_cond<const COND: u8>(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let branch_addr = ctx.pc.wrapping_add((instr as i8 as i32 as u32) << 1);
    ctx.next_instr.opcode = format!("b{} #{branch_addr:#010X}", COND_STRINGS[COND as usize]);
}

pub(super) fn bx<const LINK: bool>(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let addr_reg = instr >> 3 & 0xF;
    ctx.next_instr.opcode = format!("b{}x r{addr_reg}", if LINK { "l" } else { "" });
}

pub(super) fn bl_prefix(ctx: &mut Context, instr: u16) {
    let branch_addr_base = ctx.pc.wrapping_add(((instr as i32) << 21 >> 9) as u32);
    ctx.branch_addr_base = Some(branch_addr_base);
    ctx.next_instr.opcode = "<bl/blx prefix>".to_string();
    ctx.next_instr.comment = format!("r14 = {branch_addr_base:#010X}");
}

pub(super) fn bl_suffix<const EXCHANGE: bool>(ctx: &mut Context, instr: u16) {
    let offset = ((instr & 0x7FF) << 1) as u32;
    let exchange = if EXCHANGE { "x" } else { "" };
    ctx.next_instr.opcode = if let Some(branch_addr_base) = ctx.branch_addr_base {
        format!(
            "b{}x #{:#010X}",
            exchange,
            branch_addr_base.wrapping_add(offset)
        )
    } else {
        format!("<b{exchange}x suffix>")
    };
    ctx.branch_addr_base = None;
    ctx.next_instr.comment = format!("r14 + {offset:#05X}");
}
