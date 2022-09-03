use super::super::{add_cycles, reload_pipeline, restore_spsr};
use crate::{
    cpu::{
        arm7::bus,
        bus::CpuAccess,
        interpreter::{
            alu_utils::shifts,
            common::{MiscAddressing, ShiftTy, StateSource, WbAddressing, WbOffTy},
            Interpreter,
        },
        psr::Mode,
    },
    emu::Emu,
    utils::schedule::RawTimestamp,
};
use core::intrinsics::unlikely;

macro_rules! wb_handler {
    (
        $ident: ident,
        |
            $emu: ident,
            $instr: ident,
            $addr: ident
            $(, src = $src_reg: ident)?
            $(, dst = $dst_reg: ident)?
        | $inner: block
    ) => {
        pub fn $ident<const OFF_TY: WbOffTy, const UPWARDS: bool, const ADDRESSING: WbAddressing>(
            $emu: &mut Emu<Interpreter>,
            $instr: u32,
        ) {
            let offset = {
                let abs_off = match OFF_TY {
                    WbOffTy::Imm => $instr & 0xFFF,
                    WbOffTy::Reg(shift_ty) => {
                        let value = reg!($emu.arm7, $instr & 0xF);
                        let shift = ($instr >> 7 & 0x1F) as u8;
                        match shift_ty {
                            ShiftTy::Lsl => shifts::lsl_imm(value, shift),
                            ShiftTy::Lsr => shifts::lsr_imm(value, shift),
                            ShiftTy::Asr => shifts::asr_imm(value, shift),
                            ShiftTy::Ror => shifts::ror_imm(
                                &$emu.arm7.engine_data.regs,
                                value,
                                shift,
                            ),
                        }
                    }
                } as i32;
                if UPWARDS {
                    abs_off
                } else {
                    abs_off.wrapping_neg()
                }
            };

            let base_reg = $instr >> 16 & 0xF;
            let base = reg!($emu.arm7, base_reg);
            let $addr = if ADDRESSING.preincrement() {
                base.wrapping_add(offset as u32)
            } else {
                base
            };
            inc_r15!($emu.arm7, 4);
            $(
                { let $dst_reg = 0; let _ = $dst_reg; }
                if ADDRESSING.writeback() {
                    #[cfg(feature = "interp-r15-write-checks")]
                    if unlikely(base_reg == 15) {
                        unimplemented!(concat!(stringify!($ident), " r15 writeback"));
                    }
                    reg!($emu.arm7, base_reg) = if ADDRESSING.preincrement() {
                        $addr
                    } else {
                        $addr.wrapping_add(offset as u32)
                    };
                }
            )*

            $( let $src_reg = $instr >> 12 & 0xF; )*
            $( let $dst_reg = $instr >> 12 & 0xF; )*

            $inner

            $(
                let _ = $src_reg;
                if ADDRESSING.writeback() {
                    #[cfg(feature = "interp-r15-write-checks")]
                    if unlikely(base_reg == 15) {
                        unimplemented!(concat!(stringify!($ident), " r15 writeback"));
                    }
                    reg!($emu.arm7, base_reg) = if ADDRESSING.preincrement() {
                        $addr
                    } else {
                        $addr.wrapping_add(offset as u32)
                    };
                }
            )*
        }
    }
}

wb_handler! {
    ldr,
    |emu, instr, addr, dst = dst_reg| {
        let result = bus::read_32::<CpuAccess, _>(emu, addr).rotate_right((addr & 3) << 3);
        let cycles = emu.arm7.bus_timings.get(addr).n32;
        add_cycles(emu, cycles as RawTimestamp + 1);
        emu.arm7.engine_data.prefetch_nseq = true;
        reg!(emu.arm7, dst_reg) = result;
        if dst_reg == 15 {
            reload_pipeline::<{ StateSource::Arm }>(emu);
        }
    }
}

wb_handler! {
    str,
    |emu, instr, addr, src = src_reg| {
        bus::write_32::<CpuAccess, _>(emu, addr, reg!(emu.arm7, src_reg));
        let cycles = emu.arm7.bus_timings.get(addr).n32;
        add_cycles(emu, cycles as RawTimestamp);
        emu.arm7.engine_data.prefetch_nseq = true;
    }
}

wb_handler! {
    ldrb,
    |emu, instr, addr, dst = dst_reg| {
        let result = bus::read_8::<CpuAccess, _>(emu, addr) as u32;
        let cycles = emu.arm7.bus_timings.get(addr).n16;
        add_cycles(emu, cycles as RawTimestamp + 1);
        emu.arm7.engine_data.prefetch_nseq = true;
        reg!(emu.arm7, dst_reg) = result;
        if dst_reg == 15 {
            reload_pipeline::<{ StateSource::Arm }>(emu);
        }
    }
}

