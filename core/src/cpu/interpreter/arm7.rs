mod arm;
mod thumb;

#[cfg(feature = "interp-pipeline")]
use super::common::{thumb_pipeline_entry, PipelineEntry};
use super::{super::Regs as EngineRegs, common::StateSource, Engine, Regs};
#[cfg(feature = "debug-hooks")]
use crate::cpu::debug;
use crate::{
    cpu::{
        arm7::{bus, Arm7, Schedule, Timestamp},
        bus::CpuAccess,
        psr::{Cpsr, Mode},
        Arm7Data, CoreData, Schedule as _,
    },
    emu::Emu,
    utils::{schedule::RawTimestamp, ByteSlice},
};
use cfg_if::cfg_if;

pub struct EngineData {
    #[cfg(feature = "interp-pipeline-accurate-reloads")]
    r15_increment: u32,
    pub regs: Regs,
    #[cfg(feature = "interp-pipeline")]
    pipeline: [PipelineEntry; 2],
    prefetch_nseq: bool,
    #[cfg(feature = "debug-hooks")]
    next_breakpoint_addr: u32,
}

impl EngineData {
    pub const fn new() -> Self {
        EngineData {
            #[cfg(feature = "interp-pipeline-accurate-reloads")]
            r15_increment: 4,
            regs: Regs::STARTUP,
            #[cfg(feature = "interp-pipeline")]
            pipeline: [0; 2],
            prefetch_nseq: false,
            #[cfg(feature = "debug-hooks")]
            next_breakpoint_addr: u32::MAX,
        }
    }
}

fn multiply_cycles(mut op: u32) -> RawTimestamp {
    op ^= ((op as i32) >> 31) as u32;
    4 - ((op | 1).leading_zeros() >> 3) as RawTimestamp
}

#[inline]
fn add_cycles(emu: &mut Emu<Engine>, cycles: RawTimestamp) {
    emu.arm7
        .schedule
        .set_cur_time(emu.arm7.schedule.cur_time() + Timestamp(cycles));
}

fn reload_pipeline<const STATE_SOURCE: StateSource>(emu: &mut Emu<Engine>) {
    let mut addr = reg!(emu.arm7, 15);
    if match STATE_SOURCE {
        StateSource::Arm => false,
        StateSource::Thumb => true,
        StateSource::R15Bit0 => {
            let thumb = addr & 1 != 0;
            emu.arm7.engine_data.regs.cpsr.set_thumb_state(thumb);
            #[cfg(feature = "interp-pipeline-accurate-reloads")]
            {
                emu.arm7.engine_data.r15_increment = 4 >> thumb as u8;
            }
            thumb
        }
        StateSource::Cpsr => emu.arm7.engine_data.regs.cpsr.thumb_state(),
    } {
        addr &= !1;
        #[cfg(feature = "debug-hooks")]
        if let Some(((branch_hook_fn, branch_hook_data), _)) = emu.arm7.branch_breakpoint_hooks() {
            emu.arm7.engine_data.next_breakpoint_addr =
                branch_hook_fn(addr, *branch_hook_data).unwrap_or(u32::MAX);
        }
        #[cfg(feature = "interp-pipeline")]
        {
            emu.arm7.engine_data.pipeline[0] =
                thumb_pipeline_entry(bus::read_16::<CpuAccess, _>(emu, addr) as PipelineEntry);
            let code_timings = emu.arm7.bus_timings.get(addr);
            add_cycles(emu, code_timings.n16 as RawTimestamp);
            addr = addr.wrapping_add(2);
            emu.arm7.engine_data.pipeline[1] =
                thumb_pipeline_entry(bus::read_16::<CpuAccess, _>(emu, addr) as PipelineEntry);
            add_cycles(
                emu,
                if addr & 0x3FE == 0 {
                    bus::timing_16::<_, false>(emu, addr)
                } else {
                    code_timings.s16
                } as RawTimestamp,
            );
            reg!(emu.arm7, 15) = addr.wrapping_add(2);
        }
        #[cfg(not(feature = "interp-pipeline"))]
        {
            let code_timings = emu.arm7.bus_timings.get(addr);
            add_cycles(
                emu,
                code_timings.n16 as RawTimestamp + code_timings.s16 as RawTimestamp,
            );
            reg!(emu.arm7, 15) = addr.wrapping_add(4);
        }
    } else {
        addr &= !3;
        #[cfg(feature = "debug-hooks")]
        if let Some(((branch_hook_fn, branch_hook_data), _)) = emu.arm7.branch_breakpoint_hooks() {
            emu.arm7.engine_data.next_breakpoint_addr =
                branch_hook_fn(addr, *branch_hook_data).unwrap_or(u32::MAX);
        }
        #[cfg(feature = "interp-pipeline")]
        {
            emu.arm7.engine_data.pipeline[0] =
                bus::read_32::<CpuAccess, _>(emu, addr) as PipelineEntry;
            let code_timings = emu.arm7.bus_timings.get(addr);
            add_cycles(emu, code_timings.n32 as RawTimestamp);
            addr = addr.wrapping_add(4);
            emu.arm7.engine_data.pipeline[1] =
                bus::read_32::<CpuAccess, _>(emu, addr) as PipelineEntry;
            add_cycles(
                emu,
                if addr & 0x3FC == 0 {
                    emu.arm7.bus_timings.get(addr).n32
                } else {
                    code_timings.s32
                } as RawTimestamp,
            );
            reg!(emu.arm7, 15) = addr.wrapping_add(4);
        }
        #[cfg(not(feature = "interp-pipeline"))]
        {
            let code_timings = emu.arm7.bus_timings.get(addr);
            add_cycles(
                emu,
                code_timings.n32 as RawTimestamp + code_timings.s32 as RawTimestamp,
            );
            reg!(emu.arm7, 15) = addr.wrapping_add(8);
        }
    }
}

