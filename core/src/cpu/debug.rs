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
                let leaf = leaf_table.0[($addr >> $crate::cpu::debug::MWLT_BYTES_PER_ENTRY_SHIFT
                    & ($crate::cpu::debug::MWLT_ENTRY_COUNT_MASK
                        & !($align_mask >> $crate::cpu::debug::MWLT_BYTES_PER_ENTRY_SHIFT)))
                    as usize]
                    >> (($addr & ($crate::cpu::debug::MWLT_BYTES_PER_ENTRY_MASK & !$align_mask))
                        << 1)
                    & $mask;
                if leaf != 0
                    && unsafe {
                        hook.get()(
                            $emu,
                            $addr & !$align_mask,
                            $align_mask + 1,
                            $crate::cpu::debug::MemWatchpointTriggerCause::$cause,
                        )
                    }
                {
                    use $crate::cpu::Schedule;
                    $core.schedule.set_target_time($core.schedule.cur_time());
                    $core.stopped_by_debug_hook = true;
                }
            }
        }
    };
}

impl MemWatchpointRootTable {
    pub(super) fn add(&mut self, addr: u32, size: u8, rw: MemWatchpointRwMask) {
        let sub_table = self.0[(addr >> 21) as usize].get_or_insert_with(zeroed_box);
        let leaf_table = sub_table.0[(addr >> 10 & 0x7FF) as usize].get_or_insert_with(zeroed_box);
        let mut mask = rw.bits() as usize;
        for i in 0..size.trailing_zeros() {
            mask |= mask << (2 << i);
        }
        mask <<= (addr & MWLT_BYTES_PER_ENTRY_MASK) << 1;
        for addr in (addr..addr + size as u32).step_by(1 << MWLT_BYTES_PER_ENTRY_SHIFT) {
            let leaf_i = (addr >> MWLT_BYTES_PER_ENTRY_SHIFT & MWLT_ENTRY_COUNT_MASK) as usize;
            leaf_table.0[leaf_i] |= mask;
        }
    }

    pub(super) fn remove(&mut self, addr: u32, size: u8, rw: MemWatchpointRwMask) {
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
        let mut mask = rw.bits() as usize;
        for i in 0..size.trailing_zeros() {
            mask |= mask << (2 << i);
        }
        mask = !(mask << ((addr & MWLT_BYTES_PER_ENTRY_MASK) << 1));
        for addr in (addr..addr + size as u32).step_by(1 << MWLT_BYTES_PER_ENTRY_SHIFT) {
            let leaf_i = (addr >> MWLT_BYTES_PER_ENTRY_SHIFT & MWLT_ENTRY_COUNT_MASK) as usize;
            leaf_table.0[leaf_i] &= mask;
            if leaf_table.0[leaf_i] != 0 {
                return;
            }
        }
        if leaf_table.0.iter().any(|value| *value != 0) {
            return;
        }
        sub_table.0[sub_i] = None;
        if sub_table.0.iter().any(Option::is_some) {
            return;
        }
        self.0[root_i] = None;
    }

    pub(super) fn is_free(
        &self,
        (start_addr, end_addr): (u32, u32),
        rw: MemWatchpointRwMask,
    ) -> bool {
        // NOTE: start_addr and end_addr are assumed to be aligned to at least 32 bytes (which is
        // true in practice as this method is only used to check entire bus pointer pages).
        let mut mask = rw.bits() as usize;
        for i in 0..MWLT_BYTES_PER_ENTRY_SHIFT {
            mask |= mask << (2 << i);
        }
        for root_i in start_addr >> 21..=end_addr >> 21 {
            if let Some(sub_table) = &self.0[root_i as usize] {
                let (sub_table_start_addr, sub_table_end_addr) = (
                    (root_i << 21).max(start_addr),
                    (root_i << 21 | 0x1F_FC00).min(end_addr),
                );
                for sub_i in sub_table_start_addr >> 10 & 0x7FF..=sub_table_end_addr >> 10 & 0x7FF {
                    if let Some(leaf_table) = &sub_table.0[sub_i as usize] {
                        let (leaf_table_start_addr, leaf_table_end_addr) = (
                            (sub_table_start_addr | sub_i << 10).max(start_addr),
                            (sub_table_start_addr | sub_i << 10 | 0x3FF).min(end_addr),
                        );
                        for &entry in &leaf_table.0[(leaf_table_start_addr
                            >> MWLT_BYTES_PER_ENTRY_SHIFT
                            & MWLT_ENTRY_COUNT_MASK)
                            as usize
                            ..=(leaf_table_end_addr >> MWLT_BYTES_PER_ENTRY_SHIFT
                                & MWLT_ENTRY_COUNT_MASK) as usize]
                        {
                            if entry & mask != 0 {
                                return false;
                            }
                        }
                    }
                }
            }
        }
        true
    }

    pub(super) fn clear(&mut self) {
        for entry in &mut self.0 {
            *entry = None;
        }
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
pub type UndefHook<E> = Hook<dyn FnMut(&mut Emu<E>) -> bool>;
pub type PrefetchAbortHook<E> = Hook<dyn FnMut(&mut Emu<E>) -> bool>;
pub type DataAbortHook<E> = Hook<dyn FnMut(&mut Emu<E>, u32) -> bool>;
pub type BreakpointHook<E> = Hook<dyn FnMut(&mut Emu<E>, u32) -> bool>;
pub type MemWatchpointHook<E> =
    Hook<dyn FnMut(&mut Emu<E>, u32, u8, MemWatchpointTriggerCause) -> bool>;

pub(super) struct CoreData<E: Engine> {
    pub swi_hook: Option<SwiHook<E>>,
    pub undef_hook: Option<UndefHook<E>>,
    pub breakpoints: Vec<u32>,
    pub breakpoint_hook: Option<BreakpointHook<E>>,
    pub mem_watchpoint_hook: Option<MemWatchpointHook<E>>,
    pub mem_watchpoints: Box<MemWatchpointRootTable>,
}

impl<E: Engine> CoreData<E> {
    pub(super) fn new() -> Self {
        CoreData {
            swi_hook: None,
            undef_hook: None,
            breakpoints: Vec::new(),
            breakpoint_hook: None,
            mem_watchpoint_hook: None,
            mem_watchpoints: zeroed_box(),
        }
    }
}
