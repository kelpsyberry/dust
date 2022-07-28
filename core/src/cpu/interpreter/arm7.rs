mod arm;
mod thumb;

#[cfg(feature = "interp-pipeline")]
use super::common::{thumb_pipeline_entry, PipelineEntry};
use super::{super::Regs as EngineRegs, common::StateSource, Interpreter, Regs};
#[cfg(feature = "debugger-hooks")]
use crate::cpu::debug;
use crate::{
    cpu::{
        arm7::{bus, Arm7, Schedule, Timestamp},
        bus::CpuAccess,
        hle_bios,
        psr::{Cpsr, Mode, Spsr},
        Arm7Data, CoreData, Schedule as _,
    },
    emu::Emu,
    utils::{schedule::RawTimestamp, Savestate},
};

#[derive(Savestate)]
#[load(in_place_only)]
pub struct EngineData {
    #[cfg(feature = "interp-pipeline-accurate-reloads")]
    r15_increment: u32,
    pub regs: Regs,
    #[cfg(feature = "interp-pipeline")]
    pipeline: [PipelineEntry; 2],
    prefetch_nseq: bool,
    #[cfg(feature = "debugger-hooks")]
    #[savestate(skip)]
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
            #[cfg(feature = "debugger-hooks")]
            next_breakpoint_addr: u32::MAX,
        }
    }
}

macro_rules! get_next_breakpoint {
    ($emu: expr, $addr: expr, $mask: expr) => {
        #[cfg(feature = "debugger-hooks")]
        if !$emu.arm7.debug.breakpoints.is_empty() {
            let (Ok(i) | Err(i)) = $emu.arm7.debug.breakpoints.binary_search(&$addr);
            $emu.arm7.engine_data.next_breakpoint_addr =
                $emu.arm7.debug.breakpoints[if i >= $emu.arm7.debug.breakpoints.len() {
                    0
                } else {
                    i
                }] & !$mask;
        }
    };
}

fn multiply_cycles(mut op: u32) -> RawTimestamp {
    op ^= ((op as i32) >> 31) as u32;
    4 - ((op | 1).leading_zeros() >> 3) as RawTimestamp
}

#[inline]
fn add_cycles(emu: &mut Emu<Interpreter>, cycles: RawTimestamp) {
    emu.arm7
        .schedule
        .set_cur_time(emu.arm7.schedule.cur_time() + Timestamp(cycles));
}

