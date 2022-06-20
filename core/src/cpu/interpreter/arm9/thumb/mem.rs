use super::super::{
    add_bus_cycles, add_cycles, add_interlock, apply_reg_interlock_1, apply_reg_interlocks_2,
    apply_reg_interlocks_3, can_read, can_write, handle_data_abort, prefetch_thumb,
    reload_pipeline, write_reg_interlock,
};
use crate::{
    cpu::{
        arm9::bus,
        bus::CpuAccess,
        interpreter::{common::StateSource, Interpreter},
    },
    emu::Emu,
    utils::schedule::RawTimestamp,
};
use core::intrinsics::unlikely;

// TODO: Find out timing for register offsets. ARM single data transfers have an additional
//       internal cycle for address calculation when using scaled register offsets, but maybe that
//       applies to all register offsets, unconditionally.
// TODO: Check data abort timings.

pub fn ldr<const IMM: bool>(emu: &mut Emu<Interpreter>, instr: u16) {
    let base_reg = (instr >> 3 & 7) as u8;
    let addr = reg!(emu.arm9, base_reg).wrapping_add(if IMM {
        apply_reg_interlock_1::<false>(emu, base_reg);
        (instr >> 4 & 0x7C) as u32
    } else {
        let off_reg = (instr >> 6 & 7) as u8;
        apply_reg_interlocks_2::<0, false>(emu, base_reg, off_reg);
        reg!(emu.arm9, off_reg)
    });
    prefetch_thumb::<false, true>(emu);
    if unlikely(!can_read(
        emu,
        addr,
        emu.arm9.engine_data.regs.is_in_priv_mode(),
    )) {
        emu.arm9.engine_data.data_cycles = 1;
        add_bus_cycles(emu, 2);
        add_cycles(emu, 1);
        return handle_data_abort::<true>(emu, addr);
    }
    let result = bus::read_32::<CpuAccess, _, false>(emu, addr).rotate_right((addr & 3) << 3);
    emu.arm9.engine_data.data_cycles = emu.arm9.cp15.timings.get(addr).r_n32_data;
    add_bus_cycles(emu, 1);
    write_reg_interlock(
        emu,
        (instr & 7) as u8,
        result,
        1 + (addr & 3 != 0) as RawTimestamp,
        1,
    );
}

pub fn str<const IMM: bool>(emu: &mut Emu<Interpreter>, instr: u16) {
    let base_reg = (instr >> 3 & 7) as u8;
    let src_reg = (instr & 7) as u8;
    let addr = reg!(emu.arm9, base_reg).wrapping_add(if IMM {
        apply_reg_interlocks_2::<0, true>(emu, base_reg, src_reg);
        (instr >> 4 & 0x7C) as u32
    } else {
        let off_reg = (instr >> 6 & 7) as u8;
        apply_reg_interlocks_3::<0, true>(emu, base_reg, off_reg, src_reg);
        reg!(emu.arm9, off_reg)
    });
    prefetch_thumb::<false, true>(emu);
    if unlikely(!can_write(
        emu,
        addr,
        emu.arm9.engine_data.regs.is_in_priv_mode(),
    )) {
        emu.arm9.engine_data.data_cycles = 1;
        add_bus_cycles(emu, 2);
        add_cycles(emu, 1);
        return handle_data_abort::<true>(emu, addr);
    }
    bus::write_32::<CpuAccess, _>(emu, addr, reg!(emu.arm9, src_reg));
    emu.arm9.engine_data.data_cycles = emu.arm9.cp15.timings.get(addr).w_n32_data;
    add_bus_cycles(emu, 1);
}

pub fn ldrh<const IMM: bool>(emu: &mut Emu<Interpreter>, instr: u16) {
    let base_reg = (instr >> 3 & 7) as u8;
    let addr = reg!(emu.arm9, base_reg).wrapping_add(if IMM {
        apply_reg_interlock_1::<false>(emu, base_reg);
        (instr >> 5 & 0x3E) as u32
    } else {
        let off_reg = (instr >> 6 & 7) as u8;
        apply_reg_interlocks_2::<0, false>(emu, base_reg, off_reg);
        reg!(emu.arm9, off_reg)
    });
    prefetch_thumb::<false, true>(emu);
    if unlikely(!can_read(
        emu,
        addr,
        emu.arm9.engine_data.regs.is_in_priv_mode(),
    )) {
        emu.arm9.engine_data.data_cycles = 1;
        add_bus_cycles(emu, 2);
        add_cycles(emu, 1);
        return handle_data_abort::<true>(emu, addr);
    }
    let result = bus::read_16::<CpuAccess, _>(emu, addr) as u32;
    emu.arm9.engine_data.data_cycles = emu.arm9.cp15.timings.get(addr).r_n16_data;
    add_bus_cycles(emu, 1);
    write_reg_interlock(emu, (instr & 7) as u8, result, 2, 1);
}

