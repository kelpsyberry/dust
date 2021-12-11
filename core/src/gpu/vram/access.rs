use super::Vram;
use crate::utils::{make_zero, zero, ByteMutSlice, MemValue};
use core::intrinsics::likely;
use core::{
    mem,
    ops::{BitOr, BitOrAssign},
    ptr,
};

macro_rules! read_banks {
    ($self: expr, $T: ty, $mapped: expr, $addr: expr, $($bit: expr => $bank: ident),*$(,)?) => {{
        let mut result = zero();
        $(
            if $mapped & 1 << $bit != 0 {
                result |= unsafe { $self.banks.$bank.read_le_aligned_unchecked(
                    $addr as usize & (($self.banks.$bank.len() - 1) & !(mem::align_of::<$T>() - 1))
                ) };
            }
        )*
        result
    }}
}

macro_rules! write_banks {
    (
        $self: expr, $T: ty, $mapped: expr, $addr: expr, $value: expr,
        $($bit: expr => $bank: ident),*$(,)?
    ) => {{
        $(
            if $mapped & 1 << $bit != 0 {
                unsafe { $self.banks.$bank.write_le_aligned_unchecked(
                    $addr as usize & (($self.banks.$bank.len() - 1) & !(mem::align_of::<$T>() - 1)),
                    $value,
                ) };
            }
        )*
    }}
}

macro_rules! read_bank_slice {
    (
        $self: expr, $T: ty, $mapped: expr, $addr: expr, $result: expr,
        $($bit: expr => $bank: ident),*$(,)?
    ) => {{
        make_zero(&mut $result[..]);
        $(
            if $mapped & 1 << $bit != 0 {
                let start = $addr as usize & ($self.banks.$bank.len() - 1);
                for i in (0..$result.len()).step_by(mem::size_of::<$T>()) {
                    $result.write_ne_aligned_unchecked(
                        i,
                        $result.read_ne_aligned_unchecked::<$T>(i)
                            | $self.banks.$bank.read_ne_aligned_unchecked::<$T>(start + i),
                    );
                }
            }
        )*
    }}
}

