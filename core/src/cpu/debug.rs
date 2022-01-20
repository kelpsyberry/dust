use crate::{
    cpu::Engine,
    emu::Emu,
    utils::{zeroed_box, Zero},
};
use bitflags::bitflags;

#[repr(transparent)]
pub struct MemWatchpointRootTable(pub [Option<Box<MemWatchpointSubTable>>; 0x800]);

unsafe impl Zero for MemWatchpointRootTable {}

#[repr(transparent)]
pub struct MemWatchpointSubTable(pub [Option<Box<MemWatchpointLeafTable>>; 0x800]);

unsafe impl Zero for MemWatchpointSubTable {}

#[repr(transparent)]
pub struct MemWatchpointLeafTable(pub [usize; MWLT_ENTRY_COUNT as usize]);

unsafe impl Zero for MemWatchpointLeafTable {}

bitflags! {
    pub struct MemWatchpointRwMask: u8 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
    }
}

// Every leaf table should contain a total of 0x400 bytes, each occupying 2 bits, divided in `usize`
// blocks.
pub const MWLT_BYTES_PER_ENTRY_SHIFT: u32 = usize::BITS.trailing_zeros() - 1;
pub const MWLT_BYTES_PER_ENTRY_MASK: u32 = (1 << MWLT_BYTES_PER_ENTRY_SHIFT) - 1;
pub const MWLT_ENTRY_COUNT: u32 = 1 << (10 - MWLT_BYTES_PER_ENTRY_SHIFT);
pub const MWLT_ENTRY_COUNT_MASK: u32 = MWLT_ENTRY_COUNT - 1;

macro_rules! check_watchpoints {
    ($emu: expr, $core: expr, $addr: ident, $align_mask: expr, $mask: expr, $cause: ident) => {
        if let Some(hook) = &$core.debug.mem_watchpoint_hook {
            if let Some(leaf_table) = $core.debug.mem_watchpoints.0[($addr >> 21) as usize]
                .as_ref()
                .and_then(|sub_table| sub_table.0[($addr >> 10 & 0x7FF) as usize].as_ref())
            {
                // NOTE: Bits will never be lost by shifting as accesses are assumed to be aligned
                let leaf = leaf_table.0[($addr >> $crate::cpu::debug::MWLT_BYTES_PER_ENTRY_SHIFT
                    & ($crate::cpu::debug::MWLT_ENTRY_COUNT_MASK
                        & !($align_mask >> $crate::cpu::debug::MWLT_BYTES_PER_ENTRY_SHIFT)))
                    as usize]
                    >> (($addr & ($crate::cpu::debug::MWLT_BYTES_PER_ENTRY_MASK & !$align_mask))
                        << 1)
                    & $mask;
                if leaf != 0 {
                    use $crate::cpu::Schedule;
                    if unsafe {
                        hook.get()(
                            $emu,
                            $addr & !$align_mask,
                            $align_mask + 1,
                            $crate::cpu::debug::MemWatchpointTriggerCause::$cause,
                        )
                    } {
                        $core.schedule.set_target_time($core.schedule.cur_time());
                        $core.stopped_by_debug_hook = true;
                    }
                    $core.schedule.set_target_time($core.schedule.cur_time());
                }
            }
        }
    };
}

impl MemWatchpointRootTable {
    pub(super) fn add(&mut self, addr: u32, rw: MemWatchpointRwMask) {
        let sub_table = self.0[(addr >> 21) as usize].get_or_insert_with(zeroed_box);
        let leaf_table = sub_table.0[(addr >> 10 & 0x7FF) as usize].get_or_insert_with(zeroed_box);
        leaf_table.0[(addr >> MWLT_BYTES_PER_ENTRY_SHIFT & MWLT_ENTRY_COUNT_MASK) as usize] |=
            (rw.bits() as usize) << ((addr & MWLT_BYTES_PER_ENTRY_MASK) << 1);
    }

    pub(super) fn remove(&mut self, addr: u32, rw: MemWatchpointRwMask) {
        let root_i = (addr >> 21) as usize;
        let sub_table = match &mut self.0[root_i] {
            Some(sub_table_ptr) => sub_table_ptr,
            None => return,
        };
        let sub_i = (addr >> 10 & 0x7FF) as usize;
        let leaf_table = match &mut sub_table.0[sub_i] {
            Some(leaf_table_ptr) => leaf_table_ptr,
            None => return,
        };
        let leaf_i = (addr >> MWLT_BYTES_PER_ENTRY_SHIFT & MWLT_ENTRY_COUNT_MASK) as usize;
        leaf_table.0[leaf_i] &=
            !((rw.bits() as usize) << ((addr & MWLT_BYTES_PER_ENTRY_MASK) << 1));
        if leaf_table.0[leaf_i] != 0 || leaf_table.0.iter().any(|value| *value != 0) {
            return;
        }
        sub_table.0[sub_i] = None;
        if sub_table.0.iter().any(Option::is_some) {
            return;
        }
        self.0[root_i] = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemWatchpointTriggerCause {
    Read,
    Write,
}

pub struct Hook<T: ?Sized>(pub *mut T);

impl<T: ?Sized> Hook<T> {
    pub fn new(value: Box<T>) -> Self {
        Hook(Box::into_raw(value))
    }

    #[allow(clippy::mut_from_ref)]
    pub(super) unsafe fn get<'a>(&self) -> &'a mut T {
        &mut *self.0
    }
}

impl<T: ?Sized> Drop for Hook<T> {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(self.0));
        }
    }
}

pub type SwiHook<E> = Hook<dyn FnMut(&mut Emu<E>, u8) -> bool>;
pub type BreakpointHook<E> = Hook<dyn FnMut(&mut Emu<E>, u32) -> bool>;
pub type MemWatchpointHook<E> =
    Hook<dyn FnMut(&mut Emu<E>, u32, u8, MemWatchpointTriggerCause) -> bool>;

pub(super) struct CoreData<E: Engine> {
    pub swi_hook: Option<SwiHook<E>>,
    pub sw_breakpoints: Vec<u32>,
    pub sw_breakpoint_hook: Option<BreakpointHook<E>>,
    pub hw_breakpoint_hook: Option<BreakpointHook<E>>,
    pub mem_watchpoint_hook: Option<MemWatchpointHook<E>>,
    pub mem_watchpoints: Box<MemWatchpointRootTable>,
}

impl<E: Engine> CoreData<E> {
    pub(super) fn new() -> Self {
        CoreData {
            swi_hook: None,
            sw_breakpoints: Vec::new(),
            sw_breakpoint_hook: None,
            hw_breakpoint_hook: None,
            mem_watchpoint_hook: None,
            mem_watchpoints: zeroed_box(),
        }
    }
}
