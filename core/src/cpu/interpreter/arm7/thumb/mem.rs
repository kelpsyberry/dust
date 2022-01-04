use super::super::{add_cycles, reload_pipeline};
use crate::{
    cpu::{
        arm7::bus,
        bus::CpuAccess,
        interpreter::{common::StateSource, Engine},
    },
    emu::Emu,
    utils::schedule::RawTimestamp,
};
use core::intrinsics::unlikely;

pub fn ldr<const IMM: bool>(emu: &mut Emu<Engine>, instr: u16) {
    let addr = reg!(emu.arm7, instr >> 3 & 7).wrapping_add(if IMM {
        (instr >> 4 & 0x7C) as u32
    } else {
        reg!(emu.arm7, instr >> 6 & 7)
    });
    inc_r15!(emu.arm7, 2);
    let result = bus::read_32::<CpuAccess, _>(emu, addr).rotate_right((addr & 3) << 3);
    let cycles = bus::timing_32::<_, false>(emu, addr);
    add_cycles(emu, cycles as RawTimestamp + 1);
    reg!(emu.arm7, instr & 7) = result;
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn str<const IMM: bool>(emu: &mut Emu<Engine>, instr: u16) {
    let addr = reg!(emu.arm7, instr >> 3 & 7).wrapping_add(if IMM {
        (instr >> 4 & 0x7C) as u32
    } else {
        reg!(emu.arm7, instr >> 6 & 7)
    });
    inc_r15!(emu.arm7, 2);
    bus::write_32::<CpuAccess, _>(emu, addr, reg!(emu.arm7, instr & 7));
    let cycles = bus::timing_32::<_, false>(emu, addr);
    add_cycles(emu, cycles as RawTimestamp);
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn ldrh<const IMM: bool>(emu: &mut Emu<Engine>, instr: u16) {
    let addr = reg!(emu.arm7, instr >> 3 & 7).wrapping_add(if IMM {
        (instr >> 5 & 0x3E) as u32
    } else {
        reg!(emu.arm7, instr >> 6 & 7)
    });
    inc_r15!(emu.arm7, 2);
    let result = (bus::read_16::<CpuAccess, _>(emu, addr) as u32).rotate_right((addr & 1) << 3);
    let cycles = bus::timing_16::<_, false>(emu, addr);
    add_cycles(emu, cycles as RawTimestamp + 1);
    reg!(emu.arm7, instr & 7) = result;
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn strh<const IMM: bool>(emu: &mut Emu<Engine>, instr: u16) {
    let addr = reg!(emu.arm7, instr >> 3 & 7).wrapping_add(if IMM {
        (instr >> 5 & 0x3E) as u32
    } else {
        reg!(emu.arm7, instr >> 6 & 7)
    });
    inc_r15!(emu.arm7, 2);
    bus::write_16::<CpuAccess, _>(emu, addr, reg!(emu.arm7, instr & 7) as u16);
    let cycles = bus::timing_16::<_, false>(emu, addr);
    add_cycles(emu, cycles as RawTimestamp);
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn ldrb<const IMM: bool>(emu: &mut Emu<Engine>, instr: u16) {
    let addr = reg!(emu.arm7, instr >> 3 & 7).wrapping_add(if IMM {
        (instr >> 6 & 0x1F) as u32
    } else {
        reg!(emu.arm7, instr >> 6 & 7)
    });
    inc_r15!(emu.arm7, 2);
    let result = bus::read_8::<CpuAccess, _>(emu, addr) as u32;
    let cycles = bus::timing_16::<_, false>(emu, addr);
    add_cycles(emu, cycles as RawTimestamp + 1);
    reg!(emu.arm7, instr & 7) = result;
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn strb<const IMM: bool>(emu: &mut Emu<Engine>, instr: u16) {
    let addr = reg!(emu.arm7, instr >> 3 & 7).wrapping_add(if IMM {
        (instr >> 6 & 0x1F) as u32
    } else {
        reg!(emu.arm7, instr >> 6 & 7)
    });
    inc_r15!(emu.arm7, 2);
    bus::write_8::<CpuAccess, _>(emu, addr, reg!(emu.arm7, instr & 7) as u8);
    let cycles = bus::timing_16::<_, false>(emu, addr);
    add_cycles(emu, cycles as RawTimestamp);
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn ldrsh(emu: &mut Emu<Engine>, instr: u16) {
    let addr = reg!(emu.arm7, instr >> 3 & 7).wrapping_add(reg!(emu.arm7, instr >> 6 & 7));
    inc_r15!(emu.arm7, 2);
    let result = {
        let aligned = bus::read_16::<CpuAccess, _>(emu, addr);
        ((aligned as i32) << 16 >> (((addr & 1) | 2) << 3)) as u32
    };
    let cycles = bus::timing_16::<_, false>(emu, addr);
    add_cycles(emu, cycles as RawTimestamp + 1);
    reg!(emu.arm7, instr & 7) = result;
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn ldrsb(emu: &mut Emu<Engine>, instr: u16) {
    let addr = reg!(emu.arm7, instr >> 3 & 7).wrapping_add(reg!(emu.arm7, instr >> 6 & 7));
    inc_r15!(emu.arm7, 2);
    let result = bus::read_8::<CpuAccess, _>(emu, addr) as i8 as u32;
    let cycles = bus::timing_16::<_, false>(emu, addr);
    add_cycles(emu, cycles as RawTimestamp + 1);
    reg!(emu.arm7, instr & 7) = result;
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn ldr_pc_rel(emu: &mut Emu<Engine>, instr: u16) {
    let r15 = reg!(emu.arm7, 15);
    let addr = (r15 & !3).wrapping_add(((instr & 0xFF) << 2) as u32);
    inc_r15!(emu.arm7, 2);
    let result = bus::read_32::<CpuAccess, _>(emu, addr);
    let cycles = bus::timing_32::<_, false>(emu, addr);
    add_cycles(emu, cycles as RawTimestamp + 1);
    reg!(emu.arm7, instr >> 8 & 7) = result;
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn ldr_sp_rel(emu: &mut Emu<Engine>, instr: u16) {
    let addr = reg!(emu.arm7, 13).wrapping_add(((instr & 0xFF) << 2) as u32);
    inc_r15!(emu.arm7, 2);
    let result = bus::read_32::<CpuAccess, _>(emu, addr).rotate_right((addr & 3) << 3);
    let cycles = bus::timing_32::<_, false>(emu, addr);
    add_cycles(emu, cycles as RawTimestamp + 1);
    reg!(emu.arm7, instr >> 8 & 7) = result;
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn str_sp_rel(emu: &mut Emu<Engine>, instr: u16) {
    let addr = reg!(emu.arm7, 13).wrapping_add(((instr & 0xFF) << 2) as u32);
    inc_r15!(emu.arm7, 2);
    bus::write_32::<CpuAccess, _>(emu, addr, reg!(emu.arm7, instr >> 8 & 7));
    let cycles = bus::timing_32::<_, false>(emu, addr);
    add_cycles(emu, cycles as RawTimestamp);
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn push<const PUSH_R14: bool>(emu: &mut Emu<Engine>, instr: u16) {
    inc_r15!(emu.arm7, 2);
    if unlikely(instr as u8 == 0 && !PUSH_R14) {
        let start_addr = reg!(emu.arm7, 13).wrapping_sub(0x40);
        reg!(emu.arm7, 13) = start_addr;
        bus::write_32::<CpuAccess, _>(emu, start_addr, reg!(emu.arm7, 15));
        let cycles = bus::timing_32::<_, false>(emu, start_addr);
        add_cycles(emu, cycles as RawTimestamp);
        emu.arm7.engine_data.prefetch_nseq = true;
        return;
    }
    let mut cur_addr =
        reg!(emu.arm7, 13).wrapping_sub(((instr as u8).count_ones() + PUSH_R14 as u32) << 2);
    reg!(emu.arm7, 13) = cur_addr;
    let mut timings = emu.arm7.bus_timings.get(cur_addr);
    let mut access_cycles = timings.n32;
    for reg in 0..8 {
        if instr & 1 << reg != 0 {
            bus::write_32::<CpuAccess, _>(emu, cur_addr, reg!(emu.arm7, reg));
            add_cycles(emu, access_cycles as RawTimestamp);
            cur_addr = cur_addr.wrapping_add(4);
            if cur_addr & 0x3FC == 0 {
                timings = emu.arm7.bus_timings.get(cur_addr);
                access_cycles = timings.n32;
            } else {
                access_cycles = timings.s32;
            }
        }
    }
    if PUSH_R14 {
        bus::write_32::<CpuAccess, _>(emu, cur_addr, reg!(emu.arm7, 14));
        add_cycles(emu, access_cycles as RawTimestamp);
    }
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn pop<const POP_R15: bool>(emu: &mut Emu<Engine>, instr: u16) {
    // NOTE: Writeback should actually happen during the first load, but the effects can't be seen
    // (and `count_ones` is potentially slow).
    let mut cur_addr = reg!(emu.arm7, 13);
    inc_r15!(emu.arm7, 2);
    if unlikely(instr as u8 == 0 && !POP_R15) {
        reg!(emu.arm7, 13) = cur_addr.wrapping_add(0x40);
        let result = bus::read_32::<CpuAccess, _>(emu, cur_addr);
        let cycles = bus::timing_32::<_, false>(emu, cur_addr);
        add_cycles(emu, cycles as RawTimestamp + 1);
        reg!(emu.arm7, 15) = result;
        return reload_pipeline::<{ StateSource::Thumb }>(emu);
    }
    let mut timings = emu.arm7.bus_timings.get(cur_addr);
    let mut access_cycles = timings.n32;
    for reg in 0..8 {
        if instr & 1 << reg != 0 {
            let result = bus::read_32::<CpuAccess, _>(emu, cur_addr);
            reg!(emu.arm7, reg) = result;
            add_cycles(emu, access_cycles as RawTimestamp);
            cur_addr = cur_addr.wrapping_add(4);
            if cur_addr & 0x3FC == 0 {
                timings = emu.arm7.bus_timings.get(cur_addr);
                access_cycles = timings.n32;
            } else {
                access_cycles = timings.s32;
            }
        }
    }
    if POP_R15 {
        reg!(emu.arm7, 13) = cur_addr.wrapping_add(4);
        let result = bus::read_32::<CpuAccess, _>(emu, cur_addr);
        add_cycles(emu, access_cycles as RawTimestamp + 1);
        reg!(emu.arm7, 15) = result;
        reload_pipeline::<{ StateSource::Thumb }>(emu);
    } else {
        reg!(emu.arm7, 13) = cur_addr;
        add_cycles(emu, 1);
        emu.arm7.engine_data.prefetch_nseq = true;
    }
}

pub fn ldmia(emu: &mut Emu<Engine>, instr: u16) {
    let base_reg = instr >> 8 & 7;
    let mut cur_addr = reg!(emu.arm7, base_reg);
    inc_r15!(emu.arm7, 2);
    if unlikely(instr as u8 == 0) {
        reg!(emu.arm7, base_reg) = cur_addr.wrapping_add(0x40);
        let result = bus::read_32::<CpuAccess, _>(emu, cur_addr);
        let cycles = bus::timing_32::<_, false>(emu, cur_addr);
        add_cycles(emu, cycles as RawTimestamp + 1);
        reg!(emu.arm7, 15) = result;
        return reload_pipeline::<{ StateSource::Thumb }>(emu);
    }
    reg!(emu.arm7, base_reg) = cur_addr.wrapping_add((instr as u8).count_ones() << 2);
    let mut timings = emu.arm7.bus_timings.get(cur_addr);
    let mut access_cycles = timings.n32;
    for reg in 0..8 {
        if instr & 1 << reg != 0 {
            let result = bus::read_32::<CpuAccess, _>(emu, cur_addr);
            reg!(emu.arm7, reg) = result;
            add_cycles(emu, access_cycles as RawTimestamp);
            cur_addr = cur_addr.wrapping_add(4);
            if cur_addr & 0x3FC == 0 {
                timings = emu.arm7.bus_timings.get(cur_addr);
                access_cycles = timings.n32;
            } else {
                access_cycles = timings.s32;
            }
        }
    }
    add_cycles(emu, 1);
    emu.arm7.engine_data.prefetch_nseq = true;
}

pub fn stmia(emu: &mut Emu<Engine>, instr: u16) {
    let base_reg = instr >> 8 & 7;
    let mut cur_addr = reg!(emu.arm7, base_reg);
    inc_r15!(emu.arm7, 2);
    if unlikely(instr as u8 == 0) {
        reg!(emu.arm7, base_reg) = cur_addr.wrapping_add(0x40);
        bus::write_32::<CpuAccess, _>(emu, cur_addr, reg!(emu.arm7, 15));
        let cycles = bus::timing_32::<_, false>(emu, cur_addr);
        add_cycles(emu, cycles as RawTimestamp);
        emu.arm7.engine_data.prefetch_nseq = true;
        return;
    }
    let end_addr = cur_addr.wrapping_add((instr as u8).count_ones() << 2);
    let mut timings = emu.arm7.bus_timings.get(cur_addr);
    let mut access_cycles = timings.n32;
    for reg in 0..8 {
        if instr & 1 << reg != 0 {
            bus::write_32::<CpuAccess, _>(emu, cur_addr, reg!(emu.arm7, reg));
            add_cycles(emu, access_cycles as RawTimestamp);
            cur_addr = cur_addr.wrapping_add(4);
            if cur_addr & 0x3FC == 0 {
                timings = emu.arm7.bus_timings.get(cur_addr);
                access_cycles = timings.n32;
            } else {
                access_cycles = timings.s32;
            }
            reg!(emu.arm7, base_reg) = end_addr;
        }
    }
    emu.arm7.engine_data.prefetch_nseq = true;
}
