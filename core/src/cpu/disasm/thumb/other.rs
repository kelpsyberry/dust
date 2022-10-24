use super::super::Context;

pub(super) fn bkpt(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let comment = instr & 0xFF;
    ctx.next_instr.opcode = format!("bkpt #{comment:#04X}");
}

pub(super) fn swi(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let comment = instr & 0xFF;
    ctx.next_instr.opcode = format!("swi #{comment:#04X}");
}

pub(super) fn undefined(ctx: &mut Context, _instr: u16) {
    ctx.branch_addr_base = None;
    ctx.next_instr.opcode = "udf".to_string();
}
