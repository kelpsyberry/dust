use super::{
    super::engine_2d::{Engine2d, EngineA, EngineB},
    BankControl, Vram,
};
use crate::{
    cpu::{
        self,
        arm7::Arm7,
        arm9::{bus::ptrs::mask as ptr_mask, Arm9},
    },
    utils::make_zero,
};
use core::ptr;

// TODO: Find out what happens with invalid VRAM bank MST values.

macro_rules! large_bank_ptrs {
    ($self: expr, $ptr_region: expr, $name: ident, $mask: expr) => {{
        let offset = $ptr_region << 14 & $mask;
        (
            $self.banks.$name.as_ptr().add(offset),
            $self.banks.$name.as_ptr().add(offset),
        )
    }};
}

impl Vram {
    fn bank_ptrs_a_bg(&mut self, mask: u8, ptr_region: usize) -> (*const u8, *mut u8) {
        unsafe {
            match mask {
                0x00 => (self.banks.zero.as_ptr(), self.banks.ignore.as_ptr()),
                0x01 => large_bank_ptrs!(self, ptr_region, a, 0x1_C000),
                0x02 => large_bank_ptrs!(self, ptr_region, b, 0x1_C000),
                0x04 => large_bank_ptrs!(self, ptr_region, c, 0x1_C000),
                0x08 => large_bank_ptrs!(self, ptr_region, d, 0x1_C000),
                0x10 => large_bank_ptrs!(self, ptr_region, e, 0xC000),
                0x20 => (self.banks.f.as_ptr(), self.banks.f.as_ptr()),
                0x40 => (self.banks.g.as_ptr(), self.banks.g.as_ptr()),
                _ => (ptr::null(), ptr::null_mut()),
            }
        }
    }

    fn bank_ptrs_a_obj(&mut self, mask: u8, ptr_region: usize) -> (*const u8, *mut u8) {
        unsafe {
            match mask {
                0x00 => (self.banks.zero.as_ptr(), self.banks.ignore.as_ptr()),
                0x01 => large_bank_ptrs!(self, ptr_region, a, 0x1_C000),
                0x02 => large_bank_ptrs!(self, ptr_region, b, 0x1_C000),
                0x04 => large_bank_ptrs!(self, ptr_region, e, 0xC000),
                0x08 => (self.banks.f.as_ptr(), self.banks.f.as_ptr()),
                0x10 => (self.banks.g.as_ptr(), self.banks.g.as_ptr()),
                _ => (ptr::null(), ptr::null_mut()),
            }
        }
    }

    unsafe fn map_lcdc<E: cpu::Engine>(
        &mut self,
        arm9: &mut Arm9<E>,
        regions_lower_bound: usize,
        regions_upper_bound: usize,
        ptr: *mut u8,
    ) {
        {
            let mut ptr = ptr;
            for region in regions_lower_bound..=regions_upper_bound {
                self.map.lcdc_r_ptrs[region] = ptr;
                self.map.lcdc_w_ptrs[region] = ptr;
                ptr = ptr.add(0x4000);
            }
        }
        let lower_bound = 0x0680_0000 | (regions_lower_bound as u32) << 14;
        let upper_bound = 0x0680_0000 | (regions_upper_bound as u32) << 14 | 0x3FFF;
        let size = (regions_upper_bound - regions_lower_bound + 1) << 14;
        for mirror_base in (0..0x80_0000).step_by(0x10_0000) {
            arm9.map_sys_bus_ptr_range(
                ptr_mask::ALL & !ptr_mask::W_8,
                ptr,
                size,
                (mirror_base | lower_bound, mirror_base | upper_bound),
            );
        }
    }

    fn unmap_lcdc<E: cpu::Engine>(
        &mut self,
        arm9: &mut Arm9<E>,
        regions_lower_bound: usize,
        regions_upper_bound: usize,
    ) {
        let lower_bound = 0x0680_0000 | (regions_lower_bound as u32) << 14;
        let upper_bound = 0x0680_0000 | (regions_upper_bound as u32) << 14 | 0x3FFF;
        for region in regions_lower_bound..=regions_upper_bound {
            self.map.lcdc_r_ptrs[region] = self.banks.zero.as_ptr();
            self.map.lcdc_w_ptrs[region] = self.banks.ignore.as_ptr();
        }
        for mirror_base in (0..0x80_0000).step_by(0x10_0000) {
            arm9.unmap_sys_bus_ptr_range((mirror_base | lower_bound, mirror_base | upper_bound));
        }
    }