wb_handler! {
    strb,
    |emu, instr, addr, src = src_reg| {
        bus::write_8::<CpuAccess, _>(emu, addr, reg!(emu.arm7, src_reg) as u8);
        let cycles = emu.arm7.bus_timings.get(addr).n16;
        add_cycles(emu, cycles as RawTimestamp);
        emu.arm7.engine_data.prefetch_nseq = true;
    }
}

macro_rules! misc_handler {
    (
        $ident: ident,
        |
            $emu: ident,
            $instr: ident,
            $addr: ident
            $(, src = $src_reg: ident)?
            $(, dst = $dst_reg: ident)?
        | $inner: block
    ) => {
        pub fn $ident<const OFF_IMM: bool, const UPWARDS: bool, const ADDRESSING: MiscAddressing>(
            $emu: &mut Emu<Interpreter>,
            $instr: u32,
        ) {
            let offset = {
                let abs_off = if OFF_IMM {
                    ($instr & 0xF) | ($instr >> 4 & 0xF0)
                } else {
                    reg!($emu.arm7, $instr & 0xF)
                } as i32;
                if UPWARDS {
                    abs_off
                } else {
                    abs_off.wrapping_neg()
                }
            };
            let base_reg = $instr >> 16 & 0xF;
            let base = reg!($emu.arm7, base_reg);
            let $addr = if ADDRESSING.preincrement() {
                base.wrapping_add(offset as u32)
            } else {
                base
            };
            inc_r15!($emu.arm7, 4);
            $(
                { let $dst_reg = 0; let _ = $dst_reg; }
                if ADDRESSING.writeback() {
                    #[cfg(feature = "interp-r15-write-checks")]
                    if unlikely(base_reg == 15) {
                        unimplemented!(concat!(stringify!($ident), " r15 writeback"));
                    }
                    reg!($emu.arm7, base_reg) = if ADDRESSING.preincrement() {
                        $addr
                    } else {
                        $addr.wrapping_add(offset as u32)
                    };
                }
            )*

            $( let $src_reg = $instr >> 12 & 0xF; )*
            $( let $dst_reg = $instr >> 12 & 0xF; )*

            $inner

            $(
                let _ = $src_reg;
                if ADDRESSING.writeback() {
                    #[cfg(feature = "interp-r15-write-checks")]
                    if unlikely(base_reg == 15) {
                        unimplemented!(concat!(stringify!($ident), " r15 writeback"));
                    }
                    reg!($emu.arm7, base_reg) = if ADDRESSING.preincrement() {
                        $addr
                    } else {
                        $addr.wrapping_add(offset as u32)
                    };
                }
            )*
        }
    }
}

misc_handler! {
    ldrh,
    |emu, instr, addr, dst = dst_reg| {
        let result = (bus::read_16::<CpuAccess, _>(emu, addr) as u32).rotate_right((addr & 1) << 3);
        let cycles = emu.arm7.bus_timings.get(addr).n16;
        add_cycles(emu, cycles as RawTimestamp + 1);
        emu.arm7.engine_data.prefetch_nseq = true;
        reg!(emu.arm7, dst_reg) = result;
        if dst_reg == 15 {
            reload_pipeline::<{ StateSource::Arm }>(emu);
        }
    }
}

misc_handler! {
    strh,
    |emu, instr, addr, src = src_reg| {
        bus::write_16::<CpuAccess, _>(
            emu,
            addr,
            reg!(emu.arm7, src_reg) as u16,
        );
        let cycles = emu.arm7.bus_timings.get(addr).n16;
        add_cycles(emu, cycles as RawTimestamp);
        emu.arm7.engine_data.prefetch_nseq = true;
    }
}

// TODO: Check LDRD/STRD timings

misc_handler! {
    ldrd,
    |emu, instr, addr, dst = _dst_reg| {
        add_cycles(emu, 2);
        emu.arm7.engine_data.prefetch_nseq = true;
    }
}

misc_handler! {
    strd,
    |emu, instr, add, src = _src_reg| {
        add_cycles(emu, 1);
        emu.arm7.engine_data.prefetch_nseq = true;
    }
}

misc_handler! {
    ldrsb,
    |emu, instr, addr, dst = dst_reg| {
        let result = bus::read_8::<CpuAccess, _>(emu, addr) as i8 as u32;
        let cycles = emu.arm7.bus_timings.get(addr).n16;
        add_cycles(emu, cycles as RawTimestamp + 1);
        emu.arm7.engine_data.prefetch_nseq = true;
        reg!(emu.arm7, dst_reg) = result;
        if dst_reg == 15 {
            reload_pipeline::<{ StateSource::Arm }>(emu);
        }
    }
}

