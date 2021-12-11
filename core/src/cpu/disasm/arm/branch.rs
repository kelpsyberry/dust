use super::super::Context;

pub(super) fn b<const LINK: bool>(ctx: &mut Context, instr: u32, cond: &'static str) {
    let branch_addr = ctx.pc.wrapping_add(((instr as i32) << 8 >> 6) as u32);
    ctx.next_instr.opcode = format!(
        "b{}{} #{:#010X}",
        if LINK { "l" } else { "" },
        cond,
        branch_addr
    );
}

pub(super) fn bx<const LINK: bool>(ctx: &mut Context, instr: u32, cond: &'static str) {
    let addr_reg = instr & 0xF;
    ctx.next_instr.opcode = format!("b{}x{} r{}", if LINK { "l" } else { "" }, cond, addr_reg);
}

pub(super) fn blx_imm(ctx: &mut Context, instr: u32) {
    let branch_addr = ctx
        .pc
        .wrapping_add(((instr as i32) << 8 >> 6) as u32 | (instr >> 23 & 2));
    ctx.next_instr.opcode = format!("blx #{:#010X}", branch_addr);
}