fn set_cpsr_update_control(emu: &mut Emu<Engine>, value: Cpsr) {
    let old_value = emu.arm7.engine_data.regs.cpsr;
    emu.arm7.engine_data.regs.cpsr = value;
    emu.arm7
        .irqs
        .set_enabled_in_cpsr(!value.irqs_disabled(), &mut emu.arm7.schedule);
    emu.arm7
        .engine_data
        .regs
        .update_mode::<false>(old_value.mode(), value.mode());
}

fn restore_spsr(emu: &mut Emu<Engine>) {
    if !emu.arm7.engine_data.regs.is_in_exc_mode() {
        unimplemented!("Unpredictable SPSR restore in non-exception mode");
    }
    set_cpsr_update_control(emu, Cpsr::from_spsr(emu.arm7.engine_data.regs.spsr));
    #[cfg(feature = "interp-pipeline-accurate-reloads")]
    {
        emu.arm7.engine_data.r15_increment =
            4 >> emu.arm7.engine_data.regs.cpsr.thumb_state() as u8;
    }
}

fn handle_undefined<const THUMB: bool>(emu: &mut Emu<Engine>) {
    #[cfg(feature = "log")]
    slog::warn!(
        emu.arm7.logger,
        "Undefined instruction @ {:#010X} ({} state)",
        reg!(emu.arm7, 15).wrapping_sub(8 >> THUMB as u8),
        if THUMB { "Thumb" } else { "ARM" },
    );
    let old_cpsr = emu.arm7.engine_data.regs.cpsr;
    emu.arm7.engine_data.regs.cpsr = emu
        .arm7
        .engine_data
        .regs
        .cpsr
        .with_mode(Mode::Undefined)
        .with_thumb_state(false)
        .with_irqs_disabled(true);
    emu.arm7
        .irqs
        .set_enabled_in_cpsr(false, &mut emu.arm7.schedule);
    #[cfg(feature = "interp-pipeline-accurate-reloads")]
    {
        emu.arm7.engine_data.r15_increment = 4;
    }
    emu.arm7
        .engine_data
        .regs
        .update_mode::<false>(old_cpsr.mode(), Mode::Undefined);
    emu.arm7.engine_data.regs.spsr = old_cpsr.into();
    reg!(emu.arm7, 14) = reg!(emu.arm7, 15).wrapping_sub(4 >> THUMB as u8);
    reg!(emu.arm7, 15) = 0x0000_0004;
    reload_pipeline::<{ StateSource::Arm }>(emu);
}