    fn set_range_b_bg<E: cpu::Engine>(
        &mut self,
        arm9: &mut Arm9<E>,
        apply_mask: impl Fn(u8) -> u8,
        range: impl IntoIterator<Item = usize>,
    ) {
        for region in range {
            let mask = apply_mask(self.map.b_bg[region]);
            self.map.b_bg[region] = mask;
            for ptr_region in [region << 1, region << 1 | 1] {
                unsafe {
                    let (r_ptr, w_ptr) = match mask {
                        0x00 => (self.banks.zero.as_ptr(), self.banks.ignore.as_ptr()),
                        0x01 => large_bank_ptrs!(self, ptr_region, c, 0x1_C000),
                        0x02 => large_bank_ptrs!(self, ptr_region, h, 0x4000),
                        0x04 => (self.banks.i.as_ptr(), self.banks.i.as_ptr()),
                        _ => (ptr::null_mut(), ptr::null_mut()),
                    };
                    self.map.b_bg_r_ptrs[ptr_region] = r_ptr;
                    self.map.b_bg_w_ptrs[ptr_region] = w_ptr;
                    let lower_bound = 0x0620_0000 | (ptr_region as u32) << 14;
                    let upper_bound = lower_bound | 0x3FFF;
                    if r_ptr != w_ptr || r_ptr.is_null() {
                        for mirror_base in (0..0x20_0000).step_by(0x2_0000) {
                            arm9.unmap_sys_bus_ptr_range((
                                mirror_base | lower_bound,
                                mirror_base | upper_bound,
                            ));
                        }
                    } else {
                        for mirror_base in (0..0x20_0000).step_by(0x2_0000) {
                            arm9.map_sys_bus_ptr_range(
                                ptr_mask::ALL & !ptr_mask::W_8,
                                w_ptr,
                                0x4000,
                                (mirror_base | lower_bound, mirror_base | upper_bound),
                            );
                        }
                    }
                }
            }
        }
    }

    fn set_b_obj<E: cpu::Engine>(&mut self, arm9: &mut Arm9<E>, apply_mask: impl Fn(u8) -> u8) {
        let mask = apply_mask(self.map.b_obj);
        self.map.b_obj = mask;
        unsafe {
            match mask {
                0x00 => {
                    self.map.b_obj_r_ptrs.fill(self.banks.zero.as_ptr());
                    self.map.b_obj_w_ptrs.fill(self.banks.ignore.as_ptr());
                    arm9.unmap_sys_bus_ptr_range((0x0660_0000, 0x067F_FFFF));
                }
                0x01 => {
                    let mut ptr = self.banks.d.as_ptr();
                    for ptr_region in 0..8 {
                        self.map.b_obj_r_ptrs[ptr_region] = ptr;
                        self.map.b_obj_w_ptrs[ptr_region] = ptr;
                        ptr = ptr.add(0x4000);
                    }
                    arm9.map_sys_bus_ptr_range(
                        ptr_mask::ALL & !ptr_mask::W_8,
                        self.banks.d.as_ptr(),
                        0x2_0000,
                        (0x0660_0000, 0x067F_FFFF),
                    );
                }
                0x02 => {
                    self.map.b_obj_r_ptrs.fill(self.banks.i.as_ptr());
                    self.map.b_obj_w_ptrs.fill(self.banks.i.as_ptr());
                    arm9.map_sys_bus_ptr_range(
                        ptr_mask::ALL & !ptr_mask::W_8,
                        self.banks.i.as_ptr(),
                        0x4000,
                        (0x0660_0000, 0x067F_FFFF),
                    );
                }
                _ => {
                    make_zero(&mut self.map.b_obj_r_ptrs);
                    make_zero(&mut self.map.b_obj_w_ptrs);
                    arm9.unmap_sys_bus_ptr_range((0x0660_0000, 0x067F_FFFF));
                }
            }
        }
    }