pub fn strh<const IMM: bool>(emu: &mut Emu<Interpreter>, instr: u16) {
    let base_reg = (instr >> 3 & 7) as u8;
    let src_reg = (instr & 7) as u8;
    let addr = reg!(emu.arm9, base_reg).wrapping_add(if IMM {
        apply_reg_interlocks_2::<0, true>(emu, base_reg, src_reg);
        (instr >> 5 & 0x3E) as u32
    } else {
        let off_reg = (instr >> 6 & 7) as u8;
        apply_reg_interlocks_3::<0, true>(emu, base_reg, off_reg, src_reg);
        reg!(emu.arm9, off_reg)
    });
    prefetch_thumb::<false, true>(emu);
    if unlikely(!can_write(
        emu,
        addr,
        emu.arm9.engine_data.regs.is_in_priv_mode(),
    )) {
        emu.arm9.engine_data.data_cycles = 1;
        add_bus_cycles(emu, 2);
        add_cycles(emu, 1);
        return handle_data_abort::<true>(emu, addr);
    }
    bus::write_16::<CpuAccess, _>(emu, addr, reg!(emu.arm9, src_reg) as u16);
    emu.arm9.engine_data.data_cycles = emu.arm9.cp15.timings.get(addr).w_n16_data;
    add_bus_cycles(emu, 1);
}

pub fn ldrb<const IMM: bool>(emu: &mut Emu<Interpreter>, instr: u16) {
    let base_reg = (instr >> 3 & 7) as u8;
    let addr = reg!(emu.arm9, base_reg).wrapping_add(if IMM {
        apply_reg_interlock_1::<false>(emu, base_reg);
        (instr >> 6 & 0x1F) as u32
    } else {
        let off_reg = (instr >> 6 & 7) as u8;
        apply_reg_interlocks_2::<0, false>(emu, base_reg, off_reg);
        reg!(emu.arm9, off_reg)
    });
    prefetch_thumb::<false, true>(emu);
    if unlikely(!can_read(
        emu,
        addr,
        emu.arm9.engine_data.regs.is_in_priv_mode(),
    )) {
        emu.arm9.engine_data.data_cycles = 1;
        add_bus_cycles(emu, 2);
        add_cycles(emu, 1);
        return handle_data_abort::<true>(emu, addr);
    }
    let result = bus::read_8::<CpuAccess, _>(emu, addr) as u32;
    emu.arm9.engine_data.data_cycles = emu.arm9.cp15.timings.get(addr).r_n16_data;
    add_bus_cycles(emu, 1);
    write_reg_interlock(emu, (instr & 7) as u8, result, 2, 1);
}

pub fn strb<const IMM: bool>(emu: &mut Emu<Interpreter>, instr: u16) {
    let base_reg = (instr >> 3 & 7) as u8;
    let src_reg = (instr & 7) as u8;
    let addr = reg!(emu.arm9, base_reg).wrapping_add(if IMM {
        apply_reg_interlocks_2::<0, true>(emu, base_reg, src_reg);
        (instr >> 6 & 0x1F) as u32
    } else {
        let off_reg = (instr >> 6 & 7) as u8;
        apply_reg_interlocks_3::<0, true>(emu, base_reg, off_reg, src_reg);
        reg!(emu.arm9, off_reg)
    });
    prefetch_thumb::<false, true>(emu);
    if unlikely(!can_write(
        emu,
        addr,
        emu.arm9.engine_data.regs.is_in_priv_mode(),
    )) {
        emu.arm9.engine_data.data_cycles = 1;
        add_bus_cycles(emu, 2);
        add_cycles(emu, 1);
        return handle_data_abort::<true>(emu, addr);
    }
    bus::write_8::<CpuAccess, _>(emu, addr, reg!(emu.arm9, src_reg) as u8);
    emu.arm9.engine_data.data_cycles = emu.arm9.cp15.timings.get(addr).w_n16_data;
    add_bus_cycles(emu, 1);
}

