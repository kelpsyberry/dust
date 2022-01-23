pub mod bus;
mod irqs;
pub use irqs::{IrqFlags, Irqs};
mod schedule;
pub use schedule::{event_slots, Event, EventSlotIndex, Schedule, Timestamp};
pub mod dma;

#[cfg(feature = "debugger-hooks")]
use super::debug;
use super::{psr::Cpsr, timers::Timers, CoreData, Engine, Regs};
use crate::{
    cpu,
    emu::{swram::Swram, Emu, LocalExMemControl},
    utils::{Bytes, OwnedBytesCellPtr},
};
use cfg_if::cfg_if;

pub const BIOS_SIZE: usize = 0x4000;

pub struct Arm7<E: Engine> {
    #[cfg(feature = "log")]
    pub logger: slog::Logger,
    #[cfg(feature = "debugger-hooks")]
    pub(super) debug: debug::CoreData<E>,
    pub engine_data: E::Arm7Data,
    bios: OwnedBytesCellPtr<BIOS_SIZE>,
    pub wram: OwnedBytesCellPtr<0x1_0000>,
    pub schedule: Schedule,
    pub(super) bus_ptrs: Box<bus::ptrs::Ptrs>,
    pub(super) bus_timings: Box<bus::timings::Timings>,
    pub irqs: Irqs,
    pub timers: Timers<Schedule>,
    local_ex_mem_control: LocalExMemControl,
    post_boot_flag: bool,
    bios_prot: u16,
    last_bios_word: u32,
    pub dma: cpu::dma::Controller<dma::Timing, ()>,
    last_dma_words: [u32; 4],
    #[cfg(feature = "debugger-hooks")]
    pub stopped: bool,
    #[cfg(feature = "debugger-hooks")]
    pub(crate) stopped_by_debug_hook: bool,
}

impl<E: Engine> Arm7<E> {
    pub(crate) fn new(
        engine_data: E::Arm7Data,
        bios: OwnedBytesCellPtr<BIOS_SIZE>,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Self {
        let mut schedule = Schedule::new();
        let irqs = Irqs::new(&mut schedule);
        let timers = Timers::new(&mut schedule);
        Arm7 {
            #[cfg(feature = "log")]
            logger,
            #[cfg(feature = "debugger-hooks")]
            debug: debug::CoreData::new(),
            engine_data,
            bios,
            wram: OwnedBytesCellPtr::new_zeroed(),
            schedule,
            bus_ptrs: bus::ptrs::Ptrs::new_boxed(),
            bus_timings: bus::timings::Timings::new_boxed(),
            irqs,
            timers,
            local_ex_mem_control: LocalExMemControl(0),
            post_boot_flag: false,
            bios_prot: 0,
            last_bios_word: 0,
            dma: cpu::dma::Controller {
                channels: [
                    cpu::dma::Channel::new(
                        0x3FFF,
                        0x07FF_FFFF,
                        0x07FF_FFFF,
                        dma::Timing::Disabled,
                        (),
                    ),
                    cpu::dma::Channel::new(
                        0x3FFF,
                        0x0FFF_FFFF,
                        0x07FF_FFFF,
                        dma::Timing::Disabled,
                        (),
                    ),
                    cpu::dma::Channel::new(
                        0x3FFF,
                        0x0FFF_FFFF,
                        0x07FF_FFFF,
                        dma::Timing::Disabled,
                        (),
                    ),
                    cpu::dma::Channel::new(
                        0xFFFF,
                        0x0FFF_FFFF,
                        0x0FFF_FFFF,
                        dma::Timing::Disabled,
                        (),
                    ),
                ],
                cur_channel: None,
                running_channels: 0,
            },
            last_dma_words: [0; 4],
            #[cfg(feature = "debugger-hooks")]
            stopped: false,
            #[cfg(feature = "debugger-hooks")]
            stopped_by_debug_hook: false,
        }
    }

    pub(crate) fn setup(emu: &mut Emu<E>) {
        bus::ptrs::Ptrs::setup(emu);
        emu.arm7.bus_timings.setup();
    }

    #[inline]
    pub fn jump(emu: &mut Emu<E>, addr: u32) {
        E::Arm7Data::jump(emu, addr);
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
    pub fn set_regs(&mut self, regs: &Regs) {
        self.engine_data.set_regs(regs);
    }

    cfg_if! {
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
                }
                if rw.contains(debug::MemWatchpointRwMask::WRITE) {
                    self.bus_ptrs.disable_write(addr, cpu::bus::w_disable_flags::WATCHPOINT);
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
                if rw.contains(debug::MemWatchpointRwMask::READ)
                    && self.debug.mem_watchpoints.is_free(
                        (page_start_addr, page_end_addr),
                        debug::MemWatchpointRwMask::READ,
                    )
                {
                    self.bus_ptrs
                        .enable_read(page_start_addr, cpu::bus::r_disable_flags::WATCHPOINT);
                }
                if rw.contains(debug::MemWatchpointRwMask::WRITE)
                    && self.debug.mem_watchpoints.is_free(
                        (page_start_addr, page_end_addr),
                        debug::MemWatchpointRwMask::WRITE,
                    )
                {
                    self.bus_ptrs
                        .enable_write(page_start_addr, cpu::bus::w_disable_flags::WATCHPOINT);
                }
            }

            #[doc(cfg(feature = "debugger-hooks"))]
            #[inline]
            pub fn clear_mem_watchpoints(&mut self) {
                self.debug.mem_watchpoints.clear();
                self.engine_data.clear_mem_watchpoints();
                self.bus_ptrs.enable_read_all(cpu::bus::r_disable_flags::WATCHPOINT);
                self.bus_ptrs.enable_write_all(cpu::bus::w_disable_flags::WATCHPOINT);
            }
        }
    }