fn handle_swi<const THUMB: bool>(
    emu: &mut Emu<Engine>,
    #[cfg(feature = "debug-hooks")] swi_num: u8,
) {
    #[cfg(feature = "debug-hooks")]
    if let Some(((swi_hook_fn, swi_hook_data), _)) = emu.arm7.swi_hook() {
        swi_hook_fn(swi_num, *swi_hook_data);
    }
    let old_cpsr = emu.arm7.engine_data.regs.cpsr;
    emu.arm7.engine_data.regs.cpsr = emu
        .arm7
        .engine_data
        .regs
        .cpsr
        .with_mode(Mode::Supervisor)
        .with_thumb_state(false)
        .with_irqs_disabled(true);
    emu.arm7
        .irqs
        .set_enabled_in_cpsr(false, &mut emu.arm7.schedule);
    #[cfg(feature = "interp-pipeline-accurate-reloads")]
    {
        emu.arm7.engine_data.r15_increment = 4;
    }
    emu.arm7
        .engine_data
        .regs
        .update_mode::<false>(old_cpsr.mode(), Mode::Supervisor);
    emu.arm7.engine_data.regs.spsr = old_cpsr.into();
    reg!(emu.arm7, 14) = reg!(emu.arm7, 15).wrapping_sub(4 >> THUMB as u8);
    reg!(emu.arm7, 15) = 0x0000_0008;
    reload_pipeline::<{ StateSource::Arm }>(emu);
}

impl CoreData for EngineData {
    type Engine = Engine;

    fn setup(emu: &mut Emu<Engine>) {
        reg!(emu.arm7, 15) = 0;
        reload_pipeline::<{ StateSource::Arm }>(emu);
    }

    fn setup_direct_boot(emu: &mut Emu<Engine>, entry_addr: u32, loaded_data: (ByteSlice, u32)) {
        for (&byte, addr) in loaded_data.0[..].iter().zip(loaded_data.1..) {
            bus::write_8::<CpuAccess, _>(emu, addr, byte);
        }
        let old_mode = emu.arm7.engine_data.regs.cpsr.mode();
        emu.arm7.engine_data.regs.cpsr.set_mode(Mode::System);
        emu.arm7
            .engine_data
            .regs
            .update_mode::<false>(old_mode, Mode::System);
        for reg in 0..12 {
            reg!(emu.arm7, reg) = 0;
        }
        reg!(emu.arm7, 12) = entry_addr;
        reg!(emu.arm7, 13) = 0x0380_FD80;
        reg!(emu.arm7, 14) = entry_addr;
        emu.arm7.engine_data.regs.r13_14_irq[0] = 0x0380_FF80;
        emu.arm7.engine_data.regs.r13_14_svc[0] = 0x0380_FFC0;
        emu.arm7.engine_data.prefetch_nseq = true;
        reg!(emu.arm7, 15) = entry_addr;
        reload_pipeline::<{ StateSource::R15Bit0 }>(emu);
    }

    #[inline]
    fn invalidate_word(&mut self, _addr: u32) {}

    #[inline]
    fn invalidate_word_range(&mut self, _bounds: (u32, u32)) {}

    #[inline]
    fn jump(emu: &mut Emu<Engine>, addr: u32) {
        reg!(emu.arm7, 15) = addr;
        reload_pipeline::<{ StateSource::R15Bit0 }>(emu);
    }

    #[inline]
    fn r15(&self) -> u32 {
        self.regs.cur[15]
    }

    #[inline]
    fn cpsr(&self) -> Cpsr {
        self.regs.cpsr
    }

    #[inline]
    fn regs(&self) -> EngineRegs {
        self.regs.to_engine_regs()
    }

    #[inline]
    fn set_regs(&mut self, regs: &EngineRegs) {
        self.regs.set_from_engine_regs(regs);
        todo!("Update registers externally");
    }