pub fn ldrsh(emu: &mut Emu<Interpreter>, instr: u16) {
    let base_reg = (instr >> 3 & 7) as u8;
    let off_reg = (instr >> 6 & 7) as u8;
    let addr = reg!(emu.arm9, base_reg).wrapping_add(reg!(emu.arm9, off_reg));
    apply_reg_interlocks_2::<0, false>(emu, base_reg, off_reg);
    prefetch_thumb::<false, true>(emu);
    if unlikely(!can_read(
        emu,
        addr,
        emu.arm9.engine_data.regs.is_in_priv_mode(),
    )) {
        emu.arm9.engine_data.data_cycles = 1;
        add_bus_cycles(emu, 2);
        add_cycles(emu, 1);
        return handle_data_abort::<true>(emu, addr);
    }
    let result = bus::read_16::<CpuAccess, _>(emu, addr) as i16 as u32;
    emu.arm9.engine_data.data_cycles = emu.arm9.cp15.timings.get(addr).r_n16_data;
    add_bus_cycles(emu, 1);
    write_reg_interlock(emu, (instr & 7) as u8, result, 2, 1);
}

pub fn ldrsb(emu: &mut Emu<Interpreter>, instr: u16) {
    let base_reg = (instr >> 3 & 7) as u8;
    let off_reg = (instr >> 6 & 7) as u8;
    let addr = reg!(emu.arm9, base_reg).wrapping_add(reg!(emu.arm9, off_reg));
    apply_reg_interlocks_2::<0, false>(emu, base_reg, off_reg);
    prefetch_thumb::<false, true>(emu);
    if unlikely(!can_read(
        emu,
        addr,
        emu.arm9.engine_data.regs.is_in_priv_mode(),
    )) {
        emu.arm9.engine_data.data_cycles = 1;
        add_bus_cycles(emu, 2);
        add_cycles(emu, 1);
        return handle_data_abort::<true>(emu, addr);
    }
    let result = bus::read_8::<CpuAccess, _>(emu, addr) as i8 as u32;
    emu.arm9.engine_data.data_cycles = emu.arm9.cp15.timings.get(addr).r_n16_data;
    add_bus_cycles(emu, 1);
    write_reg_interlock(emu, (instr & 7) as u8, result, 2, 1);
}

pub fn ldr_pc_rel(emu: &mut Emu<Interpreter>, instr: u16) {
    let addr = (reg!(emu.arm9, 15) & !3).wrapping_add(((instr & 0xFF) << 2) as u32);
    prefetch_thumb::<false, true>(emu);
    if unlikely(!can_read(
        emu,
        addr,
        emu.arm9.engine_data.regs.is_in_priv_mode(),
    )) {
        emu.arm9.engine_data.data_cycles = 1;
        add_bus_cycles(emu, 2);
        add_cycles(emu, 1);
        return handle_data_abort::<true>(emu, addr);
    }
    let result = bus::read_32::<CpuAccess, _, false>(emu, addr);
    emu.arm9.engine_data.data_cycles = emu.arm9.cp15.timings.get(addr).r_n32_data;
    add_bus_cycles(emu, 1);
    write_reg_interlock(emu, (instr >> 8 & 7) as u8, result, 1, 1);
}

pub fn ldr_sp_rel(emu: &mut Emu<Interpreter>, instr: u16) {
    let addr = reg!(emu.arm9, 13).wrapping_add(((instr & 0xFF) << 2) as u32);
    prefetch_thumb::<false, true>(emu);
    if unlikely(!can_read(
        emu,
        addr,
        emu.arm9.engine_data.regs.is_in_priv_mode(),
    )) {
        emu.arm9.engine_data.data_cycles = 1;
        add_bus_cycles(emu, 2);
        add_cycles(emu, 1);
        return handle_data_abort::<true>(emu, addr);
    }
    let result = bus::read_32::<CpuAccess, _, false>(emu, addr).rotate_right((addr & 3) << 3);
    emu.arm9.engine_data.data_cycles = emu.arm9.cp15.timings.get(addr).r_n32_data;
    add_bus_cycles(emu, 1);
    write_reg_interlock(
        emu,
        (instr >> 8 & 7) as u8,
        result,
        1 + (addr & 3 != 0) as RawTimestamp,
        1,
    );
}

