use super::{map_mask, MapMask};
use crate::{
    cpu::arm9::bus::timings::{Cycles as SysBusCycles, Timings as SysBusTimings},
    utils::{zeroed_box, Zero},
};

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cycles {
    pub r_n32_data: u8,
    pub r_s32_data: u8,
    pub r_n16_data: u8,
    pub w_n32_data: u8,
    pub w_s32_data: u8,
    pub w_n16_data: u8,
    pub code: u8,
}

impl Cycles {
    #[inline]
    pub(super) const fn repeat(v: u8) -> Self {
        Cycles {
            r_n32_data: v,
            r_s32_data: v,
            r_n16_data: v,
            w_n32_data: v,
            w_s32_data: v,
            w_n16_data: v,
            code: v,
        }
    }

    #[inline]
    const fn from_sys(sys_timings: SysBusCycles) -> Self {
        Cycles {
            r_n32_data: sys_timings.n32_data,
            r_s32_data: sys_timings.s32_data,
            r_n16_data: sys_timings.n16_data,
            w_n32_data: sys_timings.n32_data,
            w_s32_data: sys_timings.s32_data,
            w_n16_data: sys_timings.n16_data,
            code: sys_timings.code,
        }
    }
}

#[repr(transparent)]
pub struct Timings([Cycles; Self::ENTRIES]);

unsafe impl Zero for Timings {}

impl Timings {
    // Min. shift: 12 (the smallest possible TCM and PU region size is 4 KiB)
    // A value of 14 specifies a minimum size of 16 KiB, the size of DTCM, so it shouldn't need
    // to be lowered unless programs only partially map it.
    pub const PAGE_SHIFT: usize = 12;
    pub const PAGE_SIZE: usize = 1 << Self::PAGE_SHIFT;
    pub const PAGE_MASK: u32 = (Self::PAGE_SIZE - 1) as u32;
    pub const ENTRIES: usize = 1 << (32 - Self::PAGE_SHIFT);
    const ENTRIES_PER_SYS_ENTRY: usize = Self::ENTRIES / SysBusTimings::ENTRIES;

    pub(super) fn new_boxed() -> Box<Self> {
        zeroed_box()
    }

    #[inline]
    pub fn get(&self, addr: u32) -> Cycles {
        self.0[(addr >> Self::PAGE_SHIFT) as usize]
    }

    pub(super) fn set_cpu_local_range(
        &mut self,
        map_mask: MapMask,
        new_timings: Cycles,
        (lower_bound, upper_bound): (u32, u32),
    ) {
        debug_assert!(lower_bound & Self::PAGE_MASK == 0);
        debug_assert!(upper_bound & Self::PAGE_MASK == Self::PAGE_MASK);

        let lower_bound = (lower_bound >> Self::PAGE_SHIFT) as usize;
        let upper_bound = (upper_bound >> Self::PAGE_SHIFT) as usize;

        if map_mask & map_mask::R_CODE != 0 {
            for timings in &mut self.0[lower_bound..=upper_bound] {
                timings.code = new_timings.code;
            }
        }

        if map_mask & map_mask::R_DATA != 0 {
            for timings in &mut self.0[lower_bound..=upper_bound] {
                timings.r_n32_data = new_timings.r_n32_data;
                timings.r_s32_data = new_timings.r_s32_data;
                timings.r_n16_data = new_timings.r_n16_data;
            }
        }

        if map_mask & map_mask::W != 0 {
            for timings in &mut self.0[lower_bound..=upper_bound] {
                timings.w_n32_data = new_timings.w_n32_data;
                timings.w_s32_data = new_timings.w_s32_data;
                timings.w_n16_data = new_timings.w_n16_data;
            }
        }
    }

    pub(super) fn set_cpu_local_subrange(
        &mut self,
        map_mask: MapMask,
        new_timings: Cycles,
        region_bounds: (u32, u32),
        map_bounds: (u32, u32),
    ) {
        if map_bounds.0 > region_bounds.1 || map_bounds.1 < region_bounds.0 {
            return;
        }

        self.set_cpu_local_range(
            map_mask,
            new_timings,
            (
                map_bounds.0.max(region_bounds.0),
                map_bounds.1.min(region_bounds.1),
            ),
        );
    }

    pub(super) fn unset_cpu_local_range(
        &mut self,
        map_mask: MapMask,
        (lower_bound, upper_bound): (u32, u32),
        sys_bus_timings: &SysBusTimings,
    ) {
        debug_assert!(lower_bound & Self::PAGE_MASK == 0);
        debug_assert!(upper_bound & Self::PAGE_MASK == Self::PAGE_MASK);

        let lower_bound = (lower_bound >> Self::PAGE_SHIFT) as usize;
        let upper_bound = (upper_bound >> Self::PAGE_SHIFT) as usize;

        if map_mask & map_mask::R_CODE != 0 {
            for i in lower_bound..=upper_bound {
                let timings = &mut self.0[i];
                let sys_timings = sys_bus_timings.timings()[i / Self::ENTRIES_PER_SYS_ENTRY];
                timings.code = sys_timings.code;
            }
        }

        if map_mask & map_mask::R_DATA != 0 {
            for i in lower_bound..=upper_bound {
                let timings = &mut self.0[i];
                let sys_timings = sys_bus_timings.timings()[i / Self::ENTRIES_PER_SYS_ENTRY];
                timings.r_n32_data = sys_timings.n32_data;
                timings.r_s32_data = sys_timings.s32_data;
                timings.r_n16_data = sys_timings.n16_data;
            }
        }

        if map_mask & map_mask::W != 0 {
            for i in lower_bound..=upper_bound {
                let timings = &mut self.0[i];
                let sys_timings = sys_bus_timings.timings()[i / Self::ENTRIES_PER_SYS_ENTRY];
                timings.w_n32_data = sys_timings.n32_data;
                timings.w_s32_data = sys_timings.s32_data;
                timings.w_n16_data = sys_timings.n16_data;
            }
        }
    }

    pub(super) fn unset_cpu_local_subrange(
        &mut self,
        map_mask: MapMask,
        region_bounds: (u32, u32),
        map_bounds: (u32, u32),
        sys_bus_timings: &SysBusTimings,
    ) {
        if map_bounds.0 > region_bounds.1 || map_bounds.1 < region_bounds.0 {
            return;
        }

        self.unset_cpu_local_range(
            map_mask,
            (
                map_bounds.0.max(region_bounds.0),
                map_bounds.1.min(region_bounds.1),
            ),
            sys_bus_timings,
        );
    }

    pub(super) fn copy_sys_bus(&mut self, sys_bus_timings: &SysBusTimings) {
        let mut i = 0;
        for &sys_timings in sys_bus_timings.timings() {
            let timings = Cycles::from_sys(sys_timings);
            self.0[i..i + Self::ENTRIES_PER_SYS_ENTRY].fill(timings);
            i += Self::ENTRIES_PER_SYS_ENTRY;
        }
    }
}