impl Vram {
    pub fn read_lcdc<T: MemValue>(&self, addr: u32) -> T {
        let region = addr as usize >> 14 & 0x3F;
        // LCDC ptrs can never be null, they'll always either point to a valid bank or to
        // zero/ignored ones.
        unsafe {
            T::read_le_aligned(
                self.map.lcdc_r_ptrs[region]
                    .add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1)))
                    as *const T,
            )
        }
    }

    pub fn write_lcdc<T: MemValue>(&mut self, addr: u32, value: T) {
        let region = addr as usize >> 14 & 0x3F;
        // See read_lcdc
        unsafe {
            value.write_le_aligned(
                self.map.lcdc_w_ptrs[region]
                    .add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1)))
                    as *mut T,
            );
        }
    }

    pub fn read_a_bg<T: MemValue + BitOrAssign>(&self, addr: u32) -> T {
        let region = addr as usize >> 14 & 0x1F;
        let ptr = self.map.a_bg_r_ptrs[region];
        if likely(!ptr.is_null()) {
            return unsafe {
                T::read_le_aligned(
                    ptr.add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1))) as *const T,
                )
            };
        }
        read_banks!(
            self, T, self.map.a_bg[region], addr,
            0 => a,
            1 => b,
            2 => c,
            3 => d,
            4 => e,
            5 => f,
            6 => g,
        )
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `result` must not overlap with any VRAM bank
    /// - `addr + result.len()` must be less than or equal to `0x8_0000`
    /// - `addr..addr + result.len()` must not cross a `0x4000`-byte boundary
    pub unsafe fn read_a_bg_slice<T: MemValue + BitOr<Output = T>>(
        &self,
        addr: u32,
        mut result: ByteMutSlice,
    ) {
        let region = addr as usize >> 14;
        let ptr = *self.map.a_bg_r_ptrs.get_unchecked(region);
        if likely(!ptr.is_null()) {
            return ptr::copy_nonoverlapping(
                ptr.add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1))) as *const T,
                result.as_mut_ptr() as *mut T,
                result.len() / mem::size_of::<T>(),
            );
        }
        read_bank_slice!(
            self, T, *self.map.a_bg.get_unchecked(region), addr, result,
            0 => a,
            1 => b,
            2 => c,
            3 => d,
            4 => e,
            5 => f,
            6 => g,
        );
    }

    pub fn write_a_bg<T: MemValue>(&mut self, addr: u32, value: T) {
        let region = addr as usize >> 14 & 0x1F;
        let ptr = self.map.a_bg_w_ptrs[region];
        if likely(!ptr.is_null()) {
            return unsafe {
                value.write_le_aligned(
                    ptr.add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1))) as *mut T,
                );
            };
        }
        write_banks!(
            self, T, self.map.a_bg[region], addr, value,
            0 => a,
            1 => b,
            2 => c,
            3 => d,
            4 => e,
            5 => f,
            6 => g,
        );
    }

    pub fn read_a_obj<T: MemValue + BitOrAssign>(&self, addr: u32) -> T {
        let region = addr as usize >> 14 & 0xF;
        let ptr = self.map.a_obj_r_ptrs[region];
        if likely(!ptr.is_null()) {
            return unsafe {
                T::read_le_aligned(
                    ptr.add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1))) as *const T,
                )
            };
        }
        read_banks!(
            self, T, self.map.a_obj[region], addr,
            0 => a,
            1 => b,
            2 => e,
            3 => f,
            4 => g,
        )
    }

    pub fn write_a_obj<T: MemValue>(&mut self, addr: u32, value: T) {
        let region = addr as usize >> 14 & 0xF;
        let ptr = self.map.a_obj_w_ptrs[region];
        if likely(!ptr.is_null()) {
            return unsafe {
                value.write_le_aligned(
                    ptr.add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1))) as *mut T,
                );
            };
        }
        write_banks!(
            self, T, self.map.a_obj[region], addr, value,
            0 => a,
            1 => b,
            2 => e,
            3 => f,
            4 => g,
        );
    }

    pub fn read_b_bg<T: MemValue + BitOrAssign>(&self, addr: u32) -> T {
        let region = addr as usize >> 14 & 7;
        let ptr = self.map.b_bg_r_ptrs[region];
        if likely(!ptr.is_null()) {
            return unsafe {
                T::read_le_aligned(
                    ptr.add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1))) as *const T,
                )
            };
        }
        read_banks!(
            self, T, self.map.b_bg[region >> 1], addr,
            0 => c,
            1 => h,
            2 => i,
        )
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `result` must not overlap with any VRAM bank
    /// - `addr + result.len()` must be less than or equal to `0x2_0000`
    /// - `addr..addr + result.len()` must not cross a `0x4000`-byte boundary
    pub unsafe fn read_b_bg_slice<T: MemValue + BitOr<Output = T>>(
        &self,
        addr: u32,
        mut result: ByteMutSlice,
    ) {
        let region = addr as usize >> 14;
        let ptr = *self.map.b_bg_r_ptrs.get_unchecked(region);
        if likely(!ptr.is_null()) {
            return ptr::copy_nonoverlapping(
                ptr.add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1))) as *const T,
                result.as_mut_ptr() as *mut T,
                result.len() / mem::size_of::<T>(),
            );
        }
        read_bank_slice!(
            self, T, *self.map.b_bg.get_unchecked(region >> 1), addr, result,
            0 => c,
            1 => h,
            2 => i,
        );
    }

    pub fn write_b_bg<T: MemValue>(&mut self, addr: u32, value: T) {
        let region = addr as usize >> 14 & 7;
        let ptr = self.map.b_bg_w_ptrs[region];
        if likely(!ptr.is_null()) {
            return unsafe {
                value.write_le_aligned(
                    ptr.add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1))) as *mut T,
                );
            };
        }
        write_banks!(
            self, T, self.map.b_bg[region >> 1], addr, value,
            0 => c,
            1 => h,
            2 => i,
        );
    }

    pub fn read_b_obj<T: MemValue + BitOrAssign>(&self, addr: u32) -> T {
        let region = addr as usize >> 14 & 7;
        let ptr = self.map.b_obj_r_ptrs[region];
        if likely(!ptr.is_null()) {
            return unsafe {
                T::read_le_aligned(
                    ptr.add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1))) as *const T,
                )
            };
        }
        read_banks!(
            self, T, self.map.b_obj, addr,
            0 => d,
            1 => i,
        )
    }

    pub fn write_b_obj<T: MemValue>(&mut self, addr: u32, value: T) {
        let region = addr as usize >> 14 & 7;
        let ptr = self.map.b_obj_w_ptrs[region];
        if likely(!ptr.is_null()) {
            return unsafe {
                value.write_le_aligned(
                    ptr.add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1))) as *mut T,
                );
            };
        }
        write_banks!(
            self, T, self.map.b_obj, addr, value,
            0 => d,
            1 => i,
        );
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `result` must not overlap with any VRAM bank
    /// - `addr + result.len()` must be less than or equal to `0x8000`
    /// - `addr..addr + result.len()` must not cross a `0x4000`-byte boundary
    pub unsafe fn read_a_bg_ext_pal_slice<T: MemValue + BitOr<Output = T>>(
        &self,
        addr: u32,
        mut result: ByteMutSlice,
    ) {
        let region = addr as usize >> 14 & 1;
        let ptr = self.map.a_bg_ext_pal_ptrs[region];
        if likely(!ptr.is_null()) {
            return ptr::copy_nonoverlapping(
                ptr.add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1))) as *const T,
                result.as_mut_ptr() as *mut T,
                result.len() / mem::size_of::<T>(),
            );
        }
        read_bank_slice!(
            self, T, self.map.a_bg_ext_pal[region], addr, result,
            0 => e,
            1 => f,
            2 => g,
        );
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `result` must not overlap with any VRAM bank
    /// - `addr + result.len()` must be less than or equal to `0x2000`
    pub unsafe fn read_a_obj_ext_pal_slice<T: MemValue + BitOr<Output = T>>(
        &self,
        addr: u32,
        mut result: ByteMutSlice,
    ) {
        if !self.map.a_obj_ext_pal_ptr.is_null() {
            return ptr::copy_nonoverlapping(
                self.map.a_obj_ext_pal_ptr.add(addr as usize) as *const T,
                result.as_mut_ptr() as *mut T,
                result.len() / mem::size_of::<T>(),
            );
        }
        read_bank_slice!(
            self, T, self.map.a_obj_ext_pal, addr, result,
            0 => f,
            1 => g,
        );
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `result` must not overlap with any VRAM bank
    /// - `addr + result.len()` must be less than or equal to `0x8000`
    pub unsafe fn read_b_bg_ext_pal_slice<T: MemValue>(&self, addr: u32, mut result: ByteMutSlice) {
        // Will never be null
        ptr::copy_nonoverlapping(
            self.map.b_bg_ext_pal_ptr.add(addr as usize) as *const T,
            result.as_mut_ptr() as *mut T,
            result.len() / mem::size_of::<T>(),
        );
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `result` must not overlap with any VRAM bank
    /// - `addr + result.len()` must be less than or equal to `0x2000`
    pub unsafe fn read_b_obj_ext_pal_slice<T: MemValue>(
        &self,
        addr: u32,
        mut result: ByteMutSlice,
    ) {
        // Will never be null
        ptr::copy_nonoverlapping(
            self.map.b_obj_ext_pal_ptr.add(addr as usize) as *const T,
            result.as_mut_ptr() as *mut T,
            result.len() / mem::size_of::<T>(),
        );
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `result` must not overlap with any VRAM bank
    /// - `addr + result.len()` must be less than or equal to `0x8_0000`
    /// - `addr..addr + result.len()` must not cross a `0x2_0000`-byte boundary
    pub unsafe fn read_texture_slice<T: MemValue + BitOr<Output = T>>(
        &self,
        addr: u32,
        mut result: ByteMutSlice,
    ) {
        let region = addr as usize >> 17 & 3;
        let ptr = self.map.texture_ptrs[region];
        if likely(!ptr.is_null()) {
            return ptr::copy_nonoverlapping(
                ptr.add(addr as usize & (0x1_FFFF & !(mem::align_of::<T>() - 1))) as *const T,
                result.as_mut_ptr() as *mut T,
                result.len() / mem::size_of::<T>(),
            );
        }
        read_bank_slice!(
            self, T, self.map.texture[region], addr, result,
            0 => a,
            1 => b,
            2 => c,
            3 => d,
        );
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `result` must not overlap with any VRAM bank
    /// - `addr + result.len()` must be less than or equal to `0x1_8000`
    /// - `addr..addr + result.len()` must not cross a `0x4000`-byte boundary
    pub unsafe fn read_tex_pal_slice<T: MemValue + BitOr<Output = T>>(
        &self,
        addr: u32,
        mut result: ByteMutSlice,
    ) {
        let region = addr as usize >> 14;
        let ptr = *self.map.tex_pal_ptrs.get_unchecked(region);
        if likely(!ptr.is_null()) {
            return ptr::copy_nonoverlapping(
                ptr.add(addr as usize & (0x3FFF & !(mem::align_of::<T>() - 1))) as *const T,
                result.as_mut_ptr() as *mut T,
                result.len() / mem::size_of::<T>(),
            );
        }
        read_bank_slice!(
            self, T, *self.map.tex_pal.get_unchecked(region), addr, result,
            0 => e,
            1 => f,
            2 => g,
        );
    }

    pub fn read_arm7<T: MemValue + BitOrAssign>(&self, addr: u32) -> T {
        let region = addr as usize >> 17 & 1;
        let ptr = self.map.arm7_r_ptrs[region];
        if likely(!ptr.is_null()) {
            return unsafe {
                T::read_le_aligned(
                    ptr.add(addr as usize & (0x1_FFFF & !(mem::align_of::<T>() - 1))) as *const T,
                )
            };
        }
        read_banks!(
            self, T, self.map.arm7[region], addr,
            0 => c,
            1 => d,
        )
    }

    pub fn write_arm7<T: MemValue>(&mut self, addr: u32, value: T) {
        let region = addr as usize >> 17 & 1;
        let ptr = self.map.arm7_w_ptrs[region];
        if likely(!ptr.is_null()) {
            return unsafe {
                value.write_le_aligned(
                    ptr.add(addr as usize & (0x1_FFFF & !(mem::align_of::<T>() - 1))) as *mut T,
                );
            };
        }
        write_banks!(
            self, T, self.map.arm7[region], addr, value,
            0 => c,
            1 => d,
        );
    }
}
