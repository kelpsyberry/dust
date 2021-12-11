#[cfg(feature = "bft-r")]
use crate::cpu::bus::{r_disable_flags, RDisableFlags};
#[cfg(feature = "bft-w")]
use crate::cpu::bus::{w_disable_flags, WDisableFlags};
use crate::{
    cpu::Engine,
    emu::Emu,
    utils::{zeroed_box, Zero},
};
use cfg_if::cfg_if;
use core::ptr;

cfg_if! {
    if #[cfg(any(feature = "bft-r", feature = "bft-w"))] {
        type Attrs = u8;
        mod attrs {
            use super::Attrs;
            use cfg_if::cfg_if;

            #[allow(unused)]
            pub const R_SHIFT: u32 = 0;
            #[allow(unused)]
            pub const R: Attrs = 1 << R_SHIFT;

            #[allow(unused)]
            pub const W_SHIFT: u32 = 1;
            #[allow(unused)]
            pub const W: Attrs = 1 << W_SHIFT;

            pub const MAPPED_SHIFT: u32 = 2;
            pub const MAPPED: Attrs = 1 << MAPPED_SHIFT;

            cfg_if! {
                if #[cfg(feature = "bft-r")] {
                    pub const R_DISABLE_START: u32 = 5;
                    pub const R_DISABLE_ALL: Attrs = super::r_disable_flags::ALL << R_DISABLE_START;
                }
            }
            cfg_if! {
                if #[cfg(feature = "bft-w")] {
                    pub const W_DISABLE_START: u32 = 6;
                    pub const W_DISABLE_ALL: Attrs = super::w_disable_flags::ALL << W_DISABLE_START;
                }
            }
        }
    }
}

#[repr(C)]
pub struct Ptrs {
    ptrs: [*mut u8; Self::ENTRIES],
    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    attrs: [Attrs; Self::ENTRIES],
}

unsafe impl Zero for Ptrs {}

macro_rules! def_ptr_getters {
    ($($fn_ident: ident, $ty: ty, $attr_ident: ident, $feature: literal);*$(;)?) => {
        $(
            #[cfg(not(feature = $feature))]
            #[inline]
            pub fn $fn_ident(&self, addr: u32) -> Option<$ty> {
                let ptr = self.ptrs[(addr >> Self::PAGE_SHIFT) as usize];
                if ptr.is_null() {
                    None
                } else {
                    Some(ptr)
                }
            }

            #[cfg(feature = $feature)]
            #[inline]
            pub fn $fn_ident(&self, addr: u32) -> Option<$ty> {
                let i = (addr >> Self::PAGE_SHIFT) as usize;
                if self.attrs[i] & attrs::$attr_ident == 0 {
                    None
                } else {
                    Some(self.ptrs[i])
                }
            }
        )*
    };
}

impl Ptrs {
    // The smallest possible block size is 16 KiB (used by SWRAM)
    pub const PAGE_SHIFT: usize = 14;
    pub const PAGE_SIZE: usize = 1 << Self::PAGE_SHIFT;
    pub const PAGE_MASK: u32 = (Self::PAGE_SIZE - 1) as u32;
    pub const ENTRIES: usize = 1 << (32 - Self::PAGE_SHIFT);

    pub(in super::super) fn new_boxed() -> Box<Self> {
        zeroed_box()
    }

    def_ptr_getters! {
        read, *const u8, R, "bft-r";
        write, *mut u8, W, "bft-w";
    }