    #[inline]
    pub fn bios(&self) -> &Bytes<BIOS_SIZE> {
        unsafe { &*self.bios.as_bytes_ptr() }
    }

    #[inline]
    pub fn into_bios(self) -> OwnedBytesCellPtr<BIOS_SIZE> {
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
    pub fn post_boot_flag(&self) -> bool {
        self.post_boot_flag
    }

    #[inline]
    pub fn set_post_boot_flag(&mut self, value: bool) {
        self.post_boot_flag = value;
    }

    #[inline]
    pub fn bios_prot(&self) -> u16 {
        self.bios_prot
    }

    #[inline]
    pub fn set_bios_prot(&mut self, value: u16) {
        self.bios_prot = value & 0x3FFE;
    }

    #[inline]
    pub fn last_bios_word(&self) -> u32 {
        self.last_bios_word
    }

    #[inline]
    pub fn last_dma_words(&self) -> [u32; 4] {
        self.last_dma_words
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
        self.invalidate_word_range(bounds);
    }

    #[inline]
    pub(crate) fn recalc_swram(&mut self, swram: &Swram) {
        unsafe {
            match swram.control().layout() {
                0 => {
                    self.map_sys_bus_ptr_range(
                        bus::ptrs::mask::ALL,
                        self.wram.as_ptr(),
                        0x1_0000,
                        (0x0300_0000, 0x037F_FFFF),
                    );
                }
                1 => {
                    self.map_sys_bus_ptr_range(
                        bus::ptrs::mask::ALL,
                        swram.contents().as_ptr(),
                        0x4000,
                        (0x0300_0000, 0x037F_FFFF),
                    );
                }
                2 => {
                    self.map_sys_bus_ptr_range(
                        bus::ptrs::mask::ALL,
                        swram.contents().as_ptr().add(0x4000),
                        0x4000,
                        (0x0300_0000, 0x037F_FFFF),
                    );
                }
                _ => {
                    self.map_sys_bus_ptr_range(
                        bus::ptrs::mask::ALL,
                        swram.contents().as_ptr(),
                        0x8000,
                        (0x0300_0000, 0x037F_FFFF),
                    );
                }
            }
        }
    }
}
