use crate::utils::Savestate;
use core::{convert::TryFrom, mem::transmute};
use proc_bitfield::UnsafeFrom;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Savestate)]
pub enum Mode {
    User = 0x0,
    Fiq = 0x1,
    Irq = 0x2,
    Supervisor = 0x3,
    Abort = 0x7,
    Undefined = 0xB,
    System = 0xF,
}

impl Mode {
    #[inline]
    pub const fn is_privileged(self) -> bool {
        !matches!(self, Mode::User)
    }

    #[inline]
    pub const fn is_exception(self) -> bool {
        !matches!(self, Mode::User | Mode::System)
    }

    #[inline]
    const fn raw_psr_is_valid(value: u32) -> bool {
        0x888F & 1 << (value & 0xF) != 0
    }

    #[inline]
    pub const fn try_from_raw(value: u8) -> Option<Self> {
        if value > 0xF || 0x888F & 1 << value == 0 {
            return None;
        }
        Some(unsafe { transmute(value) })
    }
}

impl TryFrom<u8> for Mode {
    type Error = ();

    #[inline]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::try_from_raw(value).ok_or(())
    }
}

impl const UnsafeFrom<u8> for Mode {
    #[inline]
    unsafe fn unsafe_from(raw: u8) -> Self {
        transmute(raw)
    }
}

impl const From<Mode> for u8 {
    #[inline]
    fn from(mode: Mode) -> Self {
        mode as u8
    }
}

#[inline]
const fn apply_psr_mask<const ARM9: bool>(value: u32) -> u32 {
    (value & if ARM9 { 0xF800_00EF } else { 0xF000_00EF }) | 0x10
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct Cpsr(u32): Debug {
        pub raw: u32 [read_only] @ ..,
        pub mode: u8 [unsafe Mode] @ 0..=3,
        pub thumb_state: bool @ 5,
        pub fiqs_disabled: bool @ 6,
        pub irqs_disabled: bool @ 7,
        pub sticky_overflow: bool @ 27,
        pub overflow: bool @ 28,
        pub carry: bool @ 29,
        pub zero: bool @ 30,
        pub negative: bool @ 31,
    }
}

impl Cpsr {
    #[track_caller]
    #[inline]
    pub const fn from_raw<const ARM9: bool>(value: u32) -> Self {
        assert!(Mode::raw_psr_is_valid(value), "Invalid mode specified");
        Cpsr(apply_psr_mask::<ARM9>(value))
    }

    #[inline]
    pub const fn try_from_raw<const ARM9: bool>(value: u32) -> Option<Self> {
        if !Mode::raw_psr_is_valid(value) {
            return None;
        };
        Some(Cpsr(apply_psr_mask::<ARM9>(value)))
    }

    /// # Safety
    /// Bits 0-3 of the specified value must represent a valid mode, bit 4 must be 1 and bits 8-27
    /// (or 8-26 if this PSR is used on the ARM9 core) must be 0
    #[inline]
    pub const unsafe fn from_raw_unchecked(value: u32) -> Self {
        Cpsr(value)
    }

    #[track_caller]
    #[inline]
    pub const fn from_spsr(value: Spsr) -> Self {
        assert!(Mode::raw_psr_is_valid(value.0), "Invalid mode specified");
        Cpsr(value.0)
    }

    #[inline]
    pub const fn try_from_spsr(value: Spsr) -> Option<Self> {
        if !Mode::raw_psr_is_valid(value.0) {
            return None;
        };
        Some(Cpsr(value.0))
    }
}

impl UnsafeFrom<u32> for Cpsr {
    #[inline]
    unsafe fn unsafe_from(raw: u32) -> Self {
        Cpsr(raw)
    }
}

impl TryFrom<Spsr> for Cpsr {
    type Error = ();

    #[inline]
    fn try_from(value: Spsr) -> Result<Self, Self::Error> {
        Self::try_from_spsr(value).ok_or(())
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct Spsr(u32): Debug {
        pub raw: u32 [read_only] @ ..,
        pub mode_raw: u8 @ 0..=3,
        pub mode: u8 [try_get Mode, set Mode] @ 0..=3,
        pub thumb_state: bool @ 5,
        pub fiqs_disabled: bool @ 6,
        pub irqs_disabled: bool @ 7,
        pub sticky_overflow: bool @ 27,
        pub overflow: bool @ 28,
        pub carry: bool @ 29,
        pub zero: bool @ 30,
        pub negative: bool @ 31,
    }
}

impl From<Cpsr> for Spsr {
    #[inline]
    fn from(other: Cpsr) -> Self {
        Spsr(other.0)
    }
}

impl Spsr {
    #[inline]
    pub const fn from_raw<const ARM9: bool>(value: u32) -> Self {
        Spsr(apply_psr_mask::<ARM9>(value))
    }
}