pub fn str_sp_rel(emu: &mut Emu<Interpreter>, instr: u16) {
    let src_reg = (instr >> 8 & 7) as u8;
    apply_reg_interlock_1::<false>(emu, src_reg);
    let addr = reg!(emu.arm9, 13).wrapping_add(((instr & 0xFF) << 2) as u32);
    prefetch_thumb::<false, true>(emu);
    if unlikely(!can_write(
        emu,
        addr,
        emu.arm9.engine_data.regs.is_in_priv_mode(),
    )) {
        emu.arm9.engine_data.data_cycles = 1;
        add_bus_cycles(emu, 2);
        add_cycles(emu, 1);
        return handle_data_abort::<true>(emu, addr);
    }
    bus::write_32::<CpuAccess, _>(emu, addr, reg!(emu.arm9, src_reg));
    emu.arm9.engine_data.data_cycles = emu.arm9.cp15.timings.get(addr).w_n32_data;
    add_bus_cycles(emu, 1);
}

// NOTE: Here, `prefetch_thumb` can be called before applying stored register interlocks, as they
//       happen in the execute stage, after the fetch has been initiated.
// TODO: Check timing after data aborts and with empty reg lists.

pub fn push<const PUSH_R14: bool>(emu: &mut Emu<Interpreter>, instr: u16) {
    prefetch_thumb::<false, true>(emu);
    if unlikely(!PUSH_R14 && instr as u8 == 0) {
        emu.arm9.engine_data.data_cycles = 1;
        add_bus_cycles(emu, 2);
        add_cycles(emu, 1);
        reg!(emu.arm9, 13) = reg!(emu.arm9, 13).wrapping_sub(0x40);
        return;
    }
    let start_addr =
        reg!(emu.arm9, 13).wrapping_sub(((instr as u8).count_ones() + PUSH_R14 as u32) << 2);
    let mut cur_addr = start_addr;
    let mut not_first = false;
    let mut timings = emu.arm9.cp15.timings.get(cur_addr);
    let mut access_cycles = timings.w_n32_data;
    for reg in 0..8 {
        if instr & 1 << reg != 0 {
            if not_first {
                add_cycles(emu, emu.arm9.engine_data.data_cycles as RawTimestamp);
            } else {
                apply_reg_interlock_1::<true>(emu, reg);
            }
            if unlikely(!can_write(
                emu,
                cur_addr,
                emu.arm9.engine_data.regs.is_in_priv_mode(),
            )) {
                // In case of a data abort, the instruction runs to completion before triggering
                // the exception (unclear what that means for timings, it's assumed all accesses
                // from that point on take 1 cycle).
                emu.arm9.engine_data.data_cycles = 1;
                add_bus_cycles(emu, 2);
                add_cycles(
                    emu,
                    ((instr as u8 & !((1 << reg) - 1)).count_ones() + PUSH_R14 as u32)
                        as RawTimestamp,
                );
                return handle_data_abort::<true>(emu, cur_addr);
            }
            bus::write_32::<CpuAccess, _>(emu, cur_addr, reg!(emu.arm9, reg));
            emu.arm9.engine_data.data_cycles = access_cycles;
            cur_addr = cur_addr.wrapping_add(4);
            if cur_addr & 0x3FC == 0 {
                timings = emu.arm9.cp15.timings.get(cur_addr);
                access_cycles = timings.w_n32_data;
            } else {
                access_cycles = timings.w_s32_data;
            }
            not_first = true;
        }
    }
    if PUSH_R14 {
        if not_first {
            add_cycles(emu, emu.arm9.engine_data.data_cycles as RawTimestamp);
        }
        if unlikely(!can_write(
            emu,
            cur_addr,
            emu.arm9.engine_data.regs.is_in_priv_mode(),
        )) {
            emu.arm9.engine_data.data_cycles = 1;
            add_bus_cycles(emu, 2);
            add_cycles(emu, 1);
            return handle_data_abort::<false>(emu, cur_addr);
        }
        bus::write_32::<CpuAccess, _>(emu, cur_addr, reg!(emu.arm9, 14));
        emu.arm9.engine_data.data_cycles = access_cycles;
    }
    add_bus_cycles(emu, 2);
    if if PUSH_R14 {
        instr as u8 == 0
    } else {
        instr as u8 & (instr as u8 - 1) == 0
    } {
        // Only one register present, add an internal cycle
        add_cycles(emu, emu.arm9.engine_data.data_cycles as RawTimestamp);
        emu.arm9.engine_data.data_cycles = 1;
    }
    reg!(emu.arm9, 13) = start_addr;
}

