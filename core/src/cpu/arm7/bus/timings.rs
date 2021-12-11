use crate::utils::{fill_8, zeroed_box, Fill8, Zero};

#[repr(C, align(4))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cycles {
    pub n32: u8,
    pub s32: u8,
    pub n16: u8,
    pub s16: u8,
}

#[repr(transparent)]
pub struct Timings([Cycles; Self::ENTRIES]);

unsafe impl Zero for Timings {}
unsafe impl Fill8 for Timings {}

impl Timings {
    // The smallest possible block size is 32 KiB (used by the Wi-Fi region)
    pub const PAGE_SHIFT: usize = 15;
    pub const PAGE_SIZE: usize = 1 << Self::PAGE_SHIFT;
    pub const PAGE_MASK: u32 = (Self::PAGE_SIZE - 1) as u32;
    pub const ENTRIES: usize = 1 << (32 - Self::PAGE_SHIFT);

    pub(in super::super) fn new_boxed() -> Box<Self> {
        zeroed_box()
    }

    #[inline]
    pub fn get(&self, addr: u32) -> Cycles {
        self.0[(addr >> Self::PAGE_SHIFT) as usize]
    }

    fn set_range(&mut self, cycles: Cycles, (lower_bound, upper_bound): (u32, u32)) {
        debug_assert!(lower_bound & Self::PAGE_MASK == 0);
        debug_assert!(upper_bound & Self::PAGE_MASK == Self::PAGE_MASK);
        let lower_bound = (lower_bound >> Self::PAGE_SHIFT) as usize;
        let upper_bound = (upper_bound >> Self::PAGE_SHIFT) as usize;
        self.0[lower_bound..=upper_bound].fill(cycles);
    }

    pub fn setup(&mut self) {
        // TODO:
        // - The timings for permanently unmapped regions are unknown, they're assumed to be always
        //   1 cycle.
        // - The timings for temporarily unmapped regions are assumed to stay the same as when
        //   they're mapped, what actually happens?
        // - GBA slot, Wi-Fi

        fill_8(self, 1);

        // BIOS: same as unmapped

        // Main RAM
        self.set_range(
            Cycles {
                n32: 9,
                s32: 2,
                n16: 8,
                s16: 1,
            },
            (0x0200_0000, 0x02FF_FFFF),
        );

        // SWRAM, ARM7 WRAM, I/O: same as unmapped

        // VRAM allocated as ARM7 WRAM
        self.set_range(
            Cycles {
                n32: 2,
                s32: 2,
                n16: 1,
                s16: 1,
            },
            (0x0600_0000, 0x06FF_FFFF),
        );
    }
}
