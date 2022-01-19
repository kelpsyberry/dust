pub mod bus;
pub mod psr;
mod schedule;
pub(crate) use schedule::Schedule;
mod irqs;
pub(crate) use irqs::Irqs;
#[cfg(any(feature = "debugger-hooks", doc))]
#[macro_use]
pub mod debug;
pub mod arm7;
pub mod arm9;
#[cfg(feature = "disasm")]
pub mod disasm;
pub mod dma;
mod engines_common;
pub mod interpreter;
#[cfg(feature = "jit")]
pub mod jit;
pub mod timers;

use crate::{emu::Emu, utils::ByteSlice};
use cfg_if::cfg_if;
use psr::{Cpsr, Spsr};

pub trait Engine: Sized {
    type GlobalData;
    type Arm7Data: Arm7Data + CoreData<Engine = Self>;
    type Arm9Data: Arm9Data + CoreData<Engine = Self>;

    fn into_data(self) -> (Self::GlobalData, Self::Arm7Data, Self::Arm9Data);
}

#[derive(Clone, Debug)]
pub struct Regs {
    pub gprs: [u32; 16],
    pub cpsr: Cpsr,
    pub spsr: Spsr,
    pub r8_14_fiq: [u32; 7],
    pub r8_12_other: [u32; 5],
    pub r13_14_irq: [u32; 2],
    pub r13_14_svc: [u32; 2],
    pub r13_14_abt: [u32; 2],
    pub r13_14_und: [u32; 2],
    pub r13_14_user: [u32; 2],
    pub spsr_fiq: Spsr,
    pub spsr_irq: Spsr,
    pub spsr_svc: Spsr,
    pub spsr_abt: Spsr,
    pub spsr_und: Spsr,
}

pub trait CoreData {
    type Engine: Engine;

    fn setup(emu: &mut Emu<Self::Engine>);
    fn setup_direct_boot(
        emu: &mut Emu<Self::Engine>,
        entry_addr: u32,
        loaded_data: (ByteSlice, u32),
    );

    fn invalidate_word(&mut self, addr: u32);
    fn invalidate_word_range(&mut self, bounds: (u32, u32));

    fn jump(emu: &mut Emu<Self::Engine>, addr: u32);
    fn r15(&self) -> u32;
    fn cpsr(&self) -> Cpsr;
    fn regs(&self) -> Regs;
    fn set_regs(&mut self, values: &Regs);

    cfg_if! {
        if #[cfg(any(feature = "debugger-hooks", doc))] {
            fn set_swi_hook(&mut self, hook: &Option<debug::SwiHook>);
            fn add_breakpoint(&mut self, addr: u32);
            fn remove_breakpoint(&mut self, addr: u32, i: usize, breakpoints: &[u32]);
            fn clear_breakpoints(&mut self);
            fn set_breakpoint_hook(&mut self, hook: &Option<debug::BreakpointHook>);
            fn set_mem_watchpoint_hook(&mut self, hook: &Option<debug::MemWatchpointHook>);
            fn add_mem_watchpoint(&mut self, addr: u32, rw: debug::MemWatchpointRwMask);
            fn remove_mem_watchpoint(&mut self, addr: u32, rw: debug::MemWatchpointRwMask);
        }
    }
}

pub trait Arm7Data: CoreData {
    fn run_until(emu: &mut Emu<Self::Engine>, end_time: arm7::Timestamp);
}

pub trait Arm9Data: CoreData {
    fn set_high_exc_vectors(&mut self, value: bool);
    fn set_t_bit_load_disabled(&mut self, value: bool);

    fn run_until(emu: &mut Emu<Self::Engine>, end_time: arm9::Timestamp);
}