pub fn pop<const POP_R15: bool>(emu: &mut Emu<Interpreter>, instr: u16) {
    add_bus_cycles(emu, 2);
    prefetch_thumb::<false, true>(emu);
    if unlikely(!POP_R15 && instr as u8 == 0) {
        emu.arm9.engine_data.data_cycles = 1;
        add_cycles(emu, 1);
        reg!(emu.arm9, 13) = reg!(emu.arm9, 13).wrapping_add(0x40);
        return;
    }
    let mut cur_addr = reg!(emu.arm9, 13);
    let mut not_first = false;
    let mut timings = emu.arm9.cp15.timings.get(cur_addr);
    let mut access_cycles = timings.r_n32_data;
    for reg in 0..8 {
        if instr & 1 << reg != 0 {
            if not_first {
                add_cycles(emu, emu.arm9.engine_data.data_cycles as RawTimestamp);
            }
            if unlikely(!can_read(
                emu,
                cur_addr,
                emu.arm9.engine_data.regs.is_in_priv_mode(),
            )) {
                // In case of a data abort, the instruction runs to completion before triggering
                // the exception (unclear what that means for timings, it's assumed all accesses
                // from that point on take 1 cycle).
                emu.arm9.engine_data.data_cycles = 1;
                add_cycles(
                    emu,
                    ((instr as u8 & !((1 << reg) - 1)).count_ones() + POP_R15 as u32)
                        as RawTimestamp,
                );
                return handle_data_abort::<true>(emu, cur_addr);
            }
            let result = bus::read_32::<CpuAccess, _, false>(emu, cur_addr);
            emu.arm9.engine_data.data_cycles = access_cycles;
            reg!(emu.arm9, reg) = result;
            cur_addr = cur_addr.wrapping_add(4);
            if cur_addr & 0x3FC == 0 {
                timings = emu.arm9.cp15.timings.get(cur_addr);
                access_cycles = timings.r_n32_data;
            } else {
                access_cycles = timings.r_s32_data;
            }
            not_first = true;
        }
    }
    if POP_R15 {
        if not_first {
            add_cycles(emu, emu.arm9.engine_data.data_cycles as RawTimestamp);
        }
        emu.arm9.engine_data.data_cycles = 1;
        if unlikely(!can_read(
            emu,
            cur_addr,
            emu.arm9.engine_data.regs.is_in_priv_mode(),
        )) {
            add_cycles(emu, 1);
            return handle_data_abort::<false>(emu, cur_addr);
        }
        let result = bus::read_32::<CpuAccess, _, false>(emu, cur_addr);
        add_cycles(emu, access_cycles as RawTimestamp + 1);
        cur_addr = cur_addr.wrapping_add(4);
        reg!(emu.arm9, 15) = result;
        if emu.arm9.cp15.control().t_bit_load_disabled() {
            reload_pipeline::<{ StateSource::Thumb }>(emu);
        } else {
            reload_pipeline::<{ StateSource::R15Bit0 }>(emu);
        }
    } else if instr as u8 & (instr as u8 - 1) == 0 {
        // Only one register present, add an internal cycle
        add_cycles(emu, emu.arm9.engine_data.data_cycles as RawTimestamp);
        emu.arm9.engine_data.data_cycles = 1;
    } else {
        let last_reg = (7 - (instr as u8).leading_zeros()) as u8;
        add_interlock(emu, last_reg, 1, 1);
    }
    reg!(emu.arm9, 13) = cur_addr;
}