    #[cfg(feature = "bft-r")]
    pub fn disable_read(&mut self, addr: u32, flags: RDisableFlags) {
        debug_assert!(flags != 0 && flags & !r_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        self.attrs[i] = (self.attrs[i] & !attrs::R) | flags << attrs::R_DISABLE_START;
    }

    #[cfg(feature = "bft-r")]
    pub fn enable_read(&mut self, addr: u32, flags: RDisableFlags) {
        debug_assert!(flags != 0 && flags & !r_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        let mut attrs = self.attrs[i];
        attrs &= !(flags << attrs::R_DISABLE_START);
        if attrs & attrs::R_DISABLE_ALL == 0 {
            attrs |= (attrs & attrs::MAPPED) >> (attrs::MAPPED_SHIFT - attrs::R_SHIFT);
        }
        self.attrs[i] = attrs;
    }

    #[cfg(feature = "bft-w")]
    pub fn disable_write(&mut self, addr: u32, flags: WDisableFlags) {
        debug_assert!(flags != 0 && flags & !w_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        self.attrs[i] = (self.attrs[i] & !attrs::W) | flags << attrs::W_DISABLE_START;
    }

    #[cfg(feature = "bft-w")]
    pub fn enable_write(&mut self, addr: u32, flags: WDisableFlags) {
        debug_assert!(flags != 0 && flags & !w_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        let mut attrs = self.attrs[i];
        attrs &= !(flags << attrs::W_DISABLE_START);
        if attrs & attrs::W_DISABLE_ALL == 0 {
            attrs |= (attrs & attrs::MAPPED) >> (attrs::MAPPED_SHIFT - attrs::W_SHIFT);
        }
        self.attrs[i] = attrs;
    }

    pub unsafe fn map_range(
        &mut self,
        start_ptr: *mut u8,
        mem_size: usize,
        (lower_bound, upper_bound): (u32, u32),
    ) {
        debug_assert!(lower_bound & Self::PAGE_MASK == 0);
        debug_assert!(upper_bound & Self::PAGE_MASK == Self::PAGE_MASK);
        debug_assert!(mem_size & Self::PAGE_MASK as usize == 0);
        let lower_bound = (lower_bound >> Self::PAGE_SHIFT) as usize;
        let upper_bound = (upper_bound >> Self::PAGE_SHIFT) as usize;
        let end_ptr = start_ptr.add(mem_size);
        let mut cur_ptr = start_ptr;
        for i in lower_bound..=upper_bound {
            self.ptrs[i] = cur_ptr;
            cur_ptr = cur_ptr.add(Self::PAGE_SIZE);
            if cur_ptr >= end_ptr {
                cur_ptr = start_ptr;
            }
        }
        #[cfg(any(feature = "bft-r", feature = "bft-w"))]
        for i in lower_bound..=upper_bound {
            let mut attrs = self.attrs[i];
            attrs |= attrs::MAPPED;
            #[cfg(feature = "bft-r")]
            if attrs & attrs::R_DISABLE_ALL == 0 {
                attrs |= attrs::R;
            }
            #[cfg(feature = "bft-w")]
            if attrs & attrs::W_DISABLE_ALL == 0 {
                attrs |= attrs::W;
            }
            self.attrs[i] = attrs;
        }
    }

    pub fn unmap_range(&mut self, (lower_bound, upper_bound): (u32, u32)) {
        debug_assert!(lower_bound & Self::PAGE_MASK == 0);
        debug_assert!(upper_bound & Self::PAGE_MASK == Self::PAGE_MASK);
        let lower_bound = (lower_bound >> Self::PAGE_SHIFT) as usize;
        let upper_bound = (upper_bound >> Self::PAGE_SHIFT) as usize;
        #[cfg(not(all(feature = "bft-r", feature = "bft-w")))]
        self.ptrs[lower_bound..=upper_bound].fill(ptr::null_mut());
        #[cfg(any(feature = "bft-r", feature = "bft-w"))]
        for attrs in &mut self.attrs[lower_bound..=upper_bound] {
            *attrs &= !(attrs::MAPPED | attrs::R | attrs::W);
        }
    }

    pub fn setup<E: Engine>(emu: &mut Emu<E>) {
        unsafe {
            emu.arm7.bus_ptrs.map_range(
                emu.main_mem().as_ptr(),
                0x40_0000,
                (0x0200_0000, 0x02FF_FFFF),
            );
            emu.arm7.bus_ptrs.map_range(
                emu.arm7.wram.as_ptr(),
                0x1_0000,
                (0x0380_0000, 0x03FF_FFFF),
            );
        }
    }
}
