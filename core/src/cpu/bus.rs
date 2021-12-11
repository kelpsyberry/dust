use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "bft-r")] {
        pub type RDisableFlags = u8;
        pub mod r_disable_flags {
            use super::RDisableFlags;
            pub const WATCHPOINT: RDisableFlags = 1 << 0;
            pub const ALL: RDisableFlags = WATCHPOINT;
        }
    }
}

cfg_if! {
    if #[cfg(feature = "bft-w")] {
        pub type WDisableFlags = u8;
        pub mod w_disable_flags {
            use super::WDisableFlags;
            pub const WATCHPOINT: WDisableFlags = 1 << 0;
            pub const JIT: WDisableFlags = 1 << 1;
            pub const ALL: WDisableFlags = WATCHPOINT | JIT;
        }
    }
}

pub trait AccessType {
    const IS_DMA: bool;
    const IS_DEBUG: bool;
}

pub struct CpuAccess;

impl AccessType for CpuAccess {
    const IS_DMA: bool = false;
    const IS_DEBUG: bool = false;
}

pub struct DmaAccess;

impl AccessType for DmaAccess {
    const IS_DMA: bool = true;
    const IS_DEBUG: bool = false;
}

pub struct DebugCpuAccess;

impl AccessType for DebugCpuAccess {
    const IS_DMA: bool = false;
    const IS_DEBUG: bool = true;
}

pub struct DebugDmaAccess;

impl AccessType for DebugDmaAccess {
    const IS_DMA: bool = true;
    const IS_DEBUG: bool = true;
}