misc_handler! {
    ldrsh,
    |emu, instr, addr, dst = dst_reg| {
        let result = {
            let aligned = bus::read_16::<CpuAccess, _>(emu, addr);
            ((aligned as i32) << 16 >> (((addr & 1) | 2) << 3)) as u32
        };
        let cycles = emu.arm7.bus_timings.get(addr).n16;
        add_cycles(emu, cycles as RawTimestamp + 1);
        emu.arm7.engine_data.prefetch_nseq = true;
        reg!(emu.arm7, dst_reg) = result;
        if dst_reg == 15 {
            reload_pipeline::<{ StateSource::Arm }>(emu);
        }
    }
}

pub fn swp(emu: &mut Emu<Interpreter>, instr: u32) {
    let addr = reg!(emu.arm7, instr >> 16 & 0xF);
    inc_r15!(emu.arm7, 4);
    let access_timings = emu.arm7.bus_timings.get(addr).n32 as RawTimestamp;
    let loaded_value = bus::read_32::<CpuAccess, _>(emu, addr).rotate_right((addr & 3) << 3);
    add_cycles(emu, access_timings);
    bus::write_32::<CpuAccess, _>(emu, addr, reg!(emu.arm7, instr & 0xF));
    add_cycles(emu, access_timings + 1);
    emu.arm7.engine_data.prefetch_nseq = true;
    let dst_reg = instr >> 12 & 0xF;
    #[cfg(feature = "interp-r15-write-checks")]
    if unlikely(dst_reg == 15) {
        unimplemented!("swp r15 write");
    }
    reg!(emu.arm7, dst_reg) = loaded_value;
}

pub fn swpb(emu: &mut Emu<Interpreter>, instr: u32) {
    let addr = reg!(emu.arm7, instr >> 16 & 0xF);
    inc_r15!(emu.arm7, 4);
    let access_timings = emu.arm7.bus_timings.get(addr).n16 as RawTimestamp;
    let loaded_value = bus::read_8::<CpuAccess, _>(emu, addr) as u32;
    add_cycles(emu, access_timings);
    bus::write_8::<CpuAccess, _>(emu, addr, reg!(emu.arm7, instr & 0xF) as u8);
    add_cycles(emu, access_timings + 1);
    emu.arm7.engine_data.prefetch_nseq = true;
    let dst_reg = instr >> 12 & 0xF;
    #[cfg(feature = "interp-r15-write-checks")]
    if unlikely(dst_reg == 15) {
        unimplemented!("swpb r15 write");
    }
    reg!(emu.arm7, dst_reg) = loaded_value;
}

// TODO: Check timing with empty reg lists.
// TODO: Check what happens if both the S (bank switch, when not loading r15) and W (writeback) bits
//       are set at the same time (right now, the wrong register is updated).
// TODO: Check how bank switching interacts with timing.

pub fn ldm<const UPWARDS: bool, const PREINC: bool, const WRITEBACK: bool, const S_BIT: bool>(
    emu: &mut Emu<Interpreter>,
    instr: u32,
) {
    let base_reg = instr >> 16 & 0xF;
    #[cfg(feature = "interp-r15-write-checks")]
    if unlikely(base_reg == 15 && WRITEBACK) {
        unimplemented!("ldm r15 writeback");
    }

    if unlikely(instr as u16 == 0) {
        let start_addr = if UPWARDS {
            reg!(emu.arm7, base_reg)
        } else {
            reg!(emu.arm7, base_reg).wrapping_sub(0x40)
        };
        if WRITEBACK {
            reg!(emu.arm7, base_reg) = if UPWARDS {
                start_addr.wrapping_add(0x40)
            } else {
                start_addr
            };
        }
        let addr = if PREINC {
            start_addr.wrapping_add(4)
        } else {
            start_addr
        };
        let result = bus::read_32::<CpuAccess, _>(emu, addr);
        let cycles = emu.arm7.bus_timings.get(addr).n32;
        add_cycles(emu, cycles as RawTimestamp + 1);
        reg!(emu.arm7, 15) = result;
        return if S_BIT {
            restore_spsr(emu);
            reload_pipeline::<{ StateSource::Cpsr }>(emu);
        } else {
            reload_pipeline::<{ StateSource::Arm }>(emu);
        };
    }

    let mut cur_addr = if UPWARDS {
        reg!(emu.arm7, base_reg)
    } else {
        reg!(emu.arm7, base_reg).wrapping_sub((instr as u16).count_ones() << 2)
    };
    if WRITEBACK {
        reg!(emu.arm7, base_reg) = if UPWARDS {
            cur_addr.wrapping_add((instr as u16).count_ones() << 2)
        } else {
            cur_addr
        };
    }
    inc_r15!(emu.arm7, 4);
    if S_BIT && instr & 1 << 15 == 0 {
        emu.arm7
            .engine_data
            .regs
            .update_mode::<true>(emu.arm7.engine_data.regs.cpsr.mode(), Mode::USER);
    }
    if PREINC {
        cur_addr = cur_addr.wrapping_add(4);
    }
    #[allow(unused_mut)]
    let mut timings = emu.arm7.bus_timings.get(cur_addr);
    let mut access_cycles = timings.n32;
    for reg in 0..15 {
        if instr & 1 << reg != 0 {
            let result = bus::read_32::<CpuAccess, _>(emu, cur_addr);
            reg!(emu.arm7, reg) = result;
            add_cycles(emu, access_cycles as RawTimestamp);
            cur_addr = cur_addr.wrapping_add(4);
            #[cfg(feature = "interp-timing-details")]
            if cur_addr & 0x3FC == 0 {
                timings = emu.arm7.bus_timings.get(cur_addr);
                access_cycles = timings.n32;
            } else {
                access_cycles = timings.s32;
            }
            #[cfg(not(feature = "interp-timing-details"))]
            {
                access_cycles = timings.s32;
            }
        }
    }
    if instr & 1 << 15 == 0 {
        if S_BIT {
            emu.arm7
                .engine_data
                .regs
                .update_mode::<true>(Mode::USER, emu.arm7.engine_data.regs.cpsr.mode());
        }
        add_cycles(emu, 1);
        emu.arm7.engine_data.prefetch_nseq = true;
    } else {
        let result = bus::read_32::<CpuAccess, _>(emu, cur_addr);
        add_cycles(emu, access_cycles as RawTimestamp + 1);
        reg!(emu.arm7, 15) = result;
        if S_BIT {
            restore_spsr(emu);
            reload_pipeline::<{ StateSource::Cpsr }>(emu);
        } else {
            reload_pipeline::<{ StateSource::Arm }>(emu);
        }
    }
}

