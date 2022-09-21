// Rustc seems to have an original definition of "dead code", which just happens not to coincide
// with actually dead code at the time of adding this.
#![allow(dead_code)]

use super::{map_mask, MapMask};
#[cfg(any(feature = "bft-r", feature = "bft-w"))]
use crate::cpu::arm9::bus::ptrs::attrs as sys_bus_attrs;
use crate::cpu::arm9::bus::ptrs::{
    mask as sys_bus_mask, Attrs as SysBusAttrs, Mask as SysBusMask, Ptrs as SysBusPtrs,
};
#[cfg(feature = "bft-r")]
use crate::cpu::bus::{r_disable_flags, RDisableFlags};
#[cfg(feature = "bft-w")]
use crate::cpu::bus::{w_disable_flags, WDisableFlags};

type Attrs = u8;
mod attrs {
    // R/X/W8/W16_32 mask in bits 0-3

    cfg_if::cfg_if! {
        if #[cfg(any(feature = "bft-r", feature = "bft-w"))] {
            use super::{mask, Attrs};

            pub const BAK_MASK_START: u32 = 4;
            pub const BAK_MASK_R_CODE: Attrs = mask::R_CODE << BAK_MASK_START;
            pub const BAK_MASK_R_DATA: Attrs = mask::R_DATA << BAK_MASK_START;
            pub const BAK_MASK_R: Attrs = mask::R_ALL << BAK_MASK_START;
            pub const BAK_MASK_W: Attrs = mask::W_ALL << BAK_MASK_START;
            pub const BAK_MASK_ALL: Attrs = mask::ALL << BAK_MASK_START;
        }
    }
}

const fn sys_attrs_to_attrs(sys_attrs: SysBusAttrs) -> Attrs {
    #[cfg(not(any(feature = "bft-r", feature = "bft-w")))]
    {
        sys_attrs << 1 | (sys_attrs & sys_bus_mask::R)
    }
    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    {
        (sys_attrs & sys_bus_mask::R)
            | (sys_attrs & (sys_bus_mask::ALL | sys_bus_attrs::BAK_MASK_R)) << 1
            | (sys_attrs & sys_bus_attrs::BAK_MASK_ALL) << 2
    }
}

type MapAttrs = u8;
mod map_attrs {
    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    use super::MapAttrs;

    // "R/W/X mapped to CPU-local memory" mask in bits 0-2

    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    pub const DISABLE_START: u32 = 3;

    cfg_if::cfg_if! {
        if #[cfg(feature = "bft-r")] {
            pub const R_DISABLE_START: u32 = DISABLE_START;
            pub const R_DISABLE_ALL: MapAttrs = super::r_disable_flags::ALL << R_DISABLE_START;
        }
    }
    cfg_if::cfg_if! {
        if #[cfg(feature = "bft-w")] {
            pub const W_DISABLE_START: u32 = DISABLE_START + 1;
            pub const W_DISABLE_ALL: MapAttrs = super::w_disable_flags::ALL << W_DISABLE_START;
        }
    }
}

type Mask = u8;
mod mask {
    use super::Mask;
    pub const R_CODE: Mask = 1 << 0;
    pub const R_DATA: Mask = 1 << 1;
    pub const R_ALL: Mask = R_CODE | R_DATA;
    pub const W_8: Mask = 1 << 2;
    pub const W_16_32: Mask = 1 << 3;
    pub const W_ALL: Mask = W_8 | W_16_32;
    pub const ALL: Mask = R_ALL | W_ALL;
}

#[repr(C)]
pub struct Ptrs {
    r_code_ptrs: [*const u8; Self::ENTRIES],
    r_data_ptrs: [*const u8; Self::ENTRIES],
    w_ptrs: [*mut u8; Self::ENTRIES],
    attrs: [Attrs; Self::ENTRIES],
    map_attrs: [MapAttrs; Self::ENTRIES],
}

macro_rules! def_ptr_getters {
    ($($fn_ident: ident, $ty: ty, $ptr_arr: ident, $mask_ident: ident);*$(;)?) => {
        $(
            #[inline]
            pub fn $fn_ident(&self, addr: u32) -> Option<$ty> {
                let i = (addr >> Self::PAGE_SHIFT) as usize;
                if self.attrs[i] & mask::$mask_ident == 0 {
                    None
                } else {
                    Some(self.$ptr_arr[i])
                }
            }
        )*
    };
}

