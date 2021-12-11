use super::super::Context;
use core::fmt::Write;

pub(super) fn invalid_dsp_mul(ctx: &mut Context, _instr: u32, cond: &'static str) {
    ctx.next_instr.opcode = if cond.is_empty() {
        "<Invalid DSP multiply>".to_string()
    } else {
        format!("<Invalid DSP multiply, {}>", cond)
    };
}

pub(super) fn mrs<const SPSR: bool>(ctx: &mut Context, instr: u32, cond: &'static str) {
    let dst_reg = instr >> 12 & 0xF;
    ctx.next_instr.opcode = format!(
        "mrs{} r{}, {}",
        cond,
        dst_reg,
        if SPSR { "spsr" } else { "cpsr" }
    );
    if dst_reg == 15 || instr & 0xFFF != 0 || instr >> 16 & 0xF != 0xF {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn msr<const IMM: bool, const SPSR: bool>(
    ctx: &mut Context,
    instr: u32,
    cond: &'static str,
) {
    let mask = format!(
        "{}{}{}{}",
        if instr & 1 << 16 == 0 { "" } else { "c" },
        if instr & 1 << 17 == 0 { "" } else { "x" },
        if instr & 1 << 18 == 0 { "" } else { "s" },
        if instr & 1 << 19 == 0 { "" } else { "f" },
    );
    let cpsr_spsr = if SPSR { "spsr" } else { "cpsr" };
    ctx.next_instr.opcode = if IMM {
        let src = (instr & 0xFF).rotate_right(instr >> 7 & 0x1E);
        format!("msr{} {}_{}, #{:#010X}", cond, cpsr_spsr, mask, src)
    } else {
        let src_reg = instr & 0xF;
        format!("msr{} {}_{}, r{}", cond, cpsr_spsr, mask, src_reg)
    };
    if (!IMM && instr >> 8 & 0xF != 0) || instr >> 12 & 0xF != 0xF {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn bkpt(ctx: &mut Context, instr: u32, cond: &'static str) {
    let comment = instr & 0xFF;
    ctx.next_instr.opcode = format!("bkpt{} #{:#04X}", cond, comment);
    if instr >> 28 != 0 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn swi(ctx: &mut Context, instr: u32, cond: &'static str) {
    let comment = instr & 0xFF;
    ctx.next_instr.opcode = format!("swi{} #{:#04X}", cond, comment);
}

pub(super) fn undefined(ctx: &mut Context, _instr: u32, cond: &'static str) {
    ctx.next_instr.opcode = format!("udf{}", cond);
}

pub(super) fn undefined_uncond(ctx: &mut Context, _instr: u32) {
    ctx.next_instr.opcode = "udf".to_string();
}

#[allow(clippy::similar_names)]
pub(super) fn mrc_mcr<const LOAD: bool>(ctx: &mut Context, instr: u32, cond: &'static str) {
    let src_dst_reg = instr >> 12 & 0xF;
    let opcode_1 = instr >> 21 & 7;
    let coproc_rn = instr >> 16 & 0xF;
    let coproc_index = instr >> 8 & 0xF;
    let opcode_2 = instr >> 5 & 7;
    let coproc_rm = instr & 0xF;
    ctx.next_instr.opcode = format!(
        "{}{} p{}, {}, r{}, cr{}, cr{}",
        if LOAD { "mrc" } else { "mcr" },
        cond,
        coproc_index,
        opcode_1,
        src_dst_reg,
        coproc_rn,
        coproc_rm
    );
    if opcode_2 != 0 {
        let _ = write!(ctx.next_instr.opcode, ", {}", opcode_2);
    }
    if !LOAD && src_dst_reg == 15 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn mrc2_mcr2<const LOAD: bool>(ctx: &mut Context, instr: u32) {
    mrc_mcr::<LOAD>(ctx, instr, "2");
}

pub(super) fn mrrc_mcrr<const LOAD: bool>(ctx: &mut Context, instr: u32, cond: &'static str) {
    let src_dst_reg_low = instr >> 12 & 0xF;
    let src_dst_reg_high = instr >> 16 & 0xF;
    let coproc_index = instr >> 8 & 0xF;
    let opcode = instr >> 4 & 7;
    let coproc_rm = instr & 0xF;
    ctx.next_instr.opcode = format!(
        "{}{} p{}, {}, r{}, r{}, cr{}",
        if LOAD { "mrrc" } else { "mcrr" },
        cond,
        coproc_index,
        opcode,
        src_dst_reg_low,
        src_dst_reg_high,
        coproc_rm
    );
    if src_dst_reg_low == 15 || src_dst_reg_high == 15 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

#[allow(clippy::similar_names)]
pub(super) fn cdp(ctx: &mut Context, instr: u32, cond: &'static str) {
    let coproc_rd = instr >> 12 & 0xF;
    let opcode_1 = instr >> 20 & 0xF;
    let coproc_rn = instr >> 16 & 0xF;
    let coproc_index = instr >> 8 & 0xF;
    let opcode_2 = instr >> 5 & 7;
    let coproc_rm = instr & 0xF;
    ctx.next_instr.opcode = format!(
        "cdp{} p{}, {:#03X}, r{}, cr{}, cr{}, {:#03X}",
        cond, coproc_index, opcode_1, coproc_rd, coproc_rn, coproc_rm, opcode_2
    );
}

pub(super) fn cdp2(ctx: &mut Context, instr: u32) {
    cdp(ctx, instr, "2");
}

pub(super) fn ldc_stc<const LOAD: bool>(ctx: &mut Context, instr: u32, cond: &'static str) {
    let coproc_index = instr >> 8 & 0xF;
    let coproc_src_dst_reg = instr >> 12 & 0xF;
    let base_reg = instr >> 16 & 0xF;
    let offset = instr & 0xFF;
    ctx.next_instr.opcode = format!(
        "{}{} p{}, cr{}, [r{}",
        if LOAD { "ldc" } else { "stc" },
        cond,
        coproc_index,
        coproc_src_dst_reg,
        base_reg
    );
    let p = instr & 1 << 24 != 0;
    let writeback = instr & 1 << 21 != 0;
    let offset_sign = if instr & 1 << 23 == 0 { "-" } else { "" };
    let _ = match (p, writeback) {
        (false, false) => write!(
            ctx.next_instr.opcode,
            ", #{}{:#05X}]",
            offset_sign,
            offset << 2
        ),
        (false, true) => write!(
            ctx.next_instr.opcode,
            ", #{}{:#05X}]!",
            offset_sign,
            offset << 2
        ),
        (true, false) => write!(
            ctx.next_instr.opcode,
            "], #{}{:#05X}",
            offset_sign,
            offset << 2
        ),
        (true, true) => write!(ctx.next_instr.opcode, ", {{{}}}", offset),
    };
    if writeback && base_reg == 15 {
        ctx.next_instr.comment = "Unpredictable".to_string();
    }
}

pub(super) fn ldc2_stc2<const LOAD: bool>(ctx: &mut Context, instr: u32) {
    ldc_stc::<LOAD>(ctx, instr, "2");
}