pub fn ldmia(emu: &mut Emu<Interpreter>, instr: u16) {
    let base_reg = (instr >> 8 & 7) as u8;
    apply_reg_interlock_1::<false>(emu, base_reg);
    add_bus_cycles(emu, 2);
    let mut cur_addr = reg!(emu.arm9, base_reg);
    prefetch_thumb::<false, true>(emu);
    if unlikely(instr as u8 == 0) {
        emu.arm9.engine_data.data_cycles = 1;
        add_cycles(emu, 1);
        reg!(emu.arm9, base_reg) = cur_addr.wrapping_add(0x40);
        return;
    }
    let mut not_first = false;
    let mut timings = emu.arm9.cp15.timings.get(cur_addr);
    let mut access_cycles = timings.r_n32_data;
    for reg in 0..8 {
        if instr & 1 << reg != 0 {
            if not_first {
                add_cycles(emu, emu.arm9.engine_data.data_cycles as RawTimestamp);
            }
            if unlikely(!can_read(
                emu,
                cur_addr,
                emu.arm9.engine_data.regs.is_in_priv_mode(),
            )) {
                // In case of a data abort, the instruction runs to completion before triggering
                // the exception (unclear what that means for timings, it's assumed all accesses
                // from that point on take 1 cycle).
                emu.arm9.engine_data.data_cycles = 1;
                add_cycles(
                    emu,
                    (instr as u8 & !((1 << reg) - 1)).count_ones() as RawTimestamp,
                );
                return handle_data_abort::<true>(emu, cur_addr);
            }
            let result = bus::read_32::<CpuAccess, _, false>(emu, cur_addr);
            emu.arm9.engine_data.data_cycles = access_cycles;
            reg!(emu.arm9, reg) = result;
            cur_addr = cur_addr.wrapping_add(4);
            if cur_addr & 0x3FC == 0 {
                timings = emu.arm9.cp15.timings.get(cur_addr);
                access_cycles = timings.r_n32_data;
            } else {
                access_cycles = timings.r_s32_data;
            }
            not_first = true;
        }
    }
    if instr as u8 & (instr as u8 - 1) == 0 {
        // Only one register present, add an internal cycle
        add_cycles(emu, emu.arm9.engine_data.data_cycles as RawTimestamp);
        emu.arm9.engine_data.data_cycles = 1;
    } else {
        let last_reg = (7 - (instr as u8).leading_zeros()) as u8;
        add_interlock(emu, last_reg, 1, 1);
    }
    if instr & 1 << base_reg == 0 {
        reg!(emu.arm9, base_reg) = cur_addr;
    }
}

pub fn stmia(emu: &mut Emu<Interpreter>, instr: u16) {
    add_bus_cycles(emu, 2);
    let base_reg = (instr >> 8 & 7) as u8;
    apply_reg_interlock_1::<false>(emu, base_reg);
    let mut cur_addr = reg!(emu.arm9, base_reg);
    prefetch_thumb::<false, true>(emu);
    if unlikely(instr as u8 == 0) {
        emu.arm9.engine_data.data_cycles = 1;
        add_cycles(emu, 1);
        reg!(emu.arm9, base_reg) = cur_addr.wrapping_add(0x40);
        return;
    }
    let mut not_first = false;
    let mut timings = emu.arm9.cp15.timings.get(cur_addr);
    let mut access_cycles = timings.w_n32_data;
    for reg in 0..8 {
        if instr & 1 << reg != 0 {
            if not_first {
                add_cycles(emu, emu.arm9.engine_data.data_cycles as RawTimestamp);
            } else {
                apply_reg_interlock_1::<true>(emu, reg);
            }
            if unlikely(!can_write(
                emu,
                cur_addr,
                emu.arm9.engine_data.regs.is_in_priv_mode(),
            )) {
                // In case of a data abort, the instruction runs to completion before triggering
                // the exception (unclear what that means for timings, it's assumed all accesses
                // from that point on take 1 cycle).
                emu.arm9.engine_data.data_cycles = 1;
                add_bus_cycles(emu, 2);
                add_cycles(
                    emu,
                    (instr as u8 & !((1 << reg) - 1)).count_ones() as RawTimestamp,
                );
                return handle_data_abort::<true>(emu, cur_addr);
            }
            bus::write_32::<CpuAccess, _>(emu, cur_addr, reg!(emu.arm9, reg));
            emu.arm9.engine_data.data_cycles = access_cycles;
            cur_addr = cur_addr.wrapping_add(4);
            if cur_addr & 0x3FC == 0 {
                timings = emu.arm9.cp15.timings.get(cur_addr);
                access_cycles = timings.w_n32_data;
            } else {
                access_cycles = timings.w_s32_data;
            }
            not_first = true;
        }
    }
    if instr as u8 & (instr as u8 - 1) == 0 {
        // Only one register present, add an internal cycle
        add_cycles(emu, emu.arm9.engine_data.data_cycles as RawTimestamp);
        emu.arm9.engine_data.data_cycles = 1;
    } else {
        let last_reg = (7 - (instr as u8).leading_zeros()) as u8;
        add_interlock(emu, last_reg, 1, 1);
    }
    reg!(emu.arm9, base_reg) = cur_addr;
}