impl Ptrs {
    // Min. shift: 12 (the smallest possible TCM size is 4 KiB)
    // A value of 14 specifies a minimum size of 16 KiB, the size of DTCM, so it shouldn't need
    // to be lowered unless programs only partially map it.
    pub const PAGE_SHIFT: usize = 12;
    pub const PAGE_SIZE: usize = 1 << Self::PAGE_SHIFT;
    pub const PAGE_MASK: u32 = (Self::PAGE_SIZE - 1) as u32;
    pub const ENTRIES: usize = 1 << (32 - Self::PAGE_SHIFT);
    const ENTRIES_PER_SYS_ENTRY: usize = Self::ENTRIES / SysBusPtrs::ENTRIES;

    pub(super) fn new_boxed() -> Box<Self> {
        unsafe { Box::new_zeroed().assume_init() }
    }

    def_ptr_getters! {
        read_code, *const u8, r_code_ptrs, R_CODE;
        read_data, *const u8, r_data_ptrs, R_DATA;
        write_8, *mut u8, w_ptrs, W_8;
        write_16_32, *mut u8, w_ptrs, W_16_32;
    }

    #[cfg(feature = "bft-r")]
    pub fn disable_read(&mut self, addr: u32, flags: RDisableFlags) {
        debug_assert!(flags != 0 && flags & !r_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        self.map_attrs[i] |= flags << map_attrs::R_DISABLE_START;
        self.attrs[i] &= !mask::R_ALL;
    }

    #[cfg(feature = "bft-r")]
    pub fn enable_read(&mut self, addr: u32, flags: RDisableFlags) {
        debug_assert!(flags != 0 && flags & !r_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        let map_attrs = self.map_attrs[i] & !(flags << map_attrs::R_DISABLE_START);
        if map_attrs & map_attrs::R_DISABLE_ALL == 0 {
            let attrs = self.attrs[i];
            self.attrs[i] = attrs | (attrs & attrs::BAK_MASK_R) >> attrs::BAK_MASK_START;
        }
        self.map_attrs[i] = map_attrs;
    }

    #[cfg(feature = "bft-r")]
    pub fn enable_read_all(&mut self, flags: RDisableFlags) {
        debug_assert!(flags != 0 && flags & !r_disable_flags::ALL == 0);
        let disable_attrs_mask = !(flags << map_attrs::R_DISABLE_START);
        for i in 0..Self::ENTRIES {
            let map_attrs = self.map_attrs[i] & disable_attrs_mask;
            if map_attrs & map_attrs::R_DISABLE_ALL == 0 {
                let attrs = self.attrs[i];
                self.attrs[i] = attrs | (attrs & attrs::BAK_MASK_R) >> attrs::BAK_MASK_START;
            }
            self.map_attrs[i] = map_attrs;
        }
    }

    #[cfg(feature = "bft-w")]
    pub fn disable_write(&mut self, addr: u32, flags: WDisableFlags) {
        debug_assert!(flags != 0 && flags & !w_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        self.map_attrs[i] |= flags << map_attrs::W_DISABLE_START;
        self.attrs[i] &= !mask::W_ALL;
    }

    #[cfg(feature = "bft-w")]
    pub fn enable_write(&mut self, addr: u32, flags: Attrs) {
        debug_assert!(flags != 0 && flags & !w_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        let map_attrs = self.map_attrs[i] & !(flags << map_attrs::W_DISABLE_START);
        if map_attrs & map_attrs::W_DISABLE_ALL == 0 {
            let attrs = self.attrs[i];
            self.attrs[i] = attrs | (attrs & attrs::BAK_MASK_W) >> attrs::BAK_MASK_START;
        }
        self.map_attrs[i] = map_attrs;
    }

    #[cfg(feature = "bft-w")]
    pub fn enable_write_all(&mut self, flags: Attrs) {
        debug_assert!(flags != 0 && flags & !w_disable_flags::ALL == 0);
        let disable_attrs_mask = !(flags << map_attrs::W_DISABLE_START);
        for i in 0..Self::ENTRIES {
            let map_attrs = self.map_attrs[i] & disable_attrs_mask;
            if map_attrs & map_attrs::W_DISABLE_ALL == 0 {
                let attrs = self.attrs[i];
                self.attrs[i] = attrs | (attrs & attrs::BAK_MASK_W) >> attrs::BAK_MASK_START;
            }
            self.map_attrs[i] = map_attrs;
        }
    }

    unsafe fn map_cpu_local_range_inner(
        &mut self,
        map_mask: MapMask,
        map_start_ptr: *mut u8,
        start_ptr: *mut u8,
        end_ptr: *mut u8,
        (lower_bound, upper_bound): (u32, u32),
    ) {
        debug_assert!(map_mask & !map_mask::ALL == 0);
        debug_assert!(lower_bound & Self::PAGE_MASK == 0);
        debug_assert!(upper_bound & Self::PAGE_MASK == Self::PAGE_MASK);

        let lower_bound = (lower_bound >> Self::PAGE_SHIFT) as usize;
        let upper_bound = (upper_bound >> Self::PAGE_SHIFT) as usize;

        macro_rules! repeat_ptrs {
            (|$elem_ident: ident, $arr_ident: ident, $ptr_ident: ident| $block: block) => {{
                let mut $ptr_ident = map_start_ptr;
                for $elem_ident in &mut self.$arr_ident[lower_bound..=upper_bound] {
                    $block;
                    $ptr_ident = $ptr_ident.add(Self::PAGE_SIZE);
                    if $ptr_ident >= end_ptr {
                        $ptr_ident = start_ptr;
                    }
                }
            }};
        }

        let mut attrs_mask = 0;

        if map_mask & map_mask::R_CODE != 0 {
            repeat_ptrs!(|r_code_ptr, r_code_ptrs, ptr| {
                *r_code_ptr = ptr;
            });
            #[cfg(not(feature = "bft-r"))]
            {
                attrs_mask |= mask::R_CODE;
            }
            #[cfg(feature = "bft-r")]
            {
                attrs_mask |= mask::R_CODE << attrs::BAK_MASK_START;
                for i in lower_bound..=upper_bound {
                    if self.map_attrs[i] & map_attrs::R_DISABLE_ALL == 0 {
                        self.attrs[i] |= mask::R_CODE;
                    }
                }
            }
        }

        if map_mask & map_mask::R_DATA != 0 {
            repeat_ptrs!(|r_data_ptr, r_data_ptrs, ptr| {
                *r_data_ptr = ptr;
            });
            #[cfg(not(feature = "bft-r"))]
            {
                attrs_mask |= mask::R_DATA;
            }
            #[cfg(feature = "bft-r")]
            {
                attrs_mask |= mask::R_DATA << attrs::BAK_MASK_START;
                for i in lower_bound..=upper_bound {
                    if self.map_attrs[i] & map_attrs::R_DISABLE_ALL == 0 {
                        self.attrs[i] |= mask::R_DATA;
                    }
                }
            }
        }

        if map_mask & map_mask::W != 0 {
            repeat_ptrs!(|write_ptr, w_ptrs, ptr| {
                *write_ptr = ptr;
            });
            #[cfg(not(feature = "bft-w"))]
            {
                attrs_mask |= mask::W_ALL;
            }
            #[cfg(feature = "bft-w")]
            {
                attrs_mask |= mask::W_ALL << attrs::BAK_MASK_START;
                for i in lower_bound..=upper_bound {
                    if self.map_attrs[i] & map_attrs::W_DISABLE_ALL == 0 {
                        self.attrs[i] |= mask::W_ALL;
                    }
                }
            }
        }

        for (attrs, map_attrs) in self.attrs[lower_bound..=upper_bound]
            .iter_mut()
            .zip(&mut self.map_attrs[lower_bound..=upper_bound])
        {
            *attrs |= attrs_mask;
            *map_attrs |= map_mask;
        }
    }

    pub(super) unsafe fn map_cpu_local_range(
        &mut self,
        map_mask: MapMask,
        start_ptr: *mut u8,
        mem_size: usize,
        bounds: (u32, u32),
    ) {
        debug_assert!(mem_size & Self::PAGE_MASK as usize == 0);
        self.map_cpu_local_range_inner(
            map_mask,
            start_ptr,
            start_ptr,
            start_ptr.add(mem_size),
            bounds,
        );
    }

    pub(super) unsafe fn map_cpu_local_subrange(
        &mut self,
        map_mask: MapMask,
        start_ptr: *mut u8,
        mem_size: usize,
        region_bounds: (u32, u32),
        map_bounds: (u32, u32),
    ) {
        debug_assert!(mem_size.is_power_of_two() && mem_size & Self::PAGE_MASK as usize == 0);

        if map_bounds.0 > region_bounds.1 || map_bounds.1 < region_bounds.0 {
            return;
        }

        let (map_start_ptr, adj_lower_bound) = if map_bounds.0 <= region_bounds.0 {
            (start_ptr, region_bounds.0)
        } else {
            (
                start_ptr.add((map_bounds.0 & (mem_size - 1) as u32) as usize),
                map_bounds.0,
            )
        };

        self.map_cpu_local_range_inner(
            map_mask,
            map_start_ptr,
            start_ptr,
            start_ptr.add(mem_size),
            (adj_lower_bound, map_bounds.1.min(region_bounds.1)),
        );
    }

    pub(super) fn unmap_cpu_local_range(
        &mut self,
        map_mask: MapMask,
        (lower_bound, upper_bound): (u32, u32),
        sys_bus_ptrs: &SysBusPtrs,
    ) {
        debug_assert!(map_mask & !map_mask::ALL == 0);
        debug_assert!(lower_bound & Self::PAGE_MASK == 0);
        debug_assert!(upper_bound & Self::PAGE_MASK == Self::PAGE_MASK);

        let lower_bound = (lower_bound >> Self::PAGE_SHIFT) as usize;
        let upper_bound = (upper_bound >> Self::PAGE_SHIFT) as usize;

        let mut attrs_mask = 0;

        macro_rules! map_sys_bus_ptrs {
            ($arr_ident: ident) => {{
                for i in lower_bound..=upper_bound {
                    self.$arr_ident[i] = unsafe {
                        sys_bus_ptrs.ptrs()[i / Self::ENTRIES_PER_SYS_ENTRY]
                            .add((i & (Self::ENTRIES_PER_SYS_ENTRY - 1)) << Self::PAGE_SHIFT)
                    } as _;
                }
            }};
        }

        if map_mask & map_mask::R_CODE != 0 {
            attrs_mask |= mask::R_CODE;
            map_sys_bus_ptrs!(r_code_ptrs);
        }

        if map_mask & map_mask::R_DATA != 0 {
            attrs_mask |= mask::R_DATA;
            map_sys_bus_ptrs!(r_data_ptrs);
        }

        if map_mask & map_mask::W != 0 {
            attrs_mask |= mask::W_ALL;
            map_sys_bus_ptrs!(w_ptrs);
        }

        for map_attrs in &mut self.map_attrs[lower_bound..=upper_bound] {
            *map_attrs &= !map_mask;
        }

        if map_mask == map_mask::ALL {
            for i in lower_bound..=upper_bound {
                self.attrs[i] =
                    sys_attrs_to_attrs(sys_bus_ptrs.attrs()[i / Self::ENTRIES_PER_SYS_ENTRY]);
            }
        } else {
            #[cfg(any(feature = "bft-r", feature = "bft-w"))]
            {
                attrs_mask |= attrs_mask << attrs::BAK_MASK_START;
            }
            for i in lower_bound..=upper_bound {
                let new_attrs =
                    sys_attrs_to_attrs(sys_bus_ptrs.attrs()[i / Self::ENTRIES_PER_SYS_ENTRY]);
                self.attrs[i] = (self.attrs[i] & !attrs_mask) | (new_attrs & attrs_mask);
            }
        }
    }

    pub(in super::super) unsafe fn map_sys_bus_range(
        &mut self,
        start_ptr: *mut u8,
        mem_size: usize,
        (lower_bound, upper_bound): (u32, u32),
        sys_bus_mask: SysBusMask,
    ) {
        debug_assert!(sys_bus_mask & !sys_bus_mask::ALL == 0);
        debug_assert!(lower_bound & Self::PAGE_MASK == 0);
        debug_assert!(upper_bound & Self::PAGE_MASK == Self::PAGE_MASK);
        debug_assert!(mem_size & Self::PAGE_MASK as usize == 0);

        let lower_bound = (lower_bound >> Self::PAGE_SHIFT) as usize;
        let upper_bound = (upper_bound >> Self::PAGE_SHIFT) as usize;

        let end_ptr = start_ptr.add(mem_size);

        if sys_bus_mask & sys_bus_mask::R == 0 {
            for i in lower_bound..=upper_bound {
                let map_attrs = self.map_attrs[i];

                if map_attrs & map_mask::R_CODE == 0 {
                    #[cfg(not(feature = "bft-r"))]
                    {
                        self.attrs[i] &= !mask::R_CODE;
                    }
                    #[cfg(feature = "bft-r")]
                    {
                        self.attrs[i] &= !(mask::R_CODE | attrs::BAK_MASK_R_CODE);
                    }
                }
                if map_attrs & map_mask::R_DATA == 0 {
                    #[cfg(not(feature = "bft-r"))]
                    {
                        self.attrs[i] &= !mask::R_DATA;
                    }
                    #[cfg(feature = "bft-r")]
                    {
                        self.attrs[i] &= !(mask::R_DATA | attrs::BAK_MASK_R_DATA);
                    }
                }
            }
        } else {
            let mut cur_ptr = start_ptr;
            for i in lower_bound..=upper_bound {
                let map_attrs = self.map_attrs[i];

                if map_attrs & map_mask::R_CODE == 0 {
                    self.r_code_ptrs[i] = cur_ptr;
                    #[cfg(not(feature = "bft-r"))]
                    {
                        self.attrs[i] |= mask::R_CODE;
                    }
                    #[cfg(feature = "bft-r")]
                    {
                        let mut attrs = self.attrs[i];
                        attrs |= mask::R_CODE << attrs::BAK_MASK_START;
                        if map_attrs & map_attrs::R_DISABLE_ALL == 0 {
                            attrs |= mask::R_CODE;
                        }
                        self.attrs[i] = attrs;
                    }
                }

                if map_attrs & map_mask::R_DATA == 0 {
                    self.r_data_ptrs[i] = cur_ptr;
                    #[cfg(not(feature = "bft-r"))]
                    {
                        self.attrs[i] |= mask::R_DATA;
                    }
                    #[cfg(feature = "bft-r")]
                    {
                        let mut attrs = self.attrs[i];
                        attrs |= mask::R_DATA << attrs::BAK_MASK_START;
                        if map_attrs & map_attrs::R_DISABLE_ALL == 0 {
                            attrs |= mask::R_DATA;
                        }
                        self.attrs[i] = attrs;
                    }
                }

                cur_ptr = cur_ptr.add(Self::PAGE_SIZE);
                if cur_ptr >= end_ptr {
                    cur_ptr = start_ptr;
                }
            }
        }

        if sys_bus_mask & sys_bus_mask::W_ALL == 0 {
            for i in lower_bound..=upper_bound {
                let map_attrs = self.map_attrs[i];

                if map_attrs & map_mask::W == 0 {
                    #[cfg(not(feature = "bft-w"))]
                    {
                        self.attrs[i] &= !mask::W_ALL;
                    }
                    #[cfg(feature = "bft-w")]
                    {
                        self.attrs[i] &= !(mask::W_ALL | attrs::BAK_MASK_W);
                    }
                }
            }
        } else {
            let write_mask_attrs = (sys_bus_mask & sys_bus_mask::W_ALL) << 1;
            #[cfg(feature = "bft-w")]
            let bak_write_mask_attrs = write_mask_attrs << attrs::BAK_MASK_START;
            let mut cur_ptr = start_ptr;
            for i in lower_bound..=upper_bound {
                let map_attrs = self.map_attrs[i];

                if map_attrs & map_mask::W == 0 {
                    self.w_ptrs[i] = cur_ptr;
                    #[cfg(not(feature = "bft-w"))]
                    {
                        self.attrs[i] = (self.attrs[i] & !mask::W_ALL) | write_mask_attrs;
                    }
                    #[cfg(feature = "bft-w")]
                    {
                        let mut attrs = self.attrs[i];
                        attrs = (attrs & !attrs::BAK_MASK_W) | bak_write_mask_attrs;
                        if map_attrs & map_attrs::W_DISABLE_ALL == 0 {
                            attrs = (attrs & !mask::W_ALL) | write_mask_attrs;
                        }
                        self.attrs[i] = attrs;
                    }
                }

                cur_ptr = cur_ptr.add(Self::PAGE_SIZE);
                if cur_ptr >= end_ptr {
                    cur_ptr = start_ptr;
                }
            }
        }
    }

    pub(in super::super) fn unmap_sys_bus_range(&mut self, (lower_bound, upper_bound): (u32, u32)) {
        debug_assert!(lower_bound & Self::PAGE_MASK == 0);
        debug_assert!(upper_bound & Self::PAGE_MASK == Self::PAGE_MASK);

        let lower_bound = (lower_bound >> Self::PAGE_SHIFT) as usize;
        let upper_bound = (upper_bound >> Self::PAGE_SHIFT) as usize;

        for i in lower_bound..=upper_bound {
            let map_attrs = self.map_attrs[i];

            if map_attrs & map_mask::R_CODE == 0 {
                #[cfg(not(feature = "bft-r"))]
                {
                    self.attrs[i] &= !mask::R_CODE;
                }
                #[cfg(feature = "bft-r")]
                {
                    self.attrs[i] &= !(mask::R_CODE | attrs::BAK_MASK_R_CODE);
                }
            }
            if map_attrs & map_mask::R_DATA == 0 {
                #[cfg(not(feature = "bft-r"))]
                {
                    self.attrs[i] &= !mask::R_DATA;
                }
                #[cfg(feature = "bft-r")]
                {
                    self.attrs[i] &= !(mask::R_DATA | attrs::BAK_MASK_R_DATA);
                }
            }
            if map_attrs & map_mask::W == 0 {
                #[cfg(not(feature = "bft-w"))]
                {
                    self.attrs[i] &= !mask::W_ALL;
                }
                #[cfg(feature = "bft-w")]
                {
                    self.attrs[i] &= !(mask::W_ALL | attrs::BAK_MASK_W);
                }
            }
        }
    }

    pub(super) fn copy_sys_bus(&mut self, sys_bus_ptrs: &SysBusPtrs) {
        {
            let mut i = 0;
            for &(mut ptr) in sys_bus_ptrs.ptrs() {
                for _ in 0..Self::ENTRIES_PER_SYS_ENTRY {
                    self.r_code_ptrs[i] = ptr;
                    self.r_data_ptrs[i] = ptr;
                    self.w_ptrs[i] = ptr;
                    ptr = unsafe { ptr.add(Self::PAGE_SIZE) };
                    i += 1;
                }
            }
        }

        {
            let mut i = 0;
            for sys_i in 0..SysBusPtrs::ENTRIES {
                let sys_attrs = sys_bus_ptrs.attrs()[sys_i];
                let attrs = sys_attrs_to_attrs(sys_attrs);

                #[cfg(all(
                    any(feature = "bft-r", feature = "bft-w"),
                    not(all(feature = "bft-r", feature = "bft-w"))
                ))]
                let fill_map_attrs = {
                    #[cfg(feature = "bft-r")]
                    {
                        (sys_attrs >> sys_bus_attrs::R_DISABLE_START & r_disable_flags::ALL)
                            << map_attrs::R_DISABLE_START
                    }
                    #[cfg(feature = "bft-w")]
                    {
                        (sys_attrs >> sys_bus_attrs::W_DISABLE_START & w_disable_flags::ALL)
                            << map_attrs::W_DISABLE_START
                    }
                };
                #[cfg(all(feature = "bft-r", feature = "bft-w"))]
                let fill_map_attrs =
                    sys_bus_ptrs.disable_attrs()[sys_i] << map_attrs::DISABLE_START;

                for _ in 0..Self::ENTRIES_PER_SYS_ENTRY {
                    self.attrs[i] = attrs;
                    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
                    {
                        self.map_attrs[i] = fill_map_attrs;
                    }
                    i += 1;
                }
            }
        }

        #[cfg(not(any(feature = "bft-r", feature = "bft-w")))]
        self.map_attrs.fill(0);
    }
}
