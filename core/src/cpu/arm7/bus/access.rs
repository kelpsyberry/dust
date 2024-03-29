use super::{fallback, ptrs::Ptrs};
use crate::{
    cpu::{bus::AccessType, Engine},
    emu::Emu,
    utils::mem_prelude::*,
};

#[inline]
pub fn read_8<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32) -> u8 {
    if let Some(ptr) = emu.arm7.bus_ptrs.read(addr) {
        unsafe { ptr.add((addr & Ptrs::PAGE_MASK) as usize).read() }
    } else {
        fallback::read_8::<A, _>(emu, addr)
    }
}

#[inline]
pub fn read_16<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32) -> u16 {
    if let Some(ptr) = emu.arm7.bus_ptrs.read(addr) {
        unsafe { u16::read_le_aligned(ptr.add((addr & (Ptrs::PAGE_MASK & !1)) as usize).cast()) }
    } else {
        fallback::read_16::<A, _>(emu, addr)
    }
}

#[inline]
pub fn read_32<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32) -> u32 {
    if let Some(ptr) = emu.arm7.bus_ptrs.read(addr) {
        unsafe { u32::read_le_aligned(ptr.add((addr & (Ptrs::PAGE_MASK & !3)) as usize).cast()) }
    } else {
        fallback::read_32::<A, _>(emu, addr)
    }
}

#[inline]
pub fn write_8<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32, value: u8) {
    if let Some(ptr) = emu.arm7.bus_ptrs.write(addr) {
        unsafe { ptr.add((addr & Ptrs::PAGE_MASK) as usize).write(value) };
    } else {
        fallback::write_8::<A, _>(emu, addr, value);
    }
}

#[inline]
pub fn write_16<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32, value: u16) {
    if let Some(ptr) = emu.arm7.bus_ptrs.write(addr) {
        unsafe {
            value.write_le_aligned(ptr.add((addr & (Ptrs::PAGE_MASK & !1)) as usize).cast());
        };
    } else {
        fallback::write_16::<A, _>(emu, addr, value);
    }
}

#[inline]
pub fn write_32<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32, value: u32) {
    if let Some(ptr) = emu.arm7.bus_ptrs.write(addr) {
        unsafe {
            value.write_le_aligned(ptr.add((addr & (Ptrs::PAGE_MASK & !3)) as usize).cast());
        };
    } else {
        fallback::write_32::<A, _>(emu, addr, value);
    }
}
