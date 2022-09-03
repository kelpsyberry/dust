pub use super::super::engines_common::*;

use crate::cpu::psr::Psr;

static COND_TABLE: [u16; 0x10] = [
    0xF0F0, // EQ    (z)
    0x0F0F, // NE    (!z)
    0xCCCC, // CS/HS (c)
    0x3333, // CC/LO (!c)
    0xFF00, // MI    (n)
    0x00FF, // PL    (!n)
    0xAAAA, // VS    (v)
    0x5555, // VC    (!v)
    0x0C0C, // HI    (c && !z)
    0xF3F3, // LS    (!c || z)
    0xAA55, // GE    (n == v)
    0x55AA, // LT    (n != v)
    0x0A05, // GT    (!z && n == v)
    0xF5FA, // LE    (z || n != v)
    0xFFFF, // AL    (true)
    0x0000, // NV    (false)
];

impl Psr {
    #[inline]
    pub(super) fn satisfies_condition(self, condition: u8) -> bool {
        COND_TABLE[condition as usize] & 1 << (self.raw() >> 28) != 0
    }

    pub(super) fn copy_nzcv(&mut self, value: u32) {
        *self = Psr::from_raw((self.raw() & !0xF000_0000) | (value & 0xF000_0000));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StateSource {
    Arm,
    Thumb,
    R15Bit0,
    Cpsr,
}

cfg_if::cfg_if! {
    if #[cfg(feature = "interp-pipeline")] {
        #[cfg(feature = "interp-pipeline-accurate-reloads")]
        pub type PipelineEntry = u64;
        #[cfg(not(feature = "interp-pipeline-accurate-reloads"))]
        pub type PipelineEntry = u32;

        #[inline]
        pub const fn thumb_pipeline_entry(value: PipelineEntry) -> PipelineEntry {
            #[cfg(feature = "interp-pipeline-accurate-reloads")]
            {
                value | 1 << 32
            }
            #[cfg(not(feature = "interp-pipeline-accurate-reloads"))]
            value
        }
    }
}
