pub mod bus;
pub mod psr;
mod schedule;
pub(crate) use schedule::Schedule;
mod irqs;
pub(crate) use irqs::Irqs;
#[cfg(any(feature = "debug-hooks", doc))]
#[macro_use]
pub mod debug;
pub mod arm7;
pub mod arm9;
pub mod dma;
mod engines_common;
#[cfg(feature = "disasm")]
pub mod disasm;
pub mod interpreter;
#[cfg(feature = "jit")]
pub mod jit;
pub mod timers;

use crate::{emu::Emu, utils::ByteSlice};
use cfg_if::cfg_if;
use psr::Cpsr;

pub trait Engine: Sized {
    type GlobalData;
    type Arm7Data: Arm7Data + CoreData<Engine = Self>;
    type Arm9Data: Arm9Data + CoreData<Engine = Self>;

    fn into_data(self) -> (Self::GlobalData, Self::Arm7Data, Self::Arm9Data);
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
    fn regs(&self) -> ([u32; 16], Cpsr);
    fn set_regs(&mut self, values: ([u32; 16], Cpsr));

    cfg_if! {
        if #[cfg(any(feature = "debug-hooks", doc))] {
            fn set_branch_breakpoint_hooks(
                &mut self,
                hooks: &Option<(debug::BranchHook, debug::BreakpointHook, u32)>,
            );
            fn set_swi_hook(&mut self, hooks: &Option<debug::SwiHook>);
            fn set_mem_watchpoint_hook(&mut self, hook: &Option<debug::MemWatchpointHook>);
            fn add_mem_watchpoint(&mut self, addr: u32, rw: debug::MemWatchpointRwMask);
            fn remove_mem_watchpoint(&mut self, addr: u32, rw: debug::MemWatchpointRwMask);
        }
    }
}

pub trait Arm7Data: CoreData {
    fn r15(&self) -> u32;

    fn run_until(emu: &mut Emu<Self::Engine>, end_time: arm7::Timestamp);
}

pub trait Arm9Data: CoreData {
    fn set_high_exc_vectors(&mut self, value: bool);
    fn set_t_bit_load_disabled(&mut self, value: bool);

    fn run_until(emu: &mut Emu<Self::Engine>, end_time: arm9::Timestamp);
}
