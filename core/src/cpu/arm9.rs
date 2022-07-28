pub mod bus;
mod irqs;
pub use irqs::{IrqFlags, Irqs};
mod schedule;
pub use schedule::{event_slots, Event, EventSlotIndex, Schedule, Timestamp};
pub mod cp15;
pub mod div_engine;
pub mod dma;
pub mod sqrt_engine;

#[cfg(any(feature = "debugger-hooks", doc))]
use super::debug;
use super::{psr::Cpsr, timers::Timers, CoreData, Engine, Regs};
#[cfg(feature = "debugger-hooks")]
use crate::cpu::Arm9Data;
use crate::{
    cpu::{self, hle_bios},
    emu::{swram::Swram, Emu, LocalExMemControl},
    utils::{Bytes, OwnedBytesCellPtr, Savestate},
};
use cp15::Cp15;
use div_engine::DivEngine;
use sqrt_engine::SqrtEngine;

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct PostBootFlag(pub u8): Debug {
        pub booted: bool @ 0,
        pub extra_bit: bool @ 1,
    }
}

pub const BIOS_SIZE: usize = 0x1000;
pub const BIOS_BUFFER_SIZE: usize = bus::ptrs::Ptrs::PAGE_SIZE;

#[derive(Savestate)]
#[load(in_place_only)]
pub struct Arm9<E: Engine> {
    #[cfg(feature = "log")]
    #[savestate(skip)]
    pub(super) logger: slog::Logger,
    #[cfg(feature = "debugger-hooks")]
    #[savestate(skip)]
    pub(super) debug: debug::Arm9Data<E>,
    pub engine_data: E::Arm9Data,
    pub(super) hle_bios: hle_bios::arm9::State,
    #[savestate(skip)]
    bios: OwnedBytesCellPtr<BIOS_BUFFER_SIZE>,
    pub schedule: Schedule,
    #[savestate(skip)]
    bus_ptrs: Box<bus::ptrs::Ptrs>,
    #[savestate(skip)]
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
    #[cfg(feature = "debugger-hooks")]
    #[savestate(skip)]
    pub stopped: bool,
    #[cfg(feature = "debugger-hooks")]
    #[savestate(skip)]
    pub(crate) stopped_by_debug_hook: bool,
}

