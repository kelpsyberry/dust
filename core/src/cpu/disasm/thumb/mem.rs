use super::super::Context;
use core::fmt::Write;

pub(super) fn ldr_str<const OPCODE: &'static str, const IMM_OFFSET_SHIFT: u8, const IMM: bool>(
    ctx: &mut Context,
    instr: u16,
) {
    ctx.branch_addr_base = None;
    let src_dst_reg = instr & 7;
    let base_reg = instr >> 3 & 7;
    ctx.next_instr.opcode = if IMM {
        let offset = (instr >> 6 & 0x1F) << IMM_OFFSET_SHIFT;
        format!(
            "{} r{}, [r{}, #{:#04X}]",
            OPCODE, src_dst_reg, base_reg, offset
        )
    } else {
        let off_reg = instr >> 6 & 7;
        format!("{} r{}, [r{}, r{}]", OPCODE, src_dst_reg, base_reg, off_reg)
    };
}

pub(super) fn ldr_pc_rel(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let dst_reg = instr >> 8 & 7;
    let offset = (instr & 0xFF) << 2;
    let addr = (ctx.pc & !3).wrapping_add(offset as u32);
    ctx.next_instr.opcode = format!("ldr r{}, [r15 + #{:#05X}]", dst_reg, offset);
    ctx.next_instr.comment = format!("{:#010X}", addr);
}

pub(super) fn ldr_str_sp_rel<const LOAD: bool>(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let dst_reg = instr >> 8 & 7;
    let offset = (instr & 0xFF) << 2;
    ctx.next_instr.opcode = format!(
        "{} r{}, [r13 + #{:#05X}]",
        if LOAD { "ldr" } else { "str" },
        dst_reg,
        offset
    );
}

pub(super) fn push_pop<const POP: bool>(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    ctx.next_instr.opcode = format!("{} {{", if POP { "pop" } else { "push" });
    let mut range_start = None;
    let mut separator = "";
    for reg in 0..8 {
        if instr & 1 << reg != 0 {
            range_start.get_or_insert(reg);
        } else if let Some(start) = range_start {
            let _ = if start == reg - 1 {
                write!(ctx.next_instr.opcode, "{} r{}", separator, start)
            } else {
                write!(
                    ctx.next_instr.opcode,
                    "{} r{}-r{}",
                    separator,
                    start,
                    reg - 1
                )
            };
            range_start = None;
            separator = ",";
        }
    }
    if instr & 1 << 8 != 0 {
        let _ = write!(
            ctx.next_instr.opcode,
            "{} {}",
            separator,
            if POP { "r15" } else { "r14" }
        );
    }
    if instr & 0x1FF != 0 {
        ctx.next_instr.opcode.push(' ');
    }
    ctx.next_instr.opcode.push('}');
}

pub(super) fn ldmia_stmia<const LOAD: bool>(ctx: &mut Context, instr: u16) {
    ctx.branch_addr_base = None;
    let base_reg = instr >> 8 & 7;
    ctx.next_instr.opcode = format!("{}ia r{}!, {{", if LOAD { "ldm" } else { "stm" }, base_reg);
    let mut range_start = None;
    let mut separator = "";
    for reg in 0..8 {
        if instr & 1 << reg != 0 {
            range_start.get_or_insert(reg);
        } else if let Some(start) = range_start {
            let _ = if start == reg - 1 {
                write!(ctx.next_instr.opcode, "{} r{}", separator, start)
            } else {
                write!(
                    ctx.next_instr.opcode,
                    "{} r{}-r{}",
                    separator,
                    start,
                    reg - 1
                )
            };
            range_start = None;
            separator = ",";
        }
    }
    if instr & 0xFF != 0 {
        ctx.next_instr.opcode.push(' ');
    }
    ctx.next_instr.opcode.push('}');
}