    cfg_if! {
        if #[cfg(feature = "debug-hooks")] {
            #[inline]
            fn set_branch_breakpoint_hooks(
                &mut self,
                value: &Option<(debug::BranchHook, debug::BreakpointHook, u32)>,
            ) {
                self.next_breakpoint_addr = value.map_or(u32::MAX, |v| v.2);
            }

            #[inline]
            fn set_swi_hook(&mut self, _value: &Option<debug::SwiHook>) {}

            #[inline]
            fn set_mem_watchpoint_hook(
                &mut self,
                _value: &Option<debug::MemWatchpointHook>,
            ) {
            }

            #[inline]
            fn add_mem_watchpoint(
                &mut self,
                _addr: u32,
                _rw: debug::MemWatchpointRwMask,
            ) {
            }

            #[inline]
            fn remove_mem_watchpoint(
                &mut self,
                _addr: u32,
                _rw: debug::MemWatchpointRwMask,
            ) {
            }
        }
    }
}

impl Arm7Data for EngineData {
    #[inline]
    fn run_until(emu: &mut Emu<Engine>, end_time: Timestamp) {
        while emu.arm7.schedule.cur_time() < end_time {
            Schedule::handle_pending_events(emu);
            emu.arm7
                .schedule
                .set_target_time(emu.arm7.schedule.schedule().next_event_time().min(end_time));
            if let Some(channel) = emu.arm7.dma.cur_channel() {
                Arm7::run_dma_transfer(emu, channel);
            } else {
                if emu.arm7.irqs.triggered() {
                    // Perform an extra instruction fetch before branching, like real hardware does,
                    // according to the ARM7TDMI reference manual
                    #[cfg(feature = "interp-pipeline")]
                    {
                        let fetch_addr = reg!(emu.arm7, 15);
                        let timings = emu.arm7.bus_timings.get(fetch_addr);
                        add_cycles(
                            emu,
                            if emu.arm7.engine_data.regs.cpsr.thumb_state() {
                                if fetch_addr & 0x3FF == 0 || emu.arm7.engine_data.prefetch_nseq {
                                    timings.n16
                                } else {
                                    timings.s16
                                }
                            } else if fetch_addr & 0x3FF == 0 || emu.arm7.engine_data.prefetch_nseq
                            {
                                timings.n32
                            } else {
                                timings.s32
                            } as RawTimestamp,
                        );
                    }
                    let old_cpsr = emu.arm7.engine_data.regs.cpsr;
                    emu.arm7.engine_data.regs.cpsr = emu
                        .arm7
                        .engine_data
                        .regs
                        .cpsr
                        .with_mode(Mode::Irq)
                        .with_thumb_state(false)
                        .with_irqs_disabled(true);
                    emu.arm7
                        .irqs
                        .set_enabled_in_cpsr(false, &mut emu.arm7.schedule);
                    #[cfg(feature = "interp-pipeline-accurate-reloads")]
                    {
                        emu.arm7.engine_data.r15_increment = 4;
                    }
                    emu.arm7
                        .engine_data
                        .regs
                        .update_mode::<false>(old_cpsr.mode(), Mode::Irq);
                    emu.arm7.engine_data.regs.spsr = old_cpsr.into();
                    reg!(emu.arm7, 14) =
                        reg!(emu.arm7, 15).wrapping_sub((!old_cpsr.thumb_state() as u32) << 2);
                    reg!(emu.arm7, 15) = 0x0000_0018;
                    reload_pipeline::<{ StateSource::Arm }>(emu);
                } else if emu.arm7.irqs.halted() {
                    emu.arm7
                        .schedule
                        .set_cur_time(emu.arm7.schedule.target_time());
                    continue;
                }
                while emu.arm7.schedule.cur_time() < emu.arm7.schedule.target_time() {
                    #[cfg(feature = "debug-hooks")]
                    {
                        let instr_addr = reg!(emu.arm7, 15)
                            .wrapping_sub(8 >> emu.arm7.engine_data.regs.cpsr.thumb_state() as u8);
                        if emu.arm7.engine_data.next_breakpoint_addr == instr_addr {
                            match emu.arm7.branch_breakpoint_hooks() {
                                Some((_, (breakpoint_hook, breakpoint_hook_data))) => {
                                    breakpoint_hook(instr_addr, *breakpoint_hook_data);
                                }
                                None => unsafe { core::hint::unreachable_unchecked() },
                            }
                            // TODO: Handle breakpoints somehow (would probably need to be able to
                            // pause the CPU while not in sync with the other, which complicates
                            // things)
                            todo!();
                        }
                    }
                    #[cfg(feature = "interp-pipeline")]
                    {
                        let addr = reg!(emu.arm7, 15);
                        let instr = emu.arm7.engine_data.pipeline[0];
                        emu.arm7.engine_data.pipeline[0] = emu.arm7.engine_data.pipeline[1];
                        if emu.arm7.engine_data.regs.cpsr.thumb_state() {
                            emu.arm7.engine_data.pipeline[1] = thumb_pipeline_entry(
                                bus::read_16::<CpuAccess, _>(emu, addr) as PipelineEntry,
                            );
                            let timings = emu.arm7.bus_timings.get(addr);
                            add_cycles(
                                emu,
                                if addr & 0x3FE == 0 || emu.arm7.engine_data.prefetch_nseq {
                                    timings.n16
                                } else {
                                    timings.s16
                                } as RawTimestamp,
                            );
                            #[cfg(not(feature = "interp-pipeline-accurate-reloads"))]
                            {
                                emu.arm7.engine_data.prefetch_nseq = false;
                                thumb::handle_instr(emu, instr as u16);
                            }
                        } else {
                            emu.arm7.engine_data.pipeline[1] =
                                bus::read_32::<CpuAccess, _>(emu, addr) as PipelineEntry;
                            let timings = emu.arm7.bus_timings.get(addr);
                            add_cycles(
                                emu,
                                if addr & 0x3FC == 0 || emu.arm7.engine_data.prefetch_nseq {
                                    timings.n32
                                } else {
                                    timings.s32
                                } as RawTimestamp,
                            );
                            #[cfg(not(feature = "interp-pipeline-accurate-reloads"))]
                            {
                                emu.arm7.engine_data.prefetch_nseq = false;
                                arm::handle_instr(emu, instr);
                            }
                        }
                        #[cfg(feature = "interp-pipeline-accurate-reloads")]
                        {
                            emu.arm7.engine_data.prefetch_nseq = false;
                            if instr & 1 << 32 == 0 {
                                arm::handle_instr(emu, instr as u32);
                            } else {
                                thumb::handle_instr(emu, instr as u16);
                            }
                        }
                    }
                    #[cfg(not(feature = "interp-pipeline"))]
                    if emu.arm7.engine_data.regs.cpsr.thumb_state() {
                        let addr = reg!(emu.arm7, 15).wrapping_sub(4);
                        let timings = emu.arm7.bus_timings.get(addr);
                        add_cycles(
                            emu,
                            timings.s16 as RawTimestamp
                                + if emu.arm7.engine_data.prefetch_nseq {
                                    timings.n16
                                } else {
                                    timings.s16
                                } as RawTimestamp,
                        );
                        let instr = bus::read_16::<CpuAccess, _>(emu, addr);
                        emu.arm7.engine_data.prefetch_nseq = false;
                        thumb::handle_instr(emu, instr);
                    } else {
                        let addr = reg!(emu.arm7, 15).wrapping_sub(8);
                        let timings = emu.arm7.bus_timings.get(addr);
                        add_cycles(
                            emu,
                            timings.s32 as RawTimestamp
                                + if emu.arm7.engine_data.prefetch_nseq {
                                    timings.n32
                                } else {
                                    timings.s32
                                } as RawTimestamp,
                        );
                        let instr = bus::read_32::<CpuAccess, _>(emu, addr);
                        emu.arm7.engine_data.prefetch_nseq = false;
                        arm::handle_instr(emu, instr);
                    };
                }
            }
        }
    }
}