fn reload_pipeline<const STATE_SOURCE: StateSource>(emu: &mut Emu<Interpreter>) {
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
        get_next_breakpoint!(emu, addr, 1);
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
                    emu.arm7.bus_timings.get(addr).n16
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
        get_next_breakpoint!(emu, addr, 3);
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

fn set_cpsr_update_control(emu: &mut Emu<Interpreter>, value: Cpsr) {
    let prev_value = emu.arm7.engine_data.regs.cpsr;
    emu.arm7.engine_data.regs.cpsr = value;
    emu.arm7
        .irqs
        .set_enabled_in_cpsr(!value.irqs_disabled(), &mut emu.arm7.schedule);
    emu.arm7
        .engine_data
        .regs
        .update_mode::<false>(prev_value.mode(), value.mode());
}

fn restore_spsr(emu: &mut Emu<Interpreter>) {
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

fn handle_undefined<const THUMB: bool>(emu: &mut Emu<Interpreter>) {
    #[cfg(feature = "log")]
    slog::warn!(
        emu.arm7.logger,
        "Undefined instruction @ {:#010X} ({} state)",
        reg!(emu.arm7, 15).wrapping_sub(8 >> THUMB as u8),
        if THUMB { "Thumb" } else { "ARM" },
    );
    #[cfg(feature = "debugger-hooks")]
    if let Some(undef_hook) = emu.arm7.undef_hook() {
        if unsafe { undef_hook.get()(emu) } {
            emu.arm7
                .schedule
                .set_target_time(emu.arm7.schedule.cur_time());
            emu.arm7.stopped_by_debug_hook = true;
        }
    }
    let prev_cpsr = emu.arm7.engine_data.regs.cpsr;
    emu.arm7.engine_data.regs.cpsr = emu
        .arm7
        .engine_data
        .regs
        .cpsr
        .with_mode(Mode::Undefined)
        .with_thumb_state(false)
        .with_irqs_disabled(true);
    emu.arm7.irqs.set_enabled_in_cpsr(false, ());
    #[cfg(feature = "interp-pipeline-accurate-reloads")]
    {
        emu.arm7.engine_data.r15_increment = 4;
    }
    emu.arm7
        .engine_data
        .regs
        .update_mode::<false>(prev_cpsr.mode(), Mode::Undefined);
    emu.arm7.engine_data.regs.spsr = prev_cpsr.into();
    reg!(emu.arm7, 14) = reg!(emu.arm7, 15).wrapping_sub(4 >> THUMB as u8);
    reg!(emu.arm7, 15) = 0x0000_0004;
    reload_pipeline::<{ StateSource::Arm }>(emu);
}

fn handle_swi<const THUMB: bool>(emu: &mut Emu<Interpreter>, number: u8) {
    #[cfg(feature = "debugger-hooks")]
    if let Some(swi_hook) = emu.arm7.swi_hook() {
        if unsafe { swi_hook.get()(emu, number) } {
            emu.arm7
                .schedule
                .set_target_time(emu.arm7.schedule.cur_time());
            emu.arm7.stopped_by_debug_hook = true;
        }
    }

    if emu.arm7.hle_bios_enabled() {
        enter_hle_swi::<true>(
            emu,
            number,
            reg!(emu.arm7, 15).wrapping_sub(4 >> THUMB as u8),
        );
        return;
    }

    let prev_cpsr = emu.arm7.engine_data.regs.cpsr;
    emu.arm7.engine_data.regs.cpsr = prev_cpsr
        .with_mode(Mode::Supervisor)
        .with_thumb_state(false)
        .with_irqs_disabled(true);
    emu.arm7.irqs.set_enabled_in_cpsr(false, ());
    #[cfg(feature = "interp-pipeline-accurate-reloads")]
    {
        emu.arm7.engine_data.r15_increment = 4;
    }
    emu.arm7
        .engine_data
        .regs
        .update_mode::<false>(prev_cpsr.mode(), Mode::Supervisor);
    emu.arm7.engine_data.regs.spsr = prev_cpsr.into();
    reg!(emu.arm7, 14) = reg!(emu.arm7, 15).wrapping_sub(4 >> THUMB as u8);
    reg!(emu.arm7, 15) = 0x0000_0008;
    reload_pipeline::<{ StateSource::Arm }>(emu);
}

fn enter_hle_swi<const FROM_USER_CODE: bool>(
    emu: &mut Emu<Interpreter>,
    number: u8,
    return_addr: u32,
) {
    let spsr = if FROM_USER_CODE {
        emu.arm7.engine_data.regs.cpsr.into()
    } else {
        spsr!(emu.arm7)
    };

    if FROM_USER_CODE {
        let prev_mode = emu.arm7.engine_data.regs.cpsr.mode();
        emu.arm7.engine_data.regs.cpsr.set_mode(Mode::Supervisor);
        emu.arm7
            .engine_data
            .regs
            .update_mode::<false>(prev_mode, Mode::Supervisor);
        reg!(emu.arm7, 14) = return_addr;
        emu.arm7.engine_data.regs.spsr = spsr;
    }

    reg!(emu.arm7, 13) = reg!(emu.arm7, 13).wrapping_sub(0x10);
    let base_addr = reg!(emu.arm7, 13);
    for (i, value) in [
        spsr.raw(),
        reg!(emu.arm7, 11),
        reg!(emu.arm7, 12),
        reg!(emu.arm7, 14),
    ]
    .into_iter()
    .enumerate()
    {
        bus::write_32::<CpuAccess, _>(emu, base_addr.wrapping_add((i << 2) as u32), value);
    }

    let new_cpsr = Cpsr::from_raw::<false>((spsr.raw() & 0x80) | 0x1F);
    reg!(emu.arm7, 11) = new_cpsr.raw();
    set_cpsr_update_control(emu, new_cpsr);

    let base_addr = reg!(emu.arm7, 13).wrapping_sub(8);
    reg!(emu.arm7, 13) = base_addr;
    for (i, reg) in [2_u8, 14].into_iter().enumerate() {
        bus::write_32::<CpuAccess, _>(
            emu,
            base_addr.wrapping_add((i << 2) as u32),
            reg!(emu.arm7, reg),
        );
    }

    hle_bios::arm7::handle_swi(emu, number, emu.arm7.engine_data.regs.r0_3());
}

fn return_from_hle_swi(emu: &mut Emu<Interpreter>) {
    let base_addr = reg!(emu.arm7, 13);
    reg!(emu.arm7, 13) = base_addr.wrapping_add(8);
    for (i, reg) in [2_u8, 14].into_iter().enumerate() {
        reg!(emu.arm7, reg) =
            bus::read_32::<CpuAccess, _>(emu, base_addr.wrapping_add((i << 2) as u32));
    }

    let prev_cpsr = emu.arm7.engine_data.regs.cpsr;
    emu.arm7.engine_data.regs.cpsr = prev_cpsr.with_mode(Mode::Supervisor);
    emu.arm7
        .engine_data
        .regs
        .update_mode::<false>(prev_cpsr.mode(), Mode::Supervisor);

    let base_addr = reg!(emu.arm7, 13);
    reg!(emu.arm7, 13) = base_addr.wrapping_add(0x10);
    emu.arm7.engine_data.regs.spsr =
        Spsr::from_raw::<false>(bus::read_32::<CpuAccess, _>(emu, base_addr));
    for (i, reg) in [11_u8, 12, 14].into_iter().enumerate() {
        reg!(emu.arm7, reg) =
            bus::read_32::<CpuAccess, _>(emu, base_addr.wrapping_add(((i + 1) << 2) as u32));
    }

    reg!(emu.arm7, 15) = reg!(emu.arm7, 14);
    restore_spsr(emu);
    reload_pipeline::<{ StateSource::Cpsr }>(emu);
}

fn enter_hle_irq<const FROM_USER_CODE: bool>(emu: &mut Emu<Interpreter>, return_addr: u32) {
    if FROM_USER_CODE {
        let prev_cpsr = emu.arm7.engine_data.regs.cpsr;
        emu.arm7.engine_data.regs.cpsr = prev_cpsr.with_mode(Mode::Irq).with_irqs_disabled(true);
        emu.arm7.irqs.set_enabled_in_cpsr(false, ());
        emu.arm7
            .engine_data
            .regs
            .update_mode::<false>(prev_cpsr.mode(), Mode::Irq);
        emu.arm7.engine_data.regs.spsr = prev_cpsr.into();
    }

    let base_addr = reg!(emu.arm7, 13).wrapping_sub(0x18);
    reg!(emu.arm7, 13) = base_addr;
    for (i, reg) in [0_u8, 1, 2, 3, 12].into_iter().enumerate() {
        bus::write_32::<CpuAccess, _>(
            emu,
            base_addr.wrapping_add((i << 2) as u32),
            reg!(emu.arm7, reg),
        );
    }
    bus::write_32::<CpuAccess, _>(emu, base_addr.wrapping_add(0x14), return_addr);

    reg!(emu.arm7, 0) = hle_bios::arm7::handle_irq(emu);
}

fn return_from_hle_irq(emu: &mut Emu<Interpreter>) {
    let base_addr = reg!(emu.arm7, 13);
    reg!(emu.arm7, 13) = base_addr.wrapping_add(0x18);
    for (i, reg) in [0_u8, 1, 2, 3, 12, 14].into_iter().enumerate() {
        reg!(emu.arm7, reg) =
            bus::read_32::<CpuAccess, _>(emu, base_addr.wrapping_add((i << 2) as u32));
    }
    reg!(emu.arm7, 15) = reg!(emu.arm7, 14).wrapping_sub(4);
    restore_spsr(emu);
    reload_pipeline::<{ StateSource::Cpsr }>(emu);
}

impl CoreData for EngineData {
    type Engine = Interpreter;

    fn setup(emu: &mut Emu<Interpreter>) {
        reg!(emu.arm7, 15) = 0;
        reload_pipeline::<{ StateSource::Arm }>(emu);
    }

    fn setup_direct_boot(emu: &mut Emu<Interpreter>, entry_addr: u32) {
        let prev_mode = emu.arm7.engine_data.regs.cpsr.mode();
        emu.arm7.engine_data.regs.cpsr.set_mode(Mode::System);
        emu.arm7
            .engine_data
            .regs
            .update_mode::<false>(prev_mode, Mode::System);
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
    fn post_load(_emu: &mut Emu<Interpreter>) {
        get_next_breakpoint!(
            _emu,
            reg!(_emu.arm7, 15),
            1 | (!_emu.arm7.engine_data.regs.cpsr.thumb_state() as u32) << 1
        );
    }

    #[inline]
    fn invalidate_word(&mut self, _addr: u32) {}

    #[inline]
    fn invalidate_word_range(&mut self, _bounds: (u32, u32)) {}

    #[inline]
    fn jump(emu: &mut Emu<Interpreter>, addr: u32) {
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
    fn set_cpsr(emu: &mut Emu<Interpreter>, value: Cpsr) {
        set_cpsr_update_control(emu, value);
    }

    #[inline]
    fn regs(&self) -> EngineRegs {
        self.regs.to_engine_regs()
    }

    #[inline]
    fn set_regs(emu: &mut Emu<Interpreter>, regs: &EngineRegs) {
        emu.arm7.engine_data.regs.set_from_engine_regs(regs);
        reg!(emu.arm7, 15) = reg!(emu.arm7, 15)
            .wrapping_sub(8 >> emu.arm7.engine_data.regs.cpsr.thumb_state() as u8);
        reload_pipeline::<{ StateSource::Cpsr }>(emu);
    }

    #[inline]
    fn jump_and_link(emu: &mut Emu<Interpreter>, addr: u32, lr: u32) {
        reg!(emu.arm7, 14) = lr;
        reg!(emu.arm7, 15) = addr;
        reload_pipeline::<{ StateSource::R15Bit0 }>(emu);
    }

    #[inline]
    fn return_from_hle_swi(emu: &mut Emu<Self::Engine>, r0_3: [u32; 4]) {
        emu.arm7.engine_data.regs.set_r0_3(r0_3);
        return_from_hle_swi(emu);
    }

    cfg_if::cfg_if! {
        if #[cfg(feature = "debugger-hooks")] {
            #[inline]
            fn set_swi_hook(&mut self, _hook: &Option<debug::SwiHook<Interpreter>>) {}

            #[inline]
            fn set_undef_hook(&mut self, _hook: &Option<debug::UndefHook<Interpreter>>) {}

            #[inline]
            fn add_breakpoint(&mut self, addr: u32) {
                if addr < self.next_breakpoint_addr
                    && addr
                        >= self.regs.cur[15].wrapping_sub(8 >> self.regs.cpsr.thumb_state() as u8)
                {
                    self.next_breakpoint_addr = addr;
                }
            }

            #[inline]
            fn remove_breakpoint(&mut self, addr: u32, i: usize, breakpoints: &[u32]) {
                if self.next_breakpoint_addr == addr {
                    self.next_breakpoint_addr = if breakpoints.is_empty() {
                        u32::MAX
                    } else {
                        breakpoints[if i == breakpoints.len() { 0 } else { i }]
                    };
                }
            }

            #[inline]
            fn clear_breakpoints(&mut self) {
                self.next_breakpoint_addr = u32::MAX;
            }

            #[inline]
            fn set_breakpoint_hook(&mut self, _hook: &Option<debug::BreakpointHook<Interpreter>>) {}

            #[inline]
            fn set_mem_watchpoint_hook(
                &mut self,
                _hook: &Option<debug::MemWatchpointHook<Interpreter>>,
            ) {}

            #[inline]
            fn add_mem_watchpoint(
                &mut self,
                _addr: u32,
                _size: u8,
                _rw: debug::MemWatchpointRwMask,
            ) {}

            #[inline]
            fn remove_mem_watchpoint(
                &mut self,
                _addr: u32,
                _size: u8,
                _rw: debug::MemWatchpointRwMask,
            ) {}

            #[inline]
            fn clear_mem_watchpoints(&mut self) {}
        }
    }
}

impl Arm7Data for EngineData {
    #[inline]
    fn run_stalled_until(emu: &mut Emu<Interpreter>, end_time: Timestamp) {
        emu.arm7.schedule.set_cur_time(end_time);
        Schedule::handle_pending_events(emu);
    }

    #[inline]
    fn run_until(emu: &mut Emu<Interpreter>, end_time: Timestamp) {
        while emu.arm7.schedule.cur_time() < end_time {
            Schedule::handle_pending_events(emu);
            emu.arm7
                .schedule
                .set_target_time(emu.arm7.schedule.schedule().next_event_time().min(end_time));
            if let Some(channel) = emu.arm7.dma.cur_channel {
                Arm7::run_dma_transfer(emu, channel);
                continue;
            }
            if emu.arm7.irqs.triggered() {
                let prev_cpsr = emu.arm7.engine_data.regs.cpsr;
                let return_addr =
                    reg!(emu.arm7, 15).wrapping_sub((!prev_cpsr.thumb_state() as u32) << 2);
                if emu.arm7.hle_bios_enabled() {
                    enter_hle_irq::<true>(emu, return_addr);
                } else {
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
                    emu.arm7.engine_data.regs.cpsr = prev_cpsr
                        .with_mode(Mode::Irq)
                        .with_thumb_state(false)
                        .with_irqs_disabled(true);
                    emu.arm7.irqs.set_enabled_in_cpsr(false, ());
                    #[cfg(feature = "interp-pipeline-accurate-reloads")]
                    {
                        emu.arm7.engine_data.r15_increment = 4;
                    }
                    emu.arm7
                        .engine_data
                        .regs
                        .update_mode::<false>(prev_cpsr.mode(), Mode::Irq);
                    emu.arm7.engine_data.regs.spsr = prev_cpsr.into();
                    reg!(emu.arm7, 14) = return_addr;
                    reg!(emu.arm7, 15) = 0x0000_0018;
                    reload_pipeline::<{ StateSource::Arm }>(emu);
                }
            } else if emu.arm7.irqs.halted() {
                emu.arm7
                    .schedule
                    .set_cur_time(emu.arm7.schedule.target_time());
                continue;
            }
            while emu.arm7.schedule.cur_time() < emu.arm7.schedule.target_time() {
                #[cfg(feature = "debugger-hooks")]
                {
                    let r15 = reg!(emu.arm7, 15)
                        .wrapping_sub(8 >> emu.arm7.engine_data.regs.cpsr.thumb_state() as u8);
                    if emu.arm7.engine_data.next_breakpoint_addr == r15 {
                        if let Some(breakpoint_hook) = emu.arm7.breakpoint_hook().as_ref() {
                            if unsafe { breakpoint_hook.get()(emu, r15) } {
                                emu.arm7
                                    .schedule
                                    .set_target_time(emu.arm7.schedule.cur_time());
                                emu.arm7.stopped_by_debug_hook = true;
                                return;
                            }
                        }
                    }
                }
                #[cfg(feature = "interp-pipeline")]
                {
                    let addr = reg!(emu.arm7, 15);
                    let instr = emu.arm7.engine_data.pipeline[0];
                    emu.arm7.engine_data.pipeline[0] = emu.arm7.engine_data.pipeline[1];
                    if emu.arm7.engine_data.regs.cpsr.thumb_state() {
                        emu.arm7.engine_data.pipeline[1] =
                            thumb_pipeline_entry(
                                bus::read_16::<CpuAccess, _>(emu, addr) as PipelineEntry
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
