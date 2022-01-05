use super::super::{
    common::{MiscAddressing, ShiftTy, WbAddressing, WbOffTy},
    Context,
};
use core::fmt::Write;

pub(super) fn load_store_wb<
    const LOAD: bool,
    const BYTE: bool,
    const OFF_TY: WbOffTy,
    const UPWARDS: bool,
    const ADDRESSING: WbAddressing,
>(
    ctx: &mut Context,
    instr: u32,
    cond: &'static str,
) {
    let src_dst_reg = instr >> 12 & 0xF;
    let base_reg = instr >> 16 & 0xF;
    ctx.next_instr.opcode = format!(
        "{}{}{}{} r{}, [r{}",
        if LOAD { "ldr" } else { "str" },
        cond,
        if BYTE { "b" } else { "" },
        if ADDRESSING == WbAddressing::PostUser {
            "t"
        } else {
            ""
        },
        src_dst_reg,
        base_reg
    );
    if !ADDRESSING.preincrement() {
        ctx.next_instr.opcode.push(']');
    }
    let offset_sign = if UPWARDS { "" } else { "-" };
    match OFF_TY {
        WbOffTy::Imm => {
            let offset = instr & 0xFFF;
            if offset != 0 {
                let _ = write!(ctx.next_instr.opcode, ", #{}{:#05X}", offset_sign, offset);
            }
        }
        WbOffTy::Reg(shift_ty) => {
            let off_reg = instr & 0xF;
            let _ = write!(ctx.next_instr.opcode, ", {}r{}, ", offset_sign, off_reg);
            let mut shift = (instr >> 7 & 0x1F) as u8;
            if matches!(shift_ty, ShiftTy::Lsr | ShiftTy::Asr) && shift == 0 {
                shift = 32;
            }
            let _ = match shift_ty {
                ShiftTy::Lsl => write!(ctx.next_instr.opcode, "lsl #{}", shift),
                ShiftTy::Lsr => write!(ctx.next_instr.opcode, "lsr #{}", shift),
                ShiftTy::Asr => write!(ctx.next_instr.opcode, "asr #{}", shift),
                ShiftTy::Ror => {
                    if shift == 0 {
                        write!(ctx.next_instr.opcode, "rrx")
                    } else {
                        write!(ctx.next_instr.opcode, "ror #{}", shift)
                    }
                }
            };
        }
    }
    if ADDRESSING.preincrement() {
        ctx.next_instr.opcode.push(']');
        if ADDRESSING.writeback() {
            ctx.next_instr.opcode.push('!');
        }
    }
    if (BYTE && src_dst_reg == 15)
        || (ADDRESSING.writeback() && (base_reg == src_dst_reg || base_reg == 15))
    {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn load_store_misc<
    const LOAD: bool,
    const SUFFIX: &'static str,
    const OFF_IMM: bool,
    const UPWARDS: bool,
    const ADDRESSING: MiscAddressing,
>(
    ctx: &mut Context,
    instr: u32,
    cond: &'static str,
) {
    let src_dst_reg = instr >> 12 & 0xF;
    let base_reg = instr >> 16 & 0xF;
    ctx.next_instr.opcode = format!(
        "{}{}{} r{}, [r{}",
        if LOAD { "ldr" } else { "str" },
        cond,
        SUFFIX,
        src_dst_reg,
        base_reg
    );
    if !ADDRESSING.preincrement() {
        ctx.next_instr.opcode.push(']');
    }
    let offset_sign = if UPWARDS { "" } else { "-" };
    if OFF_IMM {
        let offset = (instr & 0xF) | (instr >> 4 & 0xF0);
        if offset != 0 {
            let _ = write!(ctx.next_instr.opcode, ", #{}{:#05X}", offset_sign, offset);
        }
    } else {
        let off_reg = instr & 0xF;
        let _ = write!(ctx.next_instr.opcode, ", {}r{}", offset_sign, off_reg);
    }
    if ADDRESSING.preincrement() {
        ctx.next_instr.opcode.push(']');
        if ADDRESSING.writeback() {
            ctx.next_instr.opcode.push('!');
        }
    }
    if src_dst_reg == 15
        || (!OFF_IMM && instr & 0xF00 != 0)
        || (ADDRESSING.writeback() && (base_reg == src_dst_reg || base_reg == 15))
        || (SUFFIX == "d" && src_dst_reg & 1 != 0)
    {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn swp_swpb<const BYTE: bool>(ctx: &mut Context, instr: u32, cond: &'static str) {
    let dst_reg = instr >> 12 & 0xF;
    let src_reg = instr & 0xF;
    let addr_reg = instr >> 16 & 0xF;
    ctx.next_instr.opcode = format!(
        "swp{}{} r{}, r{}, [r{}]",
        cond,
        if BYTE { "b" } else { "" },
        dst_reg,
        src_reg,
        addr_reg
    );
    if instr >> 8 & 0xF != 0 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn ldm_stm<
    const LOAD: bool,
    const INCREMENT: bool,
    const BASE_EXCLUDED: bool,
    const WRITEBACK: bool,
    const S_BIT: bool,
>(
    ctx: &mut Context,
    instr: u32,
    cond: &'static str,
) {
    let base_reg = instr >> 16 & 0xF;
    ctx.next_instr.opcode = format!(
        "{}{}{}{} r{}{}, {{",
        if LOAD { "ldm" } else { "stm" },
        cond,
        if INCREMENT { 'i' } else { 'd' },
        if BASE_EXCLUDED { 'b' } else { 'a' },
        base_reg,
        if WRITEBACK { "!" } else { "" }
    );

    let mut range_start = None;
    let mut separator = "";
    for reg in 0..17 {
        if reg < 16 && instr & 1 << reg != 0 {
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

    if instr & 0xFFFF != 0 {
        ctx.next_instr.opcode.push(' ');
    }
    ctx.next_instr.opcode.push('}');

    if S_BIT {
        ctx.next_instr.opcode.push('^');
    }
    if (WRITEBACK && (instr & 1 << base_reg != 0 || base_reg == 15))
        || (S_BIT && (!LOAD || instr & 1 << 15 == 0) && WRITEBACK)
        || instr & 0xFFFF == 0
    {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn pld(ctx: &mut Context, instr: u32) {
    let base_reg = instr >> 16 & 0xF;
    ctx.next_instr.opcode = format!("pld [r{}, ", base_reg);
    let offset_sign = if instr & 1 << 23 == 0 { "-" } else { "" };
    // TODO
    let _ = write!(ctx.next_instr.opcode, "{}<TODO>", offset_sign);
    ctx.next_instr.opcode.push(']');
}
