use super::Vram;
use crate::utils::{zero, MemValue};
use core::{
    mem,
    ops::{BitOr, BitOrAssign},
    ptr,
};

macro_rules! set_writeback {
    ($arr: expr, $addr: expr, $T: ty) => {
        *$arr.get_unchecked_mut($addr / usize::BITS as usize) |=
            ((1 << mem::size_of::<$T>()) - 1) << ($addr & (usize::BITS as usize - 1));
    };
}

macro_rules! clear_writeback {
    ($arr: expr, $addr: expr, $T: ty) => {
        *$arr.get_unchecked_mut($addr / usize::BITS as usize) &=
            !(((1 << mem::size_of::<$T>()) - 1) << ($addr & (usize::BITS as usize - 1)));
    };
}

macro_rules! handle_mirroring {
    (
        $self: ident,
        $usage: ident, $value: ident, $T: ty, $mirror_addr: ident, $mirror_mapped: expr;
        $($bit: literal => $bank: ident,)*
    ) => {
        let writeback_mask = (1 << mem::size_of::<T>()) - 1;
        let writeback = unsafe {
            *$self.writeback.$usage.get_mut().get_unchecked($mirror_addr / usize::BITS as usize)
        } >> ($mirror_addr & (usize::BITS as usize - 1)) & writeback_mask;
        if writeback == writeback_mask {
            unsafe {
                let prev_value = $self.$usage.read_le_aligned_unchecked::<T>($mirror_addr);
                $(
                    if $mirror_mapped & 1 << $bit != 0 {
                        $self.banks.$bank.write_le_aligned_unchecked(
                            $mirror_addr & ($self.banks.$bank.len() - 1),
                            prev_value,
                        );
                    }
                )*
                $self.$usage.write_le_aligned_unchecked(
                    $mirror_addr,
                    prev_value | $value,
                );
            }
        } else {
            let mut others_value = zero::<$T>();
            $(
                if $mirror_mapped & 1 << $bit != 0 {
                    others_value |= unsafe {
                        $self.banks.$bank.read_le_aligned_unchecked(
                            $mirror_addr & ($self.banks.$bank.len() - 1),
                        )
                    };
                }
            )*
            if writeback == 0 {
                unsafe {
                    $self.$usage.write_le_aligned_unchecked(
                        $mirror_addr,
                        others_value | $value,
                    );
                }
            } else {
                let value_bytes = $value.to_le_bytes();
                let others_value_bytes = others_value.to_le_bytes();
                for (i, (value_byte, others_value_byte)) in value_bytes
                    .into_iter()
                    .zip(others_value_bytes)
                    .enumerate()
                {
                    let addr = $mirror_addr | i;
                    if writeback & 1 << i == 0 {
                        $self.$usage.write(
                            addr,
                            others_value_byte | value_byte,
                        );
                    } else {
                        let prev_value_byte = $self.$usage.read(addr);
                        $(
                            if $mirror_mapped & 1 << $bit != 0 {
                                unsafe {
                                    $self.banks.$bank.write_unchecked(
                                        addr & ($self.banks.$bank.len() - 1),
                                        prev_value_byte,
                                    );
                                }
                            }
                        )*
                        $self.$usage.write(addr, prev_value_byte | value_byte);
                    }
                }
            }
        }
        unsafe {
            clear_writeback!($self.writeback.$usage.get_mut(), $mirror_addr, $T);
        }
    };
}

// TODO: For performance reasons, all code here assumes that the size of reads/writes is lower than
// or equal to usize::BITS, which is safe for this emulator's code, but may not be for external code
// that implements `MemValue` for arbitrary types. How to solve this?

impl Vram {
    #[inline]
    pub fn read_lcdc<T: MemValue>(&self, addr: u32) -> T {
        let region = addr as usize >> 14 & 0x3F;
        // NOTE: LCDC ptrs can never be null, they'll always either point to a valid bank or to
        // zero/ignored ones.
        unsafe {
            T::read_le_aligned(
                self.lcdc_r_ptrs[region].add(addr as usize & (0x3FFF & !(mem::size_of::<T>() - 1)))
                    as *const T,
            )
        }
    }

    #[inline]
    pub fn write_lcdc<T: MemValue>(&mut self, addr: u32, value: T) {
        let region = addr as usize >> 14 & 0x3F;
        // See read_lcdc
        unsafe {
            value.write_le_aligned(
                self.lcdc_w_ptrs[region].add(addr as usize & (0x3FFF & !(mem::size_of::<T>() - 1)))
                    as *mut T,
            );
        }
    }

