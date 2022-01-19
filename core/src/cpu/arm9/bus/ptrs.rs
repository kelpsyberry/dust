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

pub type Mask = u8;
pub mod mask {
    use super::Mask;
    pub const R: Mask = 1 << 0;
    pub const W_8: Mask = 1 << 1;
    pub const W_16_32: Mask = 1 << 2;
    pub const W_ALL: Mask = W_8 | W_16_32;
    pub const ALL: Mask = R | W_ALL;
}

pub(in super::super) type Attrs = u8;
pub(in super::super) mod attrs {
    use cfg_if::cfg_if;

    cfg_if! {
        if #[cfg(any(feature = "bft-r", feature = "bft-w"))] {
            use super::{mask, Attrs};

            pub const BAK_MASK_START: u32 = 3;
            #[allow(unused)]
            pub const BAK_MASK_R: Attrs = mask::R << BAK_MASK_START;
            #[allow(unused)]
            pub const BAK_MASK_W: Attrs = mask::W_ALL << BAK_MASK_START;
            pub const BAK_MASK_ALL: Attrs = mask::ALL << BAK_MASK_START;

            cfg_if! {
                if #[cfg(all(feature = "bft-r", not(feature = "bft-w")))] {
                    pub const R_DISABLE_START: u32 = 6;
                    pub const R_DISABLE_ALL: Attrs = super::r_disable_flags::ALL << R_DISABLE_START;
                } else if #[cfg(all(feature = "bft-w", not(feature = "bft-r")))] {
                    pub const W_DISABLE_START: u32 = 6;
                    pub const W_DISABLE_ALL: Attrs = super::w_disable_flags::ALL << W_DISABLE_START;
                }
            }
        }
    }
}

cfg_if! {
    if #[cfg(all(feature = "bft-r", feature = "bft-w"))] {
        type DisableAttrs = u8;
        mod disable_attrs {
            use super::{r_disable_flags, w_disable_flags, DisableAttrs};

            pub const R_DISABLE_START: u32 = 0;
            pub const R_DISABLE_ALL: DisableAttrs = r_disable_flags::ALL << R_DISABLE_START;

            pub const W_DISABLE_START: u32 = 1;
            pub const W_DISABLE_ALL: DisableAttrs = w_disable_flags::ALL << W_DISABLE_START;
        }
    }
}

#[repr(C)]
pub struct Ptrs {
    ptrs: [*mut u8; Self::ENTRIES],
    attrs: [Attrs; Self::ENTRIES],
    #[cfg(all(feature = "bft-r", feature = "bft-w"))]
    disable_attrs: [DisableAttrs; Self::ENTRIES],
}

