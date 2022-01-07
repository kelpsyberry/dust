pub mod bus;
mod irqs;
pub use irqs::{IrqFlags, Irqs};
mod schedule;
pub use schedule::{event_slots, Event, EventSlotIndex, Schedule, Timestamp};
pub mod cp15;
pub mod div_engine;
pub mod dma;
pub mod sqrt_engine;

#[cfg(feature = "debug-hooks")]
use super::debug;
use super::{psr::Cpsr, timers::Timers, CoreData, Engine, Regs};
use crate::{
    cpu,
    emu::{swram::Swram, Emu, LocalExMemControl},
    utils::{bitfield_debug, Bytes, OwnedBytesCellPtr},
};
use cfg_if::cfg_if;
use cp15::Cp15;
use div_engine::DivEngine;
use sqrt_engine::SqrtEngine;

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct PostBootFlag(pub u8) {
        pub booted: bool @ 0,
        pub extra_bit: bool @ 1,
    }
}

pub const BIOS_SIZE: usize = 0x1000;
pub const BIOS_BUFFER_SIZE: usize = bus::ptrs::Ptrs::PAGE_SIZE;

pub struct Arm9<E: Engine> {
    #[cfg(feature = "log")]
    pub logger: slog::Logger,
    #[cfg(feature = "debug-hooks")]
    debug: debug::CoreData,
    pub engine_data: E::Arm9Data,
    pub bios: OwnedBytesCellPtr<BIOS_BUFFER_SIZE>,
    pub schedule: Schedule,
    bus_ptrs: Box<bus::ptrs::Ptrs>,
    bus_timings: Box<bus::timings::Timings>,
    pub cp15: Cp15,
    pub irqs: Irqs,
    pub timers: Timers<Schedule>,
    local_ex_mem_control: LocalExMemControl,
    post_boot_flag: PostBootFlag,
    pub dma: cpu::dma::Controller<dma::Timing, u32>,
    pub dma_fill: Bytes<16>,
    pub div_engine: DivEngine,
    pub sqrt_engine: SqrtEngine,
}

impl<E: Engine> Arm9<E> {
    pub(crate) fn new(
        engine_data: E::Arm9Data,
        bios: OwnedBytesCellPtr<BIOS_BUFFER_SIZE>,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Self {
        let mut schedule = Schedule::new();
        let timers = Timers::new(&mut schedule);
        let div_engine = DivEngine::new(&mut schedule);
        let sqrt_engine = SqrtEngine::new(&mut schedule);
        Arm9 {
            #[cfg(feature = "log")]
            logger,
            #[cfg(feature = "debug-hooks")]
            debug: debug::CoreData::new(),
            engine_data,
            bios,
            schedule,
            bus_ptrs: bus::ptrs::Ptrs::new_boxed(),
            bus_timings: bus::timings::Timings::new_boxed(),
            cp15: Cp15::new(),
            irqs: Irqs::new(),
            timers,
            local_ex_mem_control: LocalExMemControl(0),
            post_boot_flag: PostBootFlag(0),
            dma: cpu::dma::Controller {
                channels: [
                    cpu::dma::Channel::new(
                        0x001F_FFFF,
                        0x0FFF_FFFF,
                        0x0FFF_FFFF,
                        dma::Timing::Disabled,
                        0,
                    ),
                    cpu::dma::Channel::new(
                        0x001F_FFFF,
                        0x0FFF_FFFF,
                        0x0FFF_FFFF,
                        dma::Timing::Disabled,
                        0,
                    ),
                    cpu::dma::Channel::new(
                        0x001F_FFFF,
                        0x0FFF_FFFF,
                        0x0FFF_FFFF,
                        dma::Timing::Disabled,
                        0,
                    ),
                    cpu::dma::Channel::new(
                        0x001F_FFFF,
                        0x0FFF_FFFF,
                        0x0FFF_FFFF,
                        dma::Timing::Disabled,
                        0,
                    ),
                ],
                cur_channel: None,
                running_channels: 0,
            },
            dma_fill: Bytes::new([0; 16]),
            div_engine,
            sqrt_engine,
        }
    }

    pub(crate) fn setup(emu: &mut Emu<E>) {
        bus::ptrs::Ptrs::setup(emu);
        emu.arm9.bus_timings.setup();
        Cp15::setup(emu);
    }

    #[inline]
    pub fn jump(emu: &mut Emu<E>, addr: u32) {
        E::Arm9Data::jump(emu, addr);
    }

    #[inline]
    pub fn r15(&self) -> u32 {
        self.engine_data.r15()
    }

    #[inline]
    pub fn cpsr(&self) -> Cpsr {
        self.engine_data.cpsr()
    }

    #[inline]
    pub fn regs(&self) -> Regs {
        self.engine_data.regs()
    }

    #[inline]
    pub fn set_regs(&mut self, values: &Regs) {
        self.engine_data.set_regs(values);
    }

    cfg_if! {
        if #[cfg(any(feature = "debug-hooks", doc))] {
            #[doc(cfg(feature = "debug-hooks"))]
            #[inline]
            pub fn branch_breakpoint_hooks(&self) -> &Option<(debug::BranchHook, debug::BreakpointHook)> {
                &self.debug.branch_breakpoint_hooks
            }

            #[doc(cfg(feature = "debug-hooks"))]
            #[inline]
            pub fn set_branch_breakpoint_hooks(
                &mut self,
                value: Option<(debug::BranchHook, debug::BreakpointHook, u32)>,
            ) {
                self.debug.branch_breakpoint_hooks = value.map(|v| (v.0, v.1));
                self.engine_data.set_branch_breakpoint_hooks(&value);
            }

            #[doc(cfg(feature = "debug-hooks"))]
            #[inline]
            pub fn swi_hook(&self) -> &Option<debug::SwiHook> {
                &self.debug.swi_hook
            }

            #[doc(cfg(feature = "debug-hooks"))]
            #[inline]
            pub fn set_swi_hook(&mut self, value: Option<debug::SwiHook>) {
                self.debug.swi_hook = value;
                self.engine_data.set_swi_hook(&self.swi_hook);
            }

            #[doc(cfg(feature = "debug-hooks"))]
            #[inline]
            pub fn mem_watchpoint_hook(&self) -> &Option<debug::MemWatchpointHook> {
                &self.debug.mem_watchpoint_hook
            }

            #[doc(cfg(feature = "debug-hooks"))]
            #[inline]
            pub fn set_mem_watchpoint_hook(&mut self, value: Option<debug::MemWatchpointHook>) {
                self.debug.mem_watchpoint_hook = value;
                self.engine_data.set_mem_watchpoint_hook(&self.mem_watchpoint_hook);
            }

            #[doc(cfg(feature = "debug-hooks"))]
            #[inline]
            pub fn mem_watchpoints(&self) -> &debug::MemWatchpointRootTable {
                &self.debug.mem_watchpoints
            }

            #[doc(cfg(feature = "debug-hooks"))]
            #[inline]
            pub fn add_mem_watchpoint(&mut self, addr: u32, rw: debug::MemWatchpointRwMask) {
                self.debug.mem_watchpoints.add(addr, rw);
                self.engine_data.add_mem_watchpoint(addr, rw);
                todo!();
            }

            #[doc(cfg(feature = "debug-hooks"))]
            #[inline]
            pub fn remove_mem_watchpoint(&mut self, addr: u32, rw: debug::MemWatchpointRwMask) {
                self.debug.mem_watchpoints.remove(addr, rw);
                self.engine_data.remove_mem_watchpoint(addr, rw);
                todo!();
            }
        }
    }