    fn set_region_texture(&mut self, apply_mask: impl Fn(u8) -> u8, region: usize) {
        let mask = apply_mask(self.map.texture[region]);
        self.map.texture[region] = mask;
        self.map.texture_ptrs[region] = match mask {
            0x00 => self.banks.zero.as_ptr(),
            0x01 => self.banks.a.as_ptr(),
            0x02 => self.banks.b.as_ptr(),
            0x04 => self.banks.c.as_ptr(),
            0x08 => self.banks.d.as_ptr(),
            _ => ptr::null(),
        };
    }

    fn set_range_tex_pal(
        &mut self,
        apply_mask: impl Fn(u8) -> u8,
        range: impl IntoIterator<Item = usize>,
    ) {
        for region in range {
            let mask = apply_mask(self.map.tex_pal[region]);
            self.map.tex_pal[region] = mask;
            self.map.tex_pal_ptrs[region] = match mask {
                0x00 => self.banks.zero.as_ptr(),
                0x01 => unsafe { self.banks.e.as_ptr().add(region << 14 & 0xC000) },
                0x02 => self.banks.f.as_ptr(),
                0x04 => self.banks.g.as_ptr(),
                _ => ptr::null(),
            };
        }
    }

    fn set_range_a_bg_ext_pal(
        &mut self,
        apply_mask: impl Fn(u8) -> u8,
        range: impl IntoIterator<Item = usize>,
    ) {
        for region in range {
            let mask = apply_mask(self.map.a_bg_ext_pal[region]);
            self.map.a_bg_ext_pal[region] = mask;
            self.map.a_bg_ext_pal_ptrs[region] = match mask {
                0x00 => self.banks.zero.as_ptr(),
                0x01 => unsafe { self.banks.e.as_ptr().add(region << 14 & 0x4000) },
                0x02 => self.banks.f.as_ptr(),
                0x04 => self.banks.g.as_ptr(),
                _ => ptr::null(),
            };
        }
    }

    fn set_a_obj_ext_pal(&mut self, apply_mask: impl Fn(u8) -> u8) {
        let mask = apply_mask(self.map.a_obj_ext_pal);
        self.map.a_obj_ext_pal = mask;
        self.map.a_obj_ext_pal_ptr = match mask {
            0x00 => self.banks.zero.as_ptr(),
            0x01 => self.banks.f.as_ptr(),
            0x02 => self.banks.g.as_ptr(),
            _ => ptr::null(),
        };
    }

    fn set_region_arm7<E: cpu::Engine>(
        &mut self,
        arm7: &mut Arm7<E>,
        apply_mask: impl Fn(u8) -> u8,
        region: usize,
    ) {
        let mask = apply_mask(self.map.arm7[region]);
        self.map.arm7[region] = mask;
        let (r_ptr, w_ptr) = match mask {
            0x00 => (self.banks.zero.as_ptr(), self.banks.ignore.as_ptr()),
            0x01 => (self.banks.c.as_ptr(), self.banks.c.as_ptr()),
            0x02 => (self.banks.d.as_ptr(), self.banks.d.as_ptr()),
            _ => (ptr::null_mut(), ptr::null_mut()),
        };
        self.map.arm7_r_ptrs[region] = r_ptr;
        self.map.arm7_w_ptrs[region] = w_ptr;
        let lower_bound = 0x0600_0000 | (region as u32) << 17;
        let upper_bound = lower_bound | 0x1_FFFF;
        if r_ptr != w_ptr || r_ptr.is_null() {
            for mirror_base in (0..0x100_0000).step_by(0x4_0000) {
                arm7.unmap_bus_ptr_range((mirror_base | lower_bound, mirror_base | upper_bound));
            }
        } else {
            for mirror_base in (0..0x100_0000).step_by(0x4_0000) {
                unsafe {
                    arm7.map_bus_ptr_range(
                        w_ptr,
                        0x2_0000,
                        (mirror_base | lower_bound, mirror_base | upper_bound),
                    );
                }
            }
        }
    }
}