unsafe impl Zero for Ptrs {}

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
    // The smallest possible block size is 16 KiB (used by SWRAM and VRAM, the ARM9 BIOS is
    // zero-filled to fit, with the tradeoff of preventing some unknown access warnings)
    pub const PAGE_SHIFT: usize = 14;
    pub const PAGE_SIZE: usize = 1 << Self::PAGE_SHIFT;
    pub const PAGE_MASK: u32 = (Self::PAGE_SIZE - 1) as u32;
    pub const ENTRIES: usize = 1 << (32 - Self::PAGE_SHIFT);

    pub(in super::super) fn new_boxed() -> Box<Self> {
        zeroed_box()
    }

    pub(in super::super) fn ptrs(&self) -> &[*mut u8; Self::ENTRIES] {
        &self.ptrs
    }

    pub(in super::super) fn attrs(&self) -> &[Attrs; Self::ENTRIES] {
        &self.attrs
    }

    #[cfg(all(feature = "bft-r", feature = "bft-w"))]
    pub fn disable_attrs(&self) -> &[DisableAttrs; Self::ENTRIES] {
        &self.disable_attrs
    }

    def_ptr_getters! {
        read, *const u8, R;
        write_8, *mut u8, W_8;
        write_16_32, *mut u8, W_16_32;
    }

    #[cfg(feature = "bft-r")]
    pub fn disable_read(&mut self, addr: u32, flags: RDisableFlags) {
        debug_assert!(flags != 0 && flags & !r_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        #[cfg(feature = "bft-w")]
        {
            self.disable_attrs[i] |= flags << disable_attrs::R_DISABLE_START;
        }
        let mut attrs = self.attrs[i];
        #[cfg(not(feature = "bft-w"))]
        {
            attrs |= flags << attrs::R_DISABLE_START;
        }
        attrs &= !mask::R;
        self.attrs[i] = attrs;
    }

    #[cfg(feature = "bft-r")]
    pub fn enable_read(&mut self, addr: u32, flags: RDisableFlags) {
        debug_assert!(flags != 0 && flags & !r_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        #[cfg(feature = "bft-w")]
        {
            let disable_attrs = self.disable_attrs[i] & !(flags << disable_attrs::R_DISABLE_START);
            self.disable_attrs[i] = disable_attrs;
            if disable_attrs & disable_attrs::R_DISABLE_ALL == 0 {
                self.attrs[i] |= (self.attrs[i] & attrs::BAK_MASK_R) >> attrs::BAK_MASK_START;
            }
        }
        #[cfg(not(feature = "bft-w"))]
        {
            let mut attrs = self.attrs[i] & !(flags << attrs::R_DISABLE_START);
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
        #[cfg(feature = "bft-r")]
        {
            self.disable_attrs[i] |= flags << disable_attrs::W_DISABLE_START;
        }
        let mut attrs = self.attrs[i];
        #[cfg(not(feature = "bft-r"))]
        {
            attrs |= flags << attrs::W_DISABLE_START;
        }
        attrs &= !mask::W_ALL;
        self.attrs[i] = attrs;
    }

    #[cfg(feature = "bft-w")]
    pub fn enable_write(&mut self, addr: u32, flags: WDisableFlags) {
        debug_assert!(flags != 0 && flags & !w_disable_flags::ALL == 0);
        let i = (addr >> Self::PAGE_SHIFT) as usize;
        #[cfg(feature = "bft-r")]
        {
            let disable_attrs = self.disable_attrs[i] & !(flags << disable_attrs::W_DISABLE_START);
            self.disable_attrs[i] = disable_attrs;
            if disable_attrs & disable_attrs::W_DISABLE_ALL == 0 {
                self.attrs[i] |= (self.attrs[i] & attrs::BAK_MASK_W) >> attrs::BAK_MASK_START;
            }
        }
        #[cfg(not(feature = "bft-r"))]
        {
            let mut attrs = self.attrs[i] & !(flags << attrs::W_DISABLE_START);
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
        debug_assert!(mask != 0 && mask & !mask::ALL == 0);
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
            let write_mask_attrs = mask & mask::W_ALL;
            let bak_mask_attrs = mask << attrs::BAK_MASK_START;
            for i in lower_bound..=upper_bound {
                let mut attrs = self.attrs[i];
                attrs = (attrs & !attrs::BAK_MASK_ALL) | bak_mask_attrs;
                #[cfg(not(all(feature = "bft-r", feature = "bft-w")))]
                {
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
                }
                #[cfg(all(feature = "bft-r", feature = "bft-w"))]
                {
                    let disable_attrs = self.disable_attrs[i];
                    if disable_attrs & disable_attrs::R_DISABLE_ALL == 0 {
                        attrs = (attrs & !mask::R) | read_mask_attrs;
                    }
                    if disable_attrs & disable_attrs::W_DISABLE_ALL == 0 {
                        attrs = (attrs & !mask::W_ALL) | write_mask_attrs;
                    }
                }
                self.attrs[i] = attrs;
            }
        }
    }

    pub fn unmap_range(&mut self, (lower_bound, upper_bound): (u32, u32)) {
        debug_assert!(lower_bound & Self::PAGE_MASK == 0);
        debug_assert!(upper_bound & Self::PAGE_MASK == Self::PAGE_MASK);

        let lower_bound = (lower_bound >> Self::PAGE_SHIFT) as usize;
        let upper_bound = (upper_bound >> Self::PAGE_SHIFT) as usize;
        #[cfg(all(
            any(feature = "bft-r", feature = "bft-w"),
            not(all(feature = "bft-r", feature = "bft-w"))
        ))]
        for attrs in &mut self.attrs[lower_bound..=upper_bound] {
            *attrs &= !(mask::ALL | attrs::BAK_MASK_ALL);
        }
        #[cfg(any(
            not(any(feature = "bft-r", feature = "bft-w")),
            all(feature = "bft-r", feature = "bft-w"),
        ))]
        self.attrs[lower_bound..=upper_bound].fill(0);
    }

    pub fn setup<E: Engine>(emu: &mut Emu<E>) {
        unsafe {
            emu.arm9.bus_ptrs.map_range(
                mask::ALL,
                emu.main_mem().as_ptr(),
                0x40_0000,
                (0x0200_0000, 0x02FF_FFFF),
            );
            emu.gpu.vram.setup_arm9_bus_ptrs(&mut emu.arm9.bus_ptrs);
            emu.arm9.bus_ptrs.map_range(
                mask::R,
                emu.arm9.bios.as_ptr(),
                0x4000,
                (0xFFFF_0000, 0xFFFF_0000 + (emu.arm9.bios.len() - 1) as u32),
            );
        }
    }
}
