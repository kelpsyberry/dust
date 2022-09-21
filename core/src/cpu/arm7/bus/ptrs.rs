#[cfg(feature = "bft-r")]
use crate::cpu::bus::{r_disable_flags, RDisableFlags};
#[cfg(feature = "bft-w")]
use crate::cpu::bus::{w_disable_flags, WDisableFlags};

pub type Mask = u8;
pub mod mask {
    use super::Mask;
    pub const R: Mask = 1 << 0;
    pub const W: Mask = 1 << 1;
    pub const ALL: Mask = R | W;
}

type Attrs = u8;
mod attrs {
    cfg_if::cfg_if! {
        if #[cfg(any(feature = "bft-r", feature = "bft-w"))] {
            use super::{mask, Attrs};

            pub const BAK_MASK_START: u32 = 2;
            #[allow(unused)]
            pub const BAK_MASK_R: Attrs = mask::R << BAK_MASK_START;
            #[allow(unused)]
            pub const BAK_MASK_W: Attrs = mask::W << BAK_MASK_START;
            pub const BAK_MASK_ALL: Attrs = mask::ALL << BAK_MASK_START;

            cfg_if::cfg_if! {
                if #[cfg(feature = "bft-r")] {
                    pub const R_DISABLE_START: u32 = 5;
                    pub const R_DISABLE_ALL: Attrs = super::r_disable_flags::ALL << R_DISABLE_START;
                }
            }
            cfg_if::cfg_if! {
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
    attrs: [Attrs; Self::ENTRIES],
}

macro_rules! def_ptr_getters {
    ($($fn_ident: ident, $ty: ty, $mask_ident: ident);*$(;)?) => {
        $(
            #[inline]
            pub fn $fn_ident(&self, addr: u32) -> Option<$ty> {
                let i = (addr >> Self::PAGE_SHIFT) as usize;
                if self.attrs[i] & mask::$mask_ident == 0 {
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
        unsafe { Box::new_zeroed().assume_init() }
    }

    def_ptr_getters! {
        read, *const u8, R;
        write, *mut u8, W;
    }

    #[cfg(feature = "bft-r")]
    pub fn disable_read(&mut self, addr: u32, flags: RDisableFlags) {
        debug_assert!(flags != 0 && flags & !r_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        self.attrs[i] = (self.attrs[i] & !mask::R) | flags << attrs::R_DISABLE_START;
    }

    #[cfg(feature = "bft-r")]
    pub fn enable_read(&mut self, addr: u32, flags: RDisableFlags) {
        debug_assert!(flags != 0 && flags & !r_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        let mut attrs = self.attrs[i];
        attrs &= !(flags << attrs::R_DISABLE_START);
        if attrs & attrs::R_DISABLE_ALL == 0 {
            attrs |= (attrs & attrs::BAK_MASK_R) >> attrs::BAK_MASK_START;
        }
        self.attrs[i] = attrs;
    }

    #[cfg(feature = "bft-r")]
    pub fn enable_read_all(&mut self, flags: RDisableFlags) {
        debug_assert!(flags != 0 && flags & !r_disable_flags::ALL == 0);
        let disable_attrs_mask = !(flags << attrs::R_DISABLE_START);
        for i in 0..Self::ENTRIES {
            let mut attrs = self.attrs[i];
            attrs &= disable_attrs_mask;
            if attrs & attrs::R_DISABLE_ALL == 0 {
                attrs |= (attrs & attrs::BAK_MASK_R) >> attrs::BAK_MASK_START;
            }
            self.attrs[i] = attrs;
        }
    }

    #[cfg(feature = "bft-w")]
    pub fn disable_write(&mut self, addr: u32, flags: WDisableFlags) {
        debug_assert!(flags != 0 && flags & !w_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        self.attrs[i] = (self.attrs[i] & !mask::W) | flags << attrs::W_DISABLE_START;
    }

    #[cfg(feature = "bft-w")]
    pub fn enable_write(&mut self, addr: u32, flags: WDisableFlags) {
        debug_assert!(flags != 0 && flags & !w_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        let mut attrs = self.attrs[i];
        attrs &= !(flags << attrs::W_DISABLE_START);
        if attrs & attrs::W_DISABLE_ALL == 0 {
            attrs |= (attrs & attrs::BAK_MASK_W) >> attrs::BAK_MASK_START;
        }
        self.attrs[i] = attrs;
    }

    #[cfg(feature = "bft-w")]
    pub fn enable_write_all(&mut self, flags: WDisableFlags) {
        debug_assert!(flags != 0 && flags & !w_disable_flags::ALL == 0);
        let disable_attrs_mask = !(flags << attrs::W_DISABLE_START);
        for i in 0..Self::ENTRIES {
            let mut attrs = self.attrs[i];
            attrs &= disable_attrs_mask;
            if attrs & attrs::W_DISABLE_ALL == 0 {
                attrs |= (attrs & attrs::BAK_MASK_W) >> attrs::BAK_MASK_START;
            }
            self.attrs[i] = attrs;
        }
    }

    pub unsafe fn map_range(
        &mut self,
        mask: Mask,
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

        #[cfg(not(any(feature = "bft-r", feature = "bft-w")))]
        self.attrs[lower_bound..=upper_bound].fill(mask);

        #[cfg(any(feature = "bft-r", feature = "bft-w"))]
        {
            let read_mask_attrs = mask & mask::R;
            let write_mask_attrs = mask & mask::W;
            let bak_mask_attrs = mask << attrs::BAK_MASK_START;
            for i in lower_bound..=upper_bound {
                let mut attrs = self.attrs[i];
                attrs = (attrs & !attrs::BAK_MASK_ALL) | bak_mask_attrs;
                let r_not_disabled = {
                    #[cfg(feature = "bft-r")]
                    {
                        attrs & attrs::R_DISABLE_ALL == 0
                    }
                    #[cfg(not(feature = "bft-r"))]
                    true
                };
                if r_not_disabled {
                    attrs = (attrs & !mask::R) | read_mask_attrs;
                }
                let w_not_disabled = {
                    #[cfg(feature = "bft-w")]
                    {
                        attrs & attrs::W_DISABLE_ALL == 0
                    }
                    #[cfg(not(feature = "bft-w"))]
                    true
                };
                if w_not_disabled {
                    attrs = (attrs & !mask::W) | write_mask_attrs;
                }
                self.attrs[i] = attrs;
            }
        }
    }
}
