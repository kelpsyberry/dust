use crate::utils::Savestate;

mod bounded {
    use crate::utils::bounded_int_lit;
    bounded_int_lit!(pub struct Mode(u8), max 0xF);
}
pub use bounded::Mode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bank {
    System,
    Fiq,
    Irq,
    Supervisor,
    Abort,
    Undefined,
}

const REG_BANKS: [Bank; 0x10] = [
    Bank::System,
    Bank::Fiq,
    Bank::Irq,
    Bank::Supervisor,
    Bank::System,
    Bank::System,
    Bank::System,
    Bank::Abort,
    Bank::System,
    Bank::System,
    Bank::System,
    Bank::Undefined,
    Bank::System,
    Bank::System,
    Bank::System,
    Bank::System,
];

const SPSR_BANKS: [Bank; 0x10] = [
    Bank::System,
    Bank::Fiq,
    Bank::Irq,
    Bank::Supervisor,
    Bank::Abort,
    Bank::Abort,
    Bank::Abort,
    Bank::Abort,
    Bank::Undefined,
    Bank::Undefined,
    Bank::Undefined,
    Bank::Undefined,
    Bank::System,
    Bank::System,
    Bank::System,
    Bank::System,
];

impl Mode {
    pub const USER: Mode = Mode::new(0x0);
    pub const FIQ: Mode = Mode::new(0x1);
    pub const IRQ: Mode = Mode::new(0x2);
    pub const SUPERVISOR: Mode = Mode::new(0x3);
    pub const ABORT: Mode = Mode::new(0x7);
    pub const UNDEFINED: Mode = Mode::new(0xB);
    pub const SYSTEM: Mode = Mode::new(0xF);

    #[inline]
    pub const fn is_valid(self) -> bool {
        0x888F & 1 << self.get() != 0
    }

    #[inline]
    pub const fn is_privileged(self) -> bool {
        self.get() != 0
    }

    #[inline]
    pub const fn has_spsr(self) -> bool {
        0xF001 & 1 << self.get() == 0
    }

    #[inline]
    pub const fn reg_bank(self) -> Bank {
        REG_BANKS[self.get() as usize]
    }

    #[inline]
    pub const fn spsr_bank(self) -> Bank {
        SPSR_BANKS[self.get() as usize]
    }
}

#[inline]
const fn apply_psr_mask<const ARM9: bool>(value: u32) -> u32 {
    (value & if ARM9 { 0xF800_00EF } else { 0xF000_00EF }) | 0x10
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct Psr(u32): Debug {
        pub raw: u32 [read_only] @ ..,
        pub mode: u8 [Mode] @ 0..=3,
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

impl Psr {
    /// Bits 0-3 of the specified value must represent a valid mode, bit 4 must be 1 and bits 8-27
    /// (or 8-26 if this PSR is used on the ARM9 core) must be 0
    #[inline]
    pub const fn from_raw(value: u32) -> Self {
        Psr(value)
    }

    #[inline]
    pub const fn from_raw_masked<const ARM9: bool>(value: u32) -> Self {
        Psr(apply_psr_mask::<ARM9>(value))
    }
}