    #[inline]
    pub fn read_a_bg<T: MemValue>(&self, addr: u32) -> T {
        unsafe {
            self.a_bg
                .read_le_aligned_unchecked(addr as usize & (0x7_FFFF & !(mem::size_of::<T>() - 1)))
        }
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `addr + result.len()` must be less than or equal to `0x8_0000`
    #[inline]
    pub unsafe fn read_a_bg_slice<T: MemValue>(&self, addr: u32, len: usize, result: *mut T) {
        ptr::copy_nonoverlapping(
            self.a_bg.as_ptr().add(addr as usize) as *const T,
            result,
            len / mem::size_of::<T>(),
        );
    }

    #[inline(never)]
    fn handle_a_bg_mirroring<T: MemValue + BitOr<Output = T> + BitOrAssign>(
        &mut self,
        mirror_addr: usize,
        mirror_mapped: u8,
        value: T,
    ) where
        [(); mem::size_of::<T>()]: Sized,
    {
        handle_mirroring!(
            self, a_bg, value, T, mirror_addr, mirror_mapped;
            0 => a,
            1 => b,
            2 => c,
            3 => d,
            4 => e,
        );
    }

    #[inline]
    pub fn write_a_bg<T: MemValue + BitOr<Output = T> + BitOrAssign>(&mut self, addr: u32, value: T)
    where
        [(); mem::size_of::<T>()]: Sized,
    {
        let region = addr as usize >> 14 & 0x1F;
        let mapped = self.map.a_bg[region].get();
        if mapped == 0 {
            return;
        }
        let addr = addr as usize & (0x7_FFFF & !(mem::size_of::<T>() - 1));
        unsafe {
            set_writeback!(self.writeback.a_bg.get_mut(), addr, T);
            self.a_bg.write_le_aligned_unchecked(addr, value);
        }
        if mapped & 0x60 != 0 {
            let mirror_addr = addr ^ 0x8000;
            if mapped & !0x60 == 0 {
                unsafe {
                    self.a_bg.write_le_aligned_unchecked(mirror_addr, value);
                    clear_writeback!(self.writeback.a_bg.get_mut(), mirror_addr, T);
                }
            } else {
                self.handle_a_bg_mirroring(mirror_addr, mapped, value);
            }
        }
    }

    #[inline]
    pub fn read_a_obj<T: MemValue>(&self, addr: u32) -> T {
        unsafe {
            self.a_obj
                .read_le_aligned_unchecked(addr as usize & (0x3_FFFF & !(mem::size_of::<T>() - 1)))
        }
    }

    #[inline(never)]
    fn handle_a_obj_mirroring<T: MemValue + BitOr<Output = T> + BitOrAssign>(
        &mut self,
        mirror_addr: usize,
        mirror_mapped: u8,
        value: T,
    ) where
        [(); mem::size_of::<T>()]: Sized,
    {
        handle_mirroring!(
            self, a_obj, value, T, mirror_addr, mirror_mapped;
            0 => a,
            1 => b,
            2 => e,
        );
    }

    #[inline]
    pub fn write_a_obj<T: MemValue + BitOr<Output = T> + BitOrAssign>(
        &mut self,
        addr: u32,
        value: T,
    ) where
        [(); mem::size_of::<T>()]: Sized,
    {
        let region = addr as usize >> 14 & 0xF;
        let mapped = self.map.a_obj[region].get();
        if mapped == 0 {
            return;
        }
        let addr = addr as usize & (0x3_FFFF & !(mem::size_of::<T>() - 1));
        unsafe {
            set_writeback!(self.writeback.a_obj.get_mut(), addr, T);
            self.a_obj.write_le_aligned_unchecked(addr, value);
        }
        if mapped & 0x18 != 0 {
            let mirror_addr = addr ^ 0x8000;
            if mapped & !0x18 == 0 {
                unsafe {
                    self.a_obj.write_le_aligned_unchecked(mirror_addr, value);
                    clear_writeback!(self.writeback.a_obj.get_mut(), mirror_addr, T);
                }
            } else {
                self.handle_a_obj_mirroring(mirror_addr, mapped, value);
            }
        }
    }

    #[inline]
    pub fn read_b_bg<T: MemValue>(&self, addr: u32) -> T {
        unsafe {
            self.b_bg
                .read_le_aligned_unchecked(addr as usize & (0x1_FFFF & !(mem::size_of::<T>() - 1)))
        }
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `addr + result.len()` must be less than or equal to `0x2_0000`
    #[inline]
    pub unsafe fn read_b_bg_slice<T: MemValue>(&self, addr: u32, len: usize, result: *mut T) {
        ptr::copy_nonoverlapping(
            self.b_bg.as_ptr().add(addr as usize) as *const T,
            result,
            len / mem::size_of::<T>(),
        );
    }

    #[inline(never)]
    fn handle_b_bg_mirroring<T: MemValue + BitOr<Output = T> + BitOrAssign>(
        &mut self,
        addr: usize,
        mapped: u8,
        value: T,
    ) where
        [(); mem::size_of::<T>()]: Sized,
    {
        let mirror_addr = addr ^ 0x1_0000;
        if mapped & 1 == 0 {
            unsafe {
                self.b_bg.write_le_aligned_unchecked(mirror_addr, value);
                clear_writeback!(self.writeback.b_bg.get_mut(), mirror_addr, T);
            }
        } else {
            handle_mirroring!(
                self, b_bg, value, T, mirror_addr, 1;
                0 => c,
            );
        }
        if mapped & 4 != 0 {
            for mirror_addr in [addr ^ 0x4000, addr ^ 0x1_4000] {
                handle_mirroring!(
                    self, b_bg, value, T, mirror_addr, 1;
                    0 => c,
                    1 => h,
                );
            }
        }
    }

    #[inline]
    pub fn write_b_bg<T: MemValue + BitOr<Output = T> + BitOrAssign>(&mut self, addr: u32, value: T)
    where
        [(); mem::size_of::<T>()]: Sized,
    {
        let region = addr as usize >> 15 & 3;
        let mapped = self.map.b_bg[region].get();
        if mapped == 0 {
            return;
        }
        let addr = addr as usize & (0x1_FFFF & !(mem::size_of::<T>() - 1));
        unsafe {
            set_writeback!(self.writeback.b_bg.get_mut(), addr, T);
            self.b_bg.write_le_aligned_unchecked(addr, value);
        }
        if mapped & 6 != 0 {
            match mapped {
                2 => {
                    let mirror_addr = addr ^ 0x1_0000;
                    unsafe {
                        self.b_bg.write_le_aligned_unchecked(mirror_addr, value);
                        clear_writeback!(self.writeback.b_bg.get_mut(), mirror_addr, T);
                    }
                }
                4 => {
                    for mirror_addr in [addr ^ 0x4000, addr ^ 0x1_0000, addr ^ 0x1_4000] {
                        unsafe {
                            self.b_bg.write_le_aligned_unchecked(mirror_addr, value);
                            clear_writeback!(self.writeback.b_bg.get_mut(), mirror_addr, T);
                        }
                    }
                }
                _ => self.handle_b_bg_mirroring(addr, mapped, value),
            }
        }
    }

    #[inline]
    pub fn read_b_obj<T: MemValue>(&self, addr: u32) -> T {
        unsafe {
            self.b_obj
                .read_le_aligned_unchecked(addr as usize & (0x1_FFFF & !(mem::size_of::<T>() - 1)))
        }
    }

    #[inline(never)]
    fn handle_b_obj_mirroring<T: MemValue + BitOr<Output = T> + BitOrAssign>(
        &mut self,
        addr: usize,
        value: T,
    ) where
        [(); mem::size_of::<T>()]: Sized,
    {
        for mirror_addr in [
            addr ^ 0x4000,
            addr ^ 0x8000,
            addr ^ 0xC000,
            addr ^ 0x1_0000,
            addr ^ 0x1_4000,
            addr ^ 0x1_8000,
            addr ^ 0x1_C000,
        ] {
            handle_mirroring!(
                self, b_obj, value, T, mirror_addr, 1;
                0 => d,
            );
        }
    }

    #[inline]
    pub fn write_b_obj<T: MemValue + BitOr<Output = T> + BitOrAssign>(
        &mut self,
        addr: u32,
        value: T,
    ) where
        [(); mem::size_of::<T>()]: Sized,
    {
        let mapped = self.map.b_obj[0].get();
        if mapped == 0 {
            return;
        }
        let addr = addr as usize & (0x1_FFFF & !(mem::size_of::<T>() - 1));
        unsafe {
            set_writeback!(self.writeback.b_obj.get_mut(), addr, T);
            self.b_obj.write_le_aligned_unchecked(addr, value);
        }
        if mapped & 2 != 0 {
            if mapped & !2 == 0 {
                for mirror_addr in [
                    addr ^ 0x4000,
                    addr ^ 0x8000,
                    addr ^ 0xC000,
                    addr ^ 0x1_0000,
                    addr ^ 0x1_4000,
                    addr ^ 0x1_8000,
                    addr ^ 0x1_C000,
                ] {
                    unsafe {
                        self.b_obj.write_le_aligned_unchecked(mirror_addr, value);
                        clear_writeback!(self.writeback.b_obj.get_mut(), mirror_addr, T);
                    }
                }
            } else {
                self.handle_b_obj_mirroring(addr, value);
            }
        }
    }

    #[inline]
    pub fn read_a_bg_ext_pal<T: MemValue>(&self, addr: u32) -> T {
        unsafe {
            self.a_bg_ext_pal
                .read_le_aligned_unchecked(addr as usize & (0x7FFF & !(mem::size_of::<T>() - 1)))
        }
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `addr + result.len()` must be less than or equal to `0x8000`
    #[inline]
    pub unsafe fn read_a_bg_ext_pal_slice<T: MemValue>(
        &self,
        addr: u32,
        len: usize,
        result: *mut T,
    ) {
        ptr::copy_nonoverlapping(
            self.a_bg_ext_pal.as_ptr().add(addr as usize) as *const T,
            result,
            len / mem::size_of::<T>(),
        );
    }

    #[inline]
    pub fn read_a_obj_ext_pal<T: MemValue>(&self, addr: u32) -> T {
        unsafe {
            self.a_bg_ext_pal
                .read_le_aligned_unchecked(addr as usize & (0x1FFF & !(mem::size_of::<T>() - 1)))
        }
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `addr + result.len()` must be less than or equal to `0x2000`
    #[inline]
    pub unsafe fn read_a_obj_ext_pal_slice<T: MemValue>(
        &self,
        addr: u32,
        len: usize,
        result: *mut T,
    ) {
        ptr::copy_nonoverlapping(
            self.a_obj_ext_pal.as_ptr().add(addr as usize) as *const T,
            result,
            len / mem::size_of::<T>(),
        );
    }

    #[inline]
    pub fn read_b_bg_ext_pal<T: MemValue>(&self, addr: u32) -> T {
        // NOTE: As for LCDC, the pointer will never be null
        unsafe {
            (self
                .b_bg_ext_pal_ptr
                .add(addr as usize & (0x7FFF & !(mem::size_of::<T>() - 1)))
                as *const T)
                .read()
        }
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `addr + result.len()` must be less than or equal to `0x8000`
    #[inline]
    pub unsafe fn read_b_bg_ext_pal_slice<T: MemValue>(
        &self,
        addr: u32,
        len: usize,
        result: *mut T,
    ) {
        // NOTE: As for LCDC, the pointer will never be null
        ptr::copy_nonoverlapping(
            self.b_bg_ext_pal_ptr.add(addr as usize) as *const T,
            result,
            len / mem::size_of::<T>(),
        );
    }

    #[inline]
    pub fn read_b_obj_ext_pal<T: MemValue>(&self, addr: u32) -> T {
        // NOTE: As for LCDC, the pointer will never be null
        unsafe {
            (self
                .b_obj_ext_pal_ptr
                .add(addr as usize & (0x1FFF & !(mem::size_of::<T>() - 1)))
                as *const T)
                .read()
        }
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `addr + result.len()` must be less than or equal to `0x2000`
    #[inline]
    pub unsafe fn read_b_obj_ext_pal_slice<T: MemValue>(
        &self,
        addr: u32,
        len: usize,
        result: *mut T,
    ) {
        // NOTE: As for LCDC, the pointer will never be null
        ptr::copy_nonoverlapping(
            self.b_obj_ext_pal_ptr.add(addr as usize) as *const T,
            result,
            len / mem::size_of::<T>(),
        );
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `addr + result.len()` must be less than or equal to `0x8_0000`
    #[inline]
    pub unsafe fn read_texture_slice<T: MemValue>(&self, addr: u32, len: usize, result: *mut T) {
        ptr::copy_nonoverlapping(
            self.texture.as_ptr().add(addr as usize) as *const T,
            result,
            len / mem::size_of::<T>(),
        );
    }

    /// # Safety
    /// - `result`'s start and end must be aligned to a `T` boundary
    /// - `addr` must be aligned to a `T` boundary
    /// - `addr + result.len()` must be less than or equal to `0x1_8000`
    #[inline]
    pub unsafe fn read_tex_pal_slice<T: MemValue>(&self, addr: u32, len: usize, result: *mut T) {
        ptr::copy_nonoverlapping(
            self.tex_pal.as_ptr().add(addr as usize) as *const T,
            result,
            len / mem::size_of::<T>(),
        );
    }

    #[inline]
    pub fn read_arm7<T: MemValue>(&self, addr: u32) -> T {
        unsafe {
            self.arm7
                .read_le_aligned_unchecked(addr as usize & (0x3_FFFF & !(mem::size_of::<T>() - 1)))
        }
    }

    #[inline]
    pub fn write_arm7<T: MemValue>(&mut self, addr: u32, value: T) {
        let region = addr as usize >> 17 & 1;
        if self.map.arm7[region].get() == 0 {
            return;
        }
        let addr = addr as usize & (0x3_FFFF & !(mem::size_of::<T>() - 1));
        unsafe {
            set_writeback!(self.writeback.arm7.get_mut(), addr, T);
            self.arm7.write_le_aligned_unchecked(addr, value);
        }
    }
}
