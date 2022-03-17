use super::fallback;
use crate::{
    cpu::{
        arm9::{bus::ptrs::Ptrs as SysBusPtrs, cp15::ptrs::Ptrs, CoreData, Engine},
        bus::AccessType,
    },
    emu::Emu,
    utils::MemValue,
};

macro_rules! check_tcm_read {
    ($emu: expr, $addr: ident, $code: expr, $align_mask: expr) => {
        #[cfg(feature = "bft-r")]
        {
            if $addr & $emu.arm9.cp15.itcm_addr_check_mask()
                == $emu.arm9.cp15.itcm_addr_check_value()
            {
                return $emu
                    .arm9
                    .cp15
                    .itcm()
                    .read_le($addr as usize & (0x7FFF & !$align_mask));
            }
            if !$code
                && $addr & $emu.arm9.cp15.dtcm_addr_check_mask()
                    == $emu.arm9.cp15.dtcm_addr_check_value()
            {
                return $emu
                    .arm9
                    .cp15
                    .dtcm()
                    .read_le($addr as usize & (0x3FFF & !$align_mask));
            }
        }
    };
}

macro_rules! check_tcm_write {
    ($emu: expr, $addr: ident, $value: expr, $align_mask: expr) => {
        #[cfg(feature = "bft-w")]
        {
            if $addr & $emu.arm9.cp15.itcm_addr_check_mask()
                == $emu.arm9.cp15.itcm_addr_check_value()
            {
                $emu.arm9
                    .cp15
                    .itcm()
                    .write_le($addr as usize & (0x7FFF & !$align_mask), $value);
                return;
            }
            if $addr & $emu.arm9.cp15.dtcm_addr_check_mask()
                == $emu.arm9.cp15.dtcm_addr_check_value()
            {
                $emu.arm9
                    .cp15
                    .dtcm()
                    .write_le($addr as usize & (0x3FFF & !$align_mask), $value);
                return;
            }
        }
    };
}

#[inline]
pub fn read_8<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32) -> u8 {
    if let Some(ptr) = if A::IS_DMA {
        emu.arm9.bus_ptrs.read(addr)
    } else {
        emu.arm9.cp15.ptrs.read_data(addr)
    } {
        unsafe {
            ptr.add(
                (addr
                    & if A::IS_DMA {
                        SysBusPtrs::PAGE_MASK
                    } else {
                        Ptrs::PAGE_MASK
                    }) as usize,
            )
            .read()
        }
    } else {
        #[cfg(feature = "debugger-hooks")]
        check_watchpoints!(emu, emu.arm9, addr, 0, 1, Read);
        if !A::IS_DMA {
            check_tcm_read!(emu, addr, false, 0);
        }
        fallback::read_8::<A, _>(emu, addr)
    }
}

#[inline]
pub fn read_16<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32) -> u16 {
    if let Some(ptr) = if A::IS_DMA {
        emu.arm9.bus_ptrs.read(addr)
    } else {
        emu.arm9.cp15.ptrs.read_data(addr)
    } {
        unsafe {
            u16::read_le_aligned(ptr.add(
                (addr
                    & (if A::IS_DMA {
                        SysBusPtrs::PAGE_MASK
                    } else {
                        Ptrs::PAGE_MASK
                    } & !1)) as usize,
            ) as *const _)
        }
    } else {
        #[cfg(feature = "debugger-hooks")]
        check_watchpoints!(emu, emu.arm9, addr, 1, 5, Read);
        if !A::IS_DMA {
            check_tcm_read!(emu, addr, false, 1);
        }
        fallback::read_16::<A, _>(emu, addr)
    }
}

#[inline]
pub fn read_32<A: AccessType, E: Engine, const CODE: bool>(emu: &mut Emu<E>, addr: u32) -> u32 {
    if let Some(ptr) = if A::IS_DMA {
        emu.arm9.bus_ptrs.read(addr)
    } else if CODE {
        emu.arm9.cp15.ptrs.read_code(addr)
    } else {
        emu.arm9.cp15.ptrs.read_data(addr)
    } {
        unsafe {
            u32::read_le_aligned(ptr.add(
                (addr
                    & (if A::IS_DMA {
                        SysBusPtrs::PAGE_MASK
                    } else {
                        Ptrs::PAGE_MASK
                    } & !3)) as usize,
            ) as *const _)
        }
    } else {
        #[cfg(feature = "debugger-hooks")]
        check_watchpoints!(emu, emu.arm9, addr, 3, 0x55, Read);
        if !A::IS_DMA {
            check_tcm_read!(emu, addr, CODE, 3);
        }
        fallback::read_32::<A, _>(emu, addr)
    }
}

#[inline]
pub fn write_8<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32, value: u8) {
    if let Some(ptr) = if A::IS_DMA {
        emu.arm9.bus_ptrs.write_8(addr)
    } else {
        emu.arm9.cp15.ptrs.write_8(addr)
    } {
        unsafe {
            ptr.add(
                (addr
                    & if A::IS_DMA {
                        SysBusPtrs::PAGE_MASK
                    } else {
                        Ptrs::PAGE_MASK
                    }) as usize,
            )
            .write(value);
        }
    } else {
        emu.arm9.engine_data.invalidate_word(addr);
        #[cfg(feature = "debugger-hooks")]
        check_watchpoints!(emu, emu.arm9, addr, 0, 2, Write);
        if !A::IS_DMA {
            check_tcm_write!(emu, addr, value, 0);
        }
        fallback::write_8::<A, _>(emu, addr, value);
    }
}

#[inline]
pub fn write_16<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32, value: u16) {
    if let Some(ptr) = if A::IS_DMA {
        emu.arm9.bus_ptrs.write_16_32(addr)
    } else {
        emu.arm9.cp15.ptrs.write_16_32(addr)
    } {
        unsafe {
            value.write_le_aligned(ptr.add(
                (addr
                    & (if A::IS_DMA {
                        SysBusPtrs::PAGE_MASK
                    } else {
                        Ptrs::PAGE_MASK
                    } & !1)) as usize,
            ) as *mut _);
        }
    } else {
        emu.arm9.engine_data.invalidate_word(addr);
        #[cfg(feature = "debugger-hooks")]
        check_watchpoints!(emu, emu.arm9, addr, 1, 0xA, Write);
        if !A::IS_DMA {
            check_tcm_write!(emu, addr, value, 1);
        }
        fallback::write_16::<A, _>(emu, addr, value);
    }
}

#[inline]
pub fn write_32<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32, value: u32) {
    if let Some(ptr) = if A::IS_DMA {
        emu.arm9.bus_ptrs.write_16_32(addr)
    } else {
        emu.arm9.cp15.ptrs.write_16_32(addr)
    } {
        unsafe {
            value.write_le_aligned(ptr.add(
                (addr
                    & (if A::IS_DMA {
                        SysBusPtrs::PAGE_MASK
                    } else {
                        Ptrs::PAGE_MASK
                    } & !3)) as usize,
            ) as *mut _);
        }
    } else {
        emu.arm9.engine_data.invalidate_word(addr);
        #[cfg(feature = "debugger-hooks")]
        check_watchpoints!(emu, emu.arm9, addr, 3, 0xAA, Write);
        if !A::IS_DMA {
            check_tcm_write!(emu, addr, value, 3);
        }
        fallback::write_32::<A, _>(emu, addr, value);
    }
}