    #[inline]
    pub fn bios(&self) -> &Bytes<BIOS_BUFFER_SIZE> {
        unsafe { &*self.bios.as_bytes_ptr() }
    }

    #[inline]
    pub fn into_bios(self) -> OwnedBytesCellPtr<BIOS_BUFFER_SIZE> {
        self.bios
    }

    #[inline]
    pub fn local_ex_mem_control(&self) -> LocalExMemControl {
        self.local_ex_mem_control
    }

    #[inline]
    pub fn set_local_ex_mem_control(&mut self, value: LocalExMemControl) {
        self.local_ex_mem_control.0 = value.0 & 0x7F;
    }

    #[inline]
    pub fn post_boot_flag(&self) -> PostBootFlag {
        self.post_boot_flag
    }

    #[inline]
    pub fn set_post_boot_flag(&mut self, value: PostBootFlag) {
        self.post_boot_flag.0 = value.0 & 3;
    }

    #[inline]
    pub fn invalidate_word_range(&mut self, bounds: (u32, u32)) {
        self.engine_data.invalidate_word_range(bounds);
    }

    #[inline]
    pub(crate) unsafe fn map_sys_bus_ptr_range(
        &mut self,
        mask: bus::ptrs::Mask,
        start_ptr: *mut u8,
        mem_size: usize,
        bounds: (u32, u32),
    ) {
        self.bus_ptrs.map_range(mask, start_ptr, mem_size, bounds);
        self.cp15
            .ptrs
            .map_sys_bus_range(start_ptr, mem_size, bounds, mask);
        self.invalidate_word_range(bounds);
    }

    #[inline]
    pub(crate) fn unmap_sys_bus_ptr_range(&mut self, bounds: (u32, u32)) {
        self.bus_ptrs.unmap_range(bounds);
        self.cp15.ptrs.unmap_sys_bus_range(bounds);
        self.invalidate_word_range(bounds);
    }

    #[inline]
    pub(crate) fn recalc_swram(&mut self, swram: &Swram) {
        unsafe {
            match swram.control().layout() {
                0 => {
                    self.map_sys_bus_ptr_range(
                        bus::ptrs::mask::ALL,
                        swram.contents().as_ptr(),
                        0x8000,
                        (0x0300_0000, 0x03FF_FFFF),
                    );
                }
                1 => {
                    self.map_sys_bus_ptr_range(
                        bus::ptrs::mask::ALL,
                        swram.contents().as_ptr().add(0x4000),
                        0x4000,
                        (0x0300_0000, 0x03FF_FFFF),
                    );
                }
                2 => {
                    self.map_sys_bus_ptr_range(
                        bus::ptrs::mask::ALL,
                        swram.contents().as_ptr(),
                        0x4000,
                        (0x0300_0000, 0x03FF_FFFF),
                    );
                }
                _ => {
                    self.unmap_sys_bus_ptr_range((0x0300_0000, 0x03FF_FFFF));
                }
            }
        }
    }
}
