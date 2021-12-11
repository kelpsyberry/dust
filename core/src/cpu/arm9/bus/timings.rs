use crate::utils::{zeroed_box, Zero};

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cycles {
    pub n32_data: u8,
    pub s32_data: u8,
    pub n16_data: u8,
    pub s16_data: u8,
    pub code: u8,
}

#[repr(transparent)]
pub struct Timings([Cycles; Self::ENTRIES]);

unsafe impl Zero for Timings {}

impl Timings {
    // The smallest possible block size is 16 MiB (the boundary of most memory regions)
    pub const PAGE_SHIFT: usize = 24;
    pub const PAGE_SIZE: usize = 1 << Self::PAGE_SHIFT;
    pub const PAGE_MASK: u32 = (Self::PAGE_SIZE - 1) as u32;
    pub const ENTRIES: usize = 1 << (32 - Self::PAGE_SHIFT);

    pub(in super::super) fn new_boxed() -> Box<Self> {
        zeroed_box()
    }

    pub(in super::super) fn timings(&self) -> &[Cycles; Self::ENTRIES] {
        &self.0
    }

    #[inline]
    pub fn get(&self, addr: u32) -> Cycles {
        self.0[(addr >> Self::PAGE_SHIFT) as usize]
    }

    pub fn set_range(&mut self, cycles: Cycles, (lower_bound, upper_bound): (u32, u32)) {
        debug_assert!(lower_bound & Self::PAGE_MASK == 0);
        debug_assert!(upper_bound & Self::PAGE_MASK == Self::PAGE_MASK);
        let lower_bound = (lower_bound >> Self::PAGE_SHIFT) as usize;
        let upper_bound = (upper_bound >> Self::PAGE_SHIFT) as usize;
        self.0[lower_bound..=upper_bound].fill(cycles);
    }

    pub fn setup(&mut self) {
        // TODO:
        // - The timings for permanently unmapped regions are unknown, they're assumed to be the
        //   same as the BIOS region's.
        // - GBA slot

        for cycles in &mut self.0 {
            *cycles = Cycles {
                n32_data: 8,
                s32_data: 2,
                n16_data: 8,
                s16_data: 2,
                code: 8,
            };
        }

        // Main RAM
        self.set_range(
            Cycles {
                n32_data: 20,
                s32_data: 4,
                n16_data: 18,
                s16_data: 2,
                code: 18,
            },
            (0x0200_0000, 0x02FF_FFFF),
        );

        // SWRAM, I/O: same as unmapped

        // Palette, VRAM
        self.set_range(
            Cycles {
                n32_data: 10,
                s32_data: 4,
                n16_data: 8,
                s16_data: 2,
                code: 10,
            },
            (0x0500_0000, 0x06FF_FFFF),
        );

        // OAM, BIOS: same as unmapped
    }
}