macro_rules! set_range_a_bg_obj {
    (
        $self: expr,
        $arm9: expr,
        ($ptrs_fn: ident, $arr: ident, $ptr_arr_r: ident, $ptr_arr_w: ident),
        |$prev_mask_ident: ident| $apply_mask: expr,
        $range: expr,
        $bus_ptrs_base: expr,
        $mirror_stride: expr$(,)?
    ) => {
        for region in $range {
            let mask = {
                let $prev_mask_ident = $self.map.$arr[region];
                $apply_mask
            };
            $self.map.$arr[region] = mask;
            let (r_ptr, w_ptr) = $self.$ptrs_fn(mask, region);
            $self.map.$ptr_arr_r[region] = r_ptr;
            $self.map.$ptr_arr_w[region] = w_ptr;
            let lower_bound = $bus_ptrs_base | ((region as u32) << 14);
            let upper_bound = lower_bound | 0x3FFF;
            if r_ptr != w_ptr || r_ptr.is_null() {
                // Separate read and write pointers can't be represented by the pointer LUTs, so
                // that will fallback to the slow handlers as if the pointers were null.
                for mirror_base in (0..0x20_0000).step_by($mirror_stride) {
                    $arm9.unmap_sys_bus_ptr_range((
                        mirror_base | lower_bound,
                        mirror_base | upper_bound,
                    ));
                }
            } else {
                for mirror_base in (0..0x20_0000).step_by($mirror_stride) {
                    unsafe {
                        $arm9.map_sys_bus_ptr_range(
                            ptr_mask::ALL & !ptr_mask::W_8,
                            w_ptr,
                            0x4000,
                            (mirror_base | lower_bound, mirror_base | upper_bound),
                        );
                    }
                }
            }
        }
    };
}