impl<E: Engine> Arm9<E> {
    pub(crate) fn new(
        engine_data: E::Arm9Data,
        bios: Option<OwnedBytesCellPtr<BIOS_BUFFER_SIZE>>,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Self {
        let mut schedule = Schedule::new();
        let timers = Timers::new(&mut schedule);
        let div_engine = DivEngine::new(&mut schedule);
        let sqrt_engine = SqrtEngine::new(&mut schedule);
        Arm9 {
            #[cfg(feature = "log")]
            logger,
            #[cfg(feature = "debugger-hooks")]
            debug: debug::Arm9Data::new(),
            engine_data,
            hle_bios: hle_bios::arm9::State::new(bios.is_none()),
            bios: bios.unwrap_or_else(|| {
                let buf = OwnedBytesCellPtr::new_zeroed();
                (unsafe { buf.as_byte_mut_slice() })[..hle_bios::arm9::BIOS.len()]
                    .copy_from_slice(&hle_bios::arm9::BIOS);
                buf
            }),
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
            #[cfg(feature = "debugger-hooks")]
            stopped: false,
            #[cfg(feature = "debugger-hooks")]
            stopped_by_debug_hook: false,
        }
    }

    pub(crate) fn setup(emu: &mut Emu<E>) {
        Self::setup_sys_bus_ptrs(emu);
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
    pub fn set_cpsr(emu: &mut Emu<E>, value: Cpsr) {
        E::Arm9Data::set_cpsr(emu, value);
    }

    #[inline]
    pub fn regs(&self) -> Regs {
        self.engine_data.regs()
    }

    #[inline]
    pub fn set_regs(emu: &mut Emu<E>, regs: &Regs) {
        E::Arm9Data::set_regs(emu, regs);
    }

    cfg_if::cfg_if! {
        if #[cfg(any(feature = "debugger-hooks", doc))] {
            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn swi_hook(&self) -> &Option<debug::SwiHook<E>> {
                &self.debug.swi_hook
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn set_swi_hook(&mut self, value: Option<debug::SwiHook<E>>) {
                self.debug.swi_hook = value;
                self.engine_data.set_swi_hook(&self.debug.swi_hook);
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn undef_hook(&self) -> &Option<debug::UndefHook<E>> {
                &self.debug.undef_hook
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn set_undef_hook(&mut self, value: Option<debug::UndefHook<E>>) {
                self.debug.undef_hook = value;
                self.engine_data.set_undef_hook(&self.debug.undef_hook);
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn prefetch_abort_hook(&self) -> &Option<debug::PrefetchAbortHook<E>> {
                &self.debug.prefetch_abort_hook
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn set_prefetch_abort_hook(&mut self, value: Option<debug::PrefetchAbortHook<E>>) {
                self.debug.prefetch_abort_hook = value;
                self.engine_data.set_prefetch_abort_hook(&self.debug.prefetch_abort_hook);
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn data_abort_hook(&self) -> &Option<debug::DataAbortHook<E>> {
                &self.debug.data_abort_hook
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn set_data_abort_hook(&mut self, value: Option<debug::DataAbortHook<E>>) {
                self.debug.data_abort_hook = value;
                self.engine_data.set_data_abort_hook(&self.debug.data_abort_hook);
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn breakpoints(&self) -> &[u32] {
                &self.debug.breakpoints
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn add_breakpoint(&mut self, addr: u32) {
                if let Err(i) = self.debug.breakpoints.binary_search(&addr) {
                    self.debug.breakpoints.insert(i, addr);
                    self.engine_data.add_breakpoint(addr);
                }
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn remove_breakpoint(&mut self, addr: u32) {
                if let Ok(i) = self.debug.breakpoints.binary_search(&addr) {
                    self.debug.breakpoints.remove(i);
                    self.engine_data.remove_breakpoint(addr, i, &self.debug.breakpoints);
                }
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn clear_breakpoints(&mut self) {
                self.debug.breakpoints.clear();
                self.engine_data.clear_breakpoints();
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn breakpoint_hook(&self) -> &Option<debug::BreakpointHook<E>> {
                &self.debug.breakpoint_hook
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn set_breakpoint_hook(&mut self, value: Option<debug::BreakpointHook<E>>) {
                self.debug.breakpoint_hook = value;
                self.engine_data.set_breakpoint_hook(&self.debug.breakpoint_hook);
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn mem_watchpoint_hook(&self) -> &Option<debug::MemWatchpointHook<E>> {
                &self.debug.mem_watchpoint_hook
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn set_mem_watchpoint_hook(&mut self, value: Option<debug::MemWatchpointHook<E>>) {
                self.debug.mem_watchpoint_hook = value;
                self.engine_data.set_mem_watchpoint_hook(&self.debug.mem_watchpoint_hook);
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn mem_watchpoints(&self) -> &debug::MemWatchpointRootTable {
                &self.debug.mem_watchpoints
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn add_mem_watchpoint(
                &mut self,
                mut addr: u32,
                size: u8,
                rw: debug::MemWatchpointRwMask,
            ) {
                addr &= !((size - 1) as u32);
                self.debug.mem_watchpoints.add(addr, size, rw);
                if rw.contains(debug::MemWatchpointRwMask::READ) {
                    self.bus_ptrs.disable_read(addr, cpu::bus::r_disable_flags::WATCHPOINT);
                    self.cp15.ptrs.disable_read(addr, cpu::bus::r_disable_flags::WATCHPOINT);
                }
                if rw.contains(debug::MemWatchpointRwMask::WRITE) {
                    self.bus_ptrs.disable_write(addr, cpu::bus::w_disable_flags::WATCHPOINT);
                    self.cp15.ptrs.disable_write(addr, cpu::bus::w_disable_flags::WATCHPOINT);
                }
                self.engine_data.add_mem_watchpoint(addr, size, rw);
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn remove_mem_watchpoint(
                &mut self,
                mut addr: u32,
                size: u8,
                rw: debug::MemWatchpointRwMask,
            ) {
                addr &= !((size - 1) as u32);
                self.debug.mem_watchpoints.remove(addr, size, rw);
                self.engine_data.remove_mem_watchpoint(addr, size, rw);
                let page_start_addr = addr & !bus::ptrs::Ptrs::PAGE_MASK;
                let page_end_addr = page_start_addr | bus::ptrs::Ptrs::PAGE_MASK;
                let cp15_page_start_addr = addr & !cp15::ptrs::Ptrs::PAGE_MASK;
                let cp15_page_end_addr = cp15_page_start_addr | cp15::ptrs::Ptrs::PAGE_MASK;
                if rw.contains(debug::MemWatchpointRwMask::READ) {
                    let page_free = self.debug.mem_watchpoints.is_free(
                        (page_start_addr, page_end_addr),
                        debug::MemWatchpointRwMask::READ,
                    );
                    if page_free {
                        self.bus_ptrs
                            .enable_read(page_start_addr, cpu::bus::r_disable_flags::WATCHPOINT);
                    }
                    if page_free
                        || self.debug.mem_watchpoints.is_free(
                            (cp15_page_start_addr, cp15_page_end_addr),
                            debug::MemWatchpointRwMask::READ,
                        )
                    {
                        self.cp15
                            .ptrs
                            .enable_read(page_start_addr, cpu::bus::r_disable_flags::WATCHPOINT);
                    }
                }
                if rw.contains(debug::MemWatchpointRwMask::WRITE) {
                    let page_free = self.debug.mem_watchpoints.is_free(
                        (page_start_addr, page_end_addr),
                        debug::MemWatchpointRwMask::WRITE,
                    );
                    if page_free {
                        self.bus_ptrs
                            .enable_write(page_start_addr, cpu::bus::r_disable_flags::WATCHPOINT);
                    }
                    if page_free
                        || self.debug.mem_watchpoints.is_free(
                            (cp15_page_start_addr, cp15_page_end_addr),
                            debug::MemWatchpointRwMask::WRITE,
                        )
                    {
                        self.cp15
                            .ptrs
                            .enable_write(page_start_addr, cpu::bus::r_disable_flags::WATCHPOINT);
                    }
                }
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn clear_mem_watchpoints(&mut self) {
                self.debug.mem_watchpoints.clear();
                self.engine_data.clear_mem_watchpoints();
                self.bus_ptrs.enable_read_all(cpu::bus::r_disable_flags::WATCHPOINT);
                self.bus_ptrs.enable_write_all(cpu::bus::w_disable_flags::WATCHPOINT);
                self.cp15.ptrs.enable_read_all(cpu::bus::r_disable_flags::WATCHPOINT);
                self.cp15.ptrs.enable_write_all(cpu::bus::w_disable_flags::WATCHPOINT);
            }
        }
    }

    #[inline]
    pub fn hle_bios_enabled(&self) -> bool {
        self.hle_bios.enabled
    }

    #[inline]
    pub fn bios(&self) -> &Bytes<BIOS_BUFFER_SIZE> {
        unsafe { &*self.bios.as_bytes_ptr() }
    }

    #[inline]
    pub fn local_ex_mem_control(&self) -> LocalExMemControl {
        self.local_ex_mem_control
    }

    #[inline]
    pub fn write_local_ex_mem_control(&mut self, value: LocalExMemControl) {
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

    fn setup_sys_bus_ptrs(emu: &mut Emu<E>) {
        unsafe {
            emu.arm9.bus_ptrs.map_range(
                bus::ptrs::mask::ALL,
                emu.main_mem().as_ptr(),
                emu.main_mem_mask().get() as usize + 1,
                (0x0200_0000, 0x02FF_FFFF),
            );
            emu.gpu.vram.setup_arm9_bus_ptrs(&mut emu.arm9.bus_ptrs);
            emu.arm9.bus_ptrs.map_range(
                bus::ptrs::mask::R,
                emu.arm9.bios.as_ptr(),
                0x4000,
                (0xFFFF_0000, 0xFFFF_0000 + (emu.arm9.bios.len() - 1) as u32),
            );
        }
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