pub fn stm<const UPWARDS: bool, const PREINC: bool, const WRITEBACK: bool, const S_BIT: bool>(
    emu: &mut Emu<Interpreter>,
    instr: u32,
) {
    let base_reg = instr >> 16 & 0xF;
    #[cfg(feature = "interp-r15-write-checks")]
    if unlikely(base_reg == 15 && WRITEBACK) {
        unimplemented!("stm r15 writeback");
    }

    emu.arm7.engine_data.prefetch_nseq = true;

    if unlikely(instr as u16 == 0) {
        let start_addr = if UPWARDS {
            reg!(emu.arm7, base_reg)
        } else {
            reg!(emu.arm7, base_reg).wrapping_sub(0x40)
        };
        if WRITEBACK {
            reg!(emu.arm7, base_reg) = if UPWARDS {
                start_addr.wrapping_add(0x40)
            } else {
                start_addr
            };
        }
        let addr = if PREINC {
            start_addr.wrapping_add(4)
        } else {
            start_addr
        };
        inc_r15!(emu.arm7, 4);
        bus::write_32::<CpuAccess, _>(emu, addr, reg!(emu.arm7, 15));
        let cycles = emu.arm7.bus_timings.get(addr).n32;
        add_cycles(emu, cycles as RawTimestamp);
        return;
    }

    let mut cur_addr = if UPWARDS {
        reg!(emu.arm7, base_reg)
    } else {
        reg!(emu.arm7, base_reg).wrapping_sub((instr as u16).count_ones() << 2)
    };
    let end_addr = if UPWARDS {
        cur_addr.wrapping_add((instr as u16).count_ones() << 2)
    } else {
        cur_addr
    };
    inc_r15!(emu.arm7, 4);
    if S_BIT {
        emu.arm7
            .engine_data
            .regs
            .update_mode::<true>(emu.arm7.engine_data.regs.cpsr.mode(), Mode::USER);
    }
    if PREINC {
        cur_addr = cur_addr.wrapping_add(4);
    }
    #[allow(unused_mut)]
    let mut timings = emu.arm7.bus_timings.get(cur_addr);
    let mut access_cycles = timings.n32;
    for reg in 0..16 {
        if instr & 1 << reg != 0 {
            bus::write_32::<CpuAccess, _>(emu, cur_addr, reg!(emu.arm7, reg));
            add_cycles(emu, access_cycles as RawTimestamp);
            cur_addr = cur_addr.wrapping_add(4);
            #[cfg(feature = "interp-timing-details")]
            if cur_addr & 0x3FC == 0 {
                timings = emu.arm7.bus_timings.get(cur_addr);
                access_cycles = timings.n32;
            } else {
                access_cycles = timings.s32;
            }
            #[cfg(not(feature = "interp-timing-details"))]
            {
                access_cycles = timings.s32;
            }
            if WRITEBACK {
                reg!(emu.arm7, base_reg) = end_addr;
            }
        }
    }
    if S_BIT {
        emu.arm7
            .engine_data
            .regs
            .update_mode::<true>(Mode::USER, emu.arm7.engine_data.regs.cpsr.mode());
    }
}