impl Vram {
    pub fn set_bank_control_a<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm9: &mut Arm9<E>,
    ) {
        value.0 &= 0x9B;
        {
            let prev_value = self.bank_control[0];
            if value == prev_value {
                return;
            }
            self.bank_control[0] = value;
            if prev_value.enabled() {
                match prev_value.mst() & 3 {
                    0 => {
                        self.unmap_lcdc(arm9, 0, 7);
                    }
                    1 => {
                        let base_region = (prev_value.offset() as usize) << 3;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m & !0x01,
                            base_region..=base_region | 7,
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        let base_region = (prev_value.offset() as usize & 1) << 3;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_obj, a_obj, a_obj_r_ptrs, a_obj_w_ptrs),
                            |m| m & !0x01,
                            base_region..=base_region | 7,
                            0x0640_0000,
                            0x4_0000,
                        );
                    }
                    _ => {
                        let region = prev_value.offset() as usize;
                        self.set_region_texture(|m| m & !0x01, region);
                    }
                }
            }
        }
        {
            if value.enabled() {
                match value.mst() & 3 {
                    0 => unsafe {
                        self.map_lcdc(arm9, 0, 7, self.banks.a.as_ptr());
                    },
                    1 => {
                        let base_region = (value.offset() as usize) << 3;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m | 0x01,
                            base_region..=base_region | 7,
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        let base_region = (value.offset() as usize & 1) << 3;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_obj, a_obj, a_obj_r_ptrs, a_obj_w_ptrs),
                            |m| m | 0x01,
                            base_region..=base_region | 7,
                            0x0640_0000,
                            0x4_0000,
                        );
                    }
                    _ => {
                        let region = value.offset() as usize;
                        self.set_region_texture(|m| m | 0x01, region);
                    }
                }
            }
        }
    }

    pub fn set_bank_control_b<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm9: &mut Arm9<E>,
    ) {
        value.0 &= 0x9B;
        {
            let prev_value = self.bank_control[1];
            if value == prev_value {
                return;
            }
            self.bank_control[1] = value;
            if prev_value.enabled() {
                match prev_value.mst() & 3 {
                    0 => {
                        self.unmap_lcdc(arm9, 8, 0xF);
                    }
                    1 => {
                        let base_region = (prev_value.offset() as usize) << 3;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m & !0x02,
                            base_region..=base_region | 7,
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        let base_region = (prev_value.offset() as usize & 1) << 3;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_obj, a_obj, a_obj_r_ptrs, a_obj_w_ptrs),
                            |m| m & !0x02,
                            base_region..=base_region | 7,
                            0x0640_0000,
                            0x4_0000,
                        );
                    }
                    _ => {
                        let region = prev_value.offset() as usize;
                        self.set_region_texture(|m| m & !0x02, region);
                    }
                }
            }
        }
        {
            if value.enabled() {
                match value.mst() & 3 {
                    0 => unsafe {
                        self.map_lcdc(arm9, 8, 0xF, self.banks.b.as_ptr());
                    },
                    1 => {
                        let base_region = (value.offset() as usize) << 3;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m | 0x02,
                            base_region..=base_region | 7,
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        let base_region = (value.offset() as usize & 1) << 3;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_obj, a_obj, a_obj_r_ptrs, a_obj_w_ptrs),
                            |m| m | 0x02,
                            base_region..=base_region | 7,
                            0x0640_0000,
                            0x4_0000,
                        );
                    }
                    _ => {
                        let region = value.offset() as usize;
                        self.set_region_texture(|m| m | 0x02, region);
                    }
                }
            }
        }
    }

    pub fn set_bank_control_c<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm7: &mut Arm7<E>,
        arm9: &mut Arm9<E>,
    ) {
        value.0 &= 0x9F;
        {
            let prev_value = self.bank_control[2];
            if value == prev_value {
                return;
            }
            self.bank_control[2] = value;
            if prev_value.enabled() {
                match prev_value.mst() {
                    0 => {
                        self.unmap_lcdc(arm9, 0x10, 0x17);
                    }
                    1 => {
                        let base_region = (prev_value.offset() as usize) << 3;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m & !0x04,
                            base_region..=base_region | 7,
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        let region = prev_value.offset() as usize & 1;
                        self.set_region_arm7(arm7, |m| m & !0x01, region);
                        self.arm7_status.set_c_used_as_arm7(false);
                    }
                    3 => {
                        let region = prev_value.offset() as usize;
                        self.set_region_texture(|m| m & !0x04, region);
                    }
                    4 => {
                        self.set_range_b_bg(arm9, |m| m & !0x01, 0..=3);
                    }
                    _ => {
                        unimplemented!("Specified invalid mapping for bank C: {}", prev_value.mst())
                    }
                }
            }
        }
        {
            if value.enabled() {
                match value.mst() {
                    0 => unsafe {
                        self.map_lcdc(arm9, 0x10, 0x17, self.banks.c.as_ptr());
                    },
                    1 => {
                        let base_region = (value.offset() as usize) << 3;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m | 0x04,
                            base_region..=base_region | 7,
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        let region = value.offset() as usize & 1;
                        self.set_region_arm7(arm7, |m| m | 0x01, region);
                        self.arm7_status.set_c_used_as_arm7(true);
                    }
                    3 => {
                        let region = value.offset() as usize;
                        self.set_region_texture(|m| m | 0x04, region);
                    }
                    4 => {
                        self.set_range_b_bg(arm9, |m| m | 0x01, 0..=3);
                    }
                    _ => {
                        unimplemented!("Specified invalid mapping for bank C: {}", value.mst())
                    }
                }
            }
        }
    }

    pub fn set_bank_control_d<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm7: &mut Arm7<E>,
        arm9: &mut Arm9<E>,
    ) {
        value.0 &= 0x9F;
        {
            let prev_value = self.bank_control[3];
            if value == prev_value {
                return;
            }
            self.bank_control[3] = value;
            if prev_value.enabled() {
                match prev_value.mst() {
                    0 => {
                        self.unmap_lcdc(arm9, 0x18, 0x1F);
                    }
                    1 => {
                        let base_region = (prev_value.offset() as usize) << 3;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m & !0x08,
                            base_region..=base_region | 7,
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        let region = prev_value.offset() as usize & 1;
                        self.set_region_arm7(arm7, |m| m & !0x02, region);
                        self.arm7_status.set_d_used_as_arm7(false);
                    }
                    3 => {
                        let region = prev_value.offset() as usize;
                        self.set_region_texture(|m| m & !0x08, region);
                    }
                    4 => {
                        self.set_b_obj(arm9, |m| m & !0x01);
                    }
                    _ => {
                        unimplemented!("Specified invalid mapping for bank D: {}", prev_value.mst())
                    }
                }
            }
        }
        {
            if value.enabled() {
                match value.mst() {
                    0 => unsafe {
                        self.map_lcdc(arm9, 0x18, 0x1F, self.banks.d.as_ptr());
                    },
                    1 => {
                        let base_region = (value.offset() as usize) << 3;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m | 0x08,
                            base_region..=base_region | 7,
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        let region = value.offset() as usize & 1;
                        self.set_region_arm7(arm7, |m| m | 0x02, region);
                        self.arm7_status.set_d_used_as_arm7(true);
                    }
                    3 => {
                        let region = value.offset() as usize;
                        self.set_region_texture(|m| m | 0x08, region);
                    }
                    4 => {
                        self.set_b_obj(arm9, |m| m | 0x01);
                    }
                    _ => {
                        unimplemented!("Specified invalid mapping for bank D: {}", value.mst())
                    }
                }
            }
        }
    }

    pub fn set_bank_control_e<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm9: &mut Arm9<E>,
        engine_2d_a: &mut Engine2d<EngineA>,
    ) {
        value.0 &= 0x87;
        {
            let prev_value = self.bank_control[4];
            if value == prev_value {
                return;
            }
            self.bank_control[4] = value;
            if prev_value.enabled() {
                match prev_value.mst() {
                    0 => {
                        self.unmap_lcdc(arm9, 0x20, 0x23);
                    }
                    1 => {
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m & !0x10,
                            0..=3,
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_obj, a_obj, a_obj_r_ptrs, a_obj_w_ptrs),
                            |m| m & !0x04,
                            0..=3,
                            0x0640_0000,
                            0x4_0000,
                        );
                    }
                    3 => {
                        self.set_range_tex_pal(|m| m & !0x01, 0..=3);
                    }
                    4 => {
                        self.set_range_a_bg_ext_pal(|m| m & !0x01, 0..=1);
                        engine_2d_a.invalidate_bg_ext_pal_cache(0);
                        engine_2d_a.invalidate_bg_ext_pal_cache(1);
                    }
                    _ => {
                        unimplemented!("Specified invalid mapping for bank E: {}", prev_value.mst())
                    }
                }
            }
        }
        {
            if value.enabled() {
                match value.mst() {
                    0 => unsafe {
                        self.map_lcdc(arm9, 0x20, 0x23, self.banks.e.as_ptr());
                    },
                    1 => {
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m | 0x10,
                            0..=3,
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_obj, a_obj, a_obj_r_ptrs, a_obj_w_ptrs),
                            |m| m | 0x04,
                            0..=3,
                            0x0640_0000,
                            0x4_0000,
                        );
                    }
                    3 => {
                        self.set_range_tex_pal(|m| m | 0x01, 0..=3);
                    }
                    4 => {
                        self.set_range_a_bg_ext_pal(|m| m | 0x01, 0..=1);
                        engine_2d_a.invalidate_bg_ext_pal_cache(0);
                        engine_2d_a.invalidate_bg_ext_pal_cache(1);
                    }
                    _ => {
                        unimplemented!("Specified invalid mapping for bank E: {}", value.mst())
                    }
                }
            }
        }
    }

    pub fn set_bank_control_f<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm9: &mut Arm9<E>,
        engine_2d_a: &mut Engine2d<EngineA>,
    ) {
        value.0 &= 0x9F;
        {
            let prev_value = self.bank_control[5];
            if value == prev_value {
                return;
            }
            self.bank_control[5] = value;
            if prev_value.enabled() {
                match prev_value.mst() {
                    0 => {
                        self.unmap_lcdc(arm9, 0x24, 0x24);
                    }
                    1 => {
                        let base_region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m & !0x20,
                            [base_region, base_region | 2],
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        let base_region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_obj, a_obj, a_obj_r_ptrs, a_obj_w_ptrs),
                            |m| m & !0x08,
                            [base_region, base_region | 2],
                            0x0640_0000,
                            0x4_0000,
                        );
                    }
                    3 => {
                        let region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        self.set_range_tex_pal(|m| m & !0x02, core::iter::once(region));
                    }
                    4 => {
                        let region = prev_value.offset() & 1;
                        self.set_range_a_bg_ext_pal(
                            |m| m & !0x02,
                            core::iter::once(region as usize),
                        );
                        engine_2d_a.invalidate_bg_ext_pal_cache(region);
                    }
                    5 => {
                        self.set_a_obj_ext_pal(|m| m & !0x01);
                        engine_2d_a.invalidate_obj_ext_pal_cache();
                    }
                    _ => {
                        unimplemented!("Specified invalid mapping for bank F: {}", prev_value.mst())
                    }
                }
            }
        }
        {
            if value.enabled() {
                match value.mst() {
                    0 => unsafe {
                        self.map_lcdc(arm9, 0x24, 0x24, self.banks.f.as_ptr());
                    },
                    1 => {
                        let base_region =
                            ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m | 0x20,
                            [base_region, base_region | 2],
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        let base_region =
                            ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_obj, a_obj, a_obj_r_ptrs, a_obj_w_ptrs),
                            |m| m | 0x08,
                            [base_region, base_region | 2],
                            0x0640_0000,
                            0x4_0000,
                        );
                    }
                    3 => {
                        let region = ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        self.set_range_tex_pal(|m| m | 0x02, core::iter::once(region));
                    }
                    4 => {
                        let region = value.offset() & 1;
                        self.set_range_a_bg_ext_pal(
                            |m| m | 0x02,
                            core::iter::once(region as usize),
                        );
                        engine_2d_a.invalidate_bg_ext_pal_cache(region);
                    }
                    5 => {
                        self.set_a_obj_ext_pal(|m| m | 0x01);
                        engine_2d_a.invalidate_obj_ext_pal_cache();
                    }
                    _ => {
                        unimplemented!("Specified invalid mapping for bank F: {}", value.mst())
                    }
                }
            }
        }
    }

    pub fn set_bank_control_g<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm9: &mut Arm9<E>,
        engine_2d_a: &mut Engine2d<EngineA>,
    ) {
        value.0 &= 0x9F;
        {
            let prev_value = self.bank_control[6];
            if value == prev_value {
                return;
            }
            self.bank_control[6] = value;
            if prev_value.enabled() {
                match prev_value.mst() {
                    0 => {
                        self.unmap_lcdc(arm9, 0x25, 0x25);
                    }
                    1 => {
                        let base_region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m & !0x40,
                            [base_region, base_region | 2],
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        let base_region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_obj, a_obj, a_obj_r_ptrs, a_obj_w_ptrs),
                            |m| m & !0x10,
                            [base_region, base_region | 2],
                            0x0640_0000,
                            0x4_0000,
                        );
                    }
                    3 => {
                        let region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        self.set_range_tex_pal(|m| m & !0x04, core::iter::once(region));
                    }
                    4 => {
                        let region = prev_value.offset() & 1;
                        self.set_range_a_bg_ext_pal(
                            |m| m & !0x04,
                            core::iter::once(region as usize),
                        );
                        engine_2d_a.invalidate_bg_ext_pal_cache(region);
                    }
                    5 => {
                        self.set_a_obj_ext_pal(|m| m & !0x02);
                        engine_2d_a.invalidate_obj_ext_pal_cache();
                    }
                    _ => {
                        unimplemented!("Specified invalid mapping for bank G: {}", prev_value.mst())
                    }
                }
            }
        }
        {
            if value.enabled() {
                match value.mst() {
                    0 => unsafe {
                        self.map_lcdc(arm9, 0x25, 0x25, self.banks.g.as_ptr());
                    },
                    1 => {
                        let base_region =
                            ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_bg, a_bg, a_bg_r_ptrs, a_bg_w_ptrs),
                            |m| m | 0x40,
                            [base_region, base_region | 2],
                            0x0600_0000,
                            0x8_0000,
                        );
                    }
                    2 => {
                        let base_region =
                            ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        set_range_a_bg_obj!(
                            self,
                            arm9,
                            (bank_ptrs_a_obj, a_obj, a_obj_r_ptrs, a_obj_w_ptrs),
                            |m| m | 0x10,
                            [base_region, base_region | 2],
                            0x0640_0000,
                            0x4_0000,
                        );
                    }
                    3 => {
                        let region = ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        self.set_range_tex_pal(|m| m | 0x04, core::iter::once(region));
                    }
                    4 => {
                        let region = value.offset() & 1;
                        self.set_range_a_bg_ext_pal(
                            |m| m | 0x04,
                            core::iter::once(region as usize),
                        );
                        engine_2d_a.invalidate_bg_ext_pal_cache(region);
                    }
                    5 => {
                        self.set_a_obj_ext_pal(|m| m | 0x02);
                        engine_2d_a.invalidate_obj_ext_pal_cache();
                    }
                    _ => {
                        unimplemented!("Specified invalid mapping for bank G: {}", value.mst())
                    }
                }
            }
        }
    }

    pub fn set_bank_control_h<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm9: &mut Arm9<E>,
        engine_2d_b: &mut Engine2d<EngineB>,
    ) {
        value.0 &= 0x83;
        {
            let prev_value = self.bank_control[7];
            if value == prev_value {
                return;
            }
            self.bank_control[7] = value;
            if prev_value.enabled() {
                match prev_value.mst() & 3 {
                    0 => {
                        self.unmap_lcdc(arm9, 0x26, 0x27);
                    }
                    1 => {
                        self.set_range_b_bg(arm9, |m| m & !0x02, core::iter::once(0));
                        self.set_range_b_bg(arm9, |m| m & !0x02, core::iter::once(2));
                    }
                    2 => {
                        self.map.b_bg_ext_pal_ptr = self.banks.zero.as_ptr();
                        engine_2d_b.invalidate_bg_ext_pal_cache(0);
                        engine_2d_b.invalidate_bg_ext_pal_cache(1);
                    }
                    _ => {
                        unimplemented!("Specified invalid mapping for bank H: {}", prev_value.mst())
                    }
                }
            }
        }
        {
            if value.enabled() {
                match value.mst() & 3 {
                    0 => unsafe {
                        self.map_lcdc(arm9, 0x26, 0x27, self.banks.h.as_ptr());
                    },
                    1 => {
                        self.set_range_b_bg(arm9, |m| m | 0x02, core::iter::once(0));
                        self.set_range_b_bg(arm9, |m| m | 0x02, core::iter::once(2));
                    }
                    2 => {
                        self.map.b_bg_ext_pal_ptr = self.banks.h.as_ptr();
                        engine_2d_b.invalidate_bg_ext_pal_cache(0);
                        engine_2d_b.invalidate_bg_ext_pal_cache(1);
                    }
                    _ => {
                        unimplemented!("Specified invalid mapping for bank H: {}", value.mst())
                    }
                }
            }
        }
    }

    pub fn set_bank_control_i<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm9: &mut Arm9<E>,
        engine_2d_b: &mut Engine2d<EngineB>,
    ) {
        value.0 &= 0x83;
        {
            let prev_value = self.bank_control[8];
            if value == prev_value {
                return;
            }
            self.bank_control[8] = value;
            if prev_value.enabled() {
                match prev_value.mst() & 3 {
                    0 => {
                        self.unmap_lcdc(arm9, 0x28, 0x28);
                    }
                    1 => {
                        self.set_range_b_bg(arm9, |m| m & !0x04, core::iter::once(1));
                        self.set_range_b_bg(arm9, |m| m & !0x04, core::iter::once(3));
                    }
                    2 => {
                        self.set_b_obj(arm9, |m| m & !0x02);
                    }
                    _ => {
                        self.map.b_obj_ext_pal_ptr = self.banks.zero.as_ptr();
                        engine_2d_b.invalidate_obj_ext_pal_cache();
                    }
                }
            }
        }
        {
            if value.enabled() {
                match value.mst() & 3 {
                    0 => unsafe {
                        self.map_lcdc(arm9, 0x28, 0x28, self.banks.i.as_ptr());
                    },
                    1 => {
                        self.set_range_b_bg(arm9, |m| m | 0x04, core::iter::once(1));
                        self.set_range_b_bg(arm9, |m| m | 0x04, core::iter::once(3));
                    }
                    2 => {
                        self.set_b_obj(arm9, |m| m | 0x02);
                    }
                    _ => {
                        self.map.b_obj_ext_pal_ptr = self.banks.i.as_ptr();
                        engine_2d_b.invalidate_obj_ext_pal_cache();
                    }
                }
            }
        }
    }
}
