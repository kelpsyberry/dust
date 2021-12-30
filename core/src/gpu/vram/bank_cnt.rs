use super::{BankControl, Vram};
use crate::cpu::{
    self,
    arm9::{bus::ptrs::mask as ptr_mask, Arm9},
};
use core::{iter::once, mem::size_of};

// TODO: Find out what happens with invalid VRAM bank MST values.

macro_rules! unmap_regions {
    (
        no_wb
        $self: expr,
        $bank: ident,
        $usage: ident,
        $bank_bit: expr,
        $region_shift: expr,
        $regions: expr;
        $($bit: literal => $mappable_bank: ident),*$(,)?
    ) => {{
        for region in $regions {
            let new = $self.map.$usage[region] & !(1 << $bank_bit);
            $self.map.$usage[region] = new;
            let usage_addr_range = region << $region_shift..(region + 1) << $region_shift;
            if new == 0 {
                $self.$usage.as_byte_mut_slice()
                    [usage_addr_range]
                    .fill(0);
            } else {
                for usage_addr in usage_addr_range.step_by(size_of::<usize>()) {
                    let mut value = 0_usize;
                    $(
                        if new & 1 << $bit != 0 {
                            value |= $self
                                .banks
                                .$mappable_bank
                                .read_ne_aligned_unchecked::<usize>(
                                    usage_addr & ($self.banks.$mappable_bank.len() - 1),
                                );
                        }
                    )*
                    $self.$usage.write_ne_aligned_unchecked(usage_addr, value);
                }
            }
        }
    }};
    (
        wb
        $self: expr,
        $bank: ident,
        $usage: ident,
        $bank_bit: expr,
        $region_shift: expr,
        $regions: expr;
        $($bit: literal => $mappable_bank: ident),*$(,)?
    ) => {{
        for region in $regions {
            let new = $self.map.$usage[region] & !(1 << $bank_bit);
            $self.map.$usage[region] = new;
            let usage_addr_range = region << $region_shift..(region + 1) << $region_shift;
            let bank_addr_range = usage_addr_range.start & ($self.banks.$bank.len() - 1)
                ..=(usage_addr_range.end - 1) & ($self.banks.$bank.len() - 1);
            if new == 0 {
                $self.banks.$bank.as_byte_mut_slice()[bank_addr_range]
                    .copy_from_slice(&$self.$usage.as_byte_slice()[usage_addr_range.clone()]);
                $self.$usage.as_byte_mut_slice()[usage_addr_range].fill(0);
            } else {
                for (usage_addr, bank_addr) in usage_addr_range.zip(bank_addr_range) {
                    if $self.writeback.$usage[usage_addr / usize::BITS as usize]
                        & 1 << (usage_addr & (usize::BITS - 1) as usize)
                        == 0
                    {
                        let mut value = 0;
                        $(
                            if new & 1 << $bit != 0 {
                                value |= $self.banks.$mappable_bank.read_unchecked(
                                    usage_addr & ($self.banks.$mappable_bank.len() - 1),
                                );
                            }
                        )*
                        $self.$usage.write(usage_addr, value);
                    } else {
                        $self
                            .banks
                            .$bank
                            .write(bank_addr, $self.$usage.read(usage_addr));
                    }
                }
            }
        }
    }};
    (a_bg $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        unmap_regions!(
            wb
            $self, $bank, a_bg, $bank_bit, 14, $regions;
            0 => a,
            1 => b,
            2 => c,
            3 => d,
            4 => e,
            5 => f,
            6 => g,
        )
    };
    (a_obj $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        unmap_regions!(
            wb
            $self, $bank, a_obj, $bank_bit, 14, $regions;
            0 => a,
            1 => b,
            2 => e,
            3 => f,
            4 => g,
        )
    };
    (a_bg_ext_pal $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        unmap_regions!(
            no_wb
            $self, $bank, a_bg_ext_pal, $bank_bit, 14, $regions;
            0 => e,
            1 => f,
            2 => g,
        )
    };
    (a_obj_ext_pal $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        unmap_regions!(
            no_wb
            $self, $bank, a_obj_ext_pal, $bank_bit, 13, $regions;
            0 => f,
            1 => g,
        )
    };
    (b_bg $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {{
        unmap_regions!(
            wb
            $self, $bank, b_bg, $bank_bit, 15, $regions;
            0 => c,
            1 => h,
            2 => i,
        )
    }};
    (b_obj $self: expr, d, $bank_bit: expr) => {{
        unmap_regions!(
            wb
            $self, d, b_obj, $bank_bit, 17, once(0);
            0 => d,
            1 => i,
        )
    }};
    (texture $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        unmap_regions!(
            no_wb
            $self, $bank, texture, $bank_bit, 17, $regions;
            0 => a,
            1 => b,
            2 => c,
            3 => d,
        )
    };
    (tex_pal $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        unmap_regions!(
            no_wb
            $self, $bank, tex_pal, $bank_bit, 14, $regions;
            0 => e,
            1 => f,
            2 => g,
        )
    };
    (arm7 $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        unmap_regions!(
            wb
            $self, $bank, arm7, $bank_bit, 17, $regions;
            0 => c,
            1 => d,
        )
    };
}

macro_rules! map_regions {
    (
        no_wb
        $self: expr,
        $bank: ident,
        $usage: ident,
        $bank_bit: expr,
        $region_shift: expr,
        $regions: expr
    ) => {{
        for region in $regions {
            let prev = $self.map.$usage[region];
            $self.map.$usage[region] = prev | 1 << $bank_bit;
            let usage_addr_range = region << $region_shift..(region + 1) << $region_shift;
            let bank_addr_range = usage_addr_range.start & ($self.banks.$bank.len() - 1)
                ..=(usage_addr_range.end - 1) & ($self.banks.$bank.len() - 1);
            let addr_iter = bank_addr_range.zip(usage_addr_range).step_by(size_of::<usize>());
            if prev == 0 {
                for (bank_addr, usage_addr) in addr_iter {
                    $self.$usage.write_ne_aligned_unchecked(
                        usage_addr,
                        $self.banks.$bank.read_ne_aligned_unchecked::<usize>(bank_addr),
                    );
                }
            } else {
                for (bank_addr, usage_addr) in addr_iter {
                    $self.$usage.write_ne_aligned_unchecked(
                        usage_addr,
                        $self.$usage.read_ne_aligned_unchecked::<usize>(usage_addr)
                            | $self
                                .banks
                                .$bank
                                .read_ne_aligned_unchecked::<usize>(bank_addr),
                    );
                }
            }
        }
    }};
    (
        wb
        $self: expr,
        $bank: ident,
        $usage: ident,
        $bank_bit: expr,
        $region_shift: expr,
        $regions: expr;
        $($bit: literal => $mappable_bank: ident),*$(,)?
    ) => {{
        for region in $regions {
            let prev = $self.map.$usage[region];
            $self.map.$usage[region] = prev | 1 << $bank_bit;
            let usage_addr_range = region << $region_shift..(region + 1) << $region_shift;
            let bank_addr_range = usage_addr_range.start & ($self.banks.$bank.len() - 1)
                ..=(usage_addr_range.end - 1) & ($self.banks.$bank.len() - 1);
            let addr_iter = bank_addr_range.zip(usage_addr_range.clone());
            if prev == 0 {
                for (bank_addr, usage_addr) in addr_iter.step_by(size_of::<usize>()) {
                    $self.$usage.write_ne_aligned_unchecked(
                        usage_addr,
                        $self.banks.$bank.read_ne_aligned_unchecked::<usize>(bank_addr),
                    );
                }
            } else {
                for (bank_addr, usage_addr) in addr_iter {
                    let prev_value = $self.$usage.read(usage_addr);
                    if $self.writeback.$usage[usage_addr / usize::BITS as usize]
                        & 1 << (usage_addr & (usize::BITS - 1) as usize)
                        != 0
                    {
                        $(
                            if prev & 1 << $bit != 0 {
                                $self.banks.$mappable_bank.write_unchecked(
                                    usage_addr & ($self.banks.$mappable_bank.len() - 1),
                                    prev_value,
                                );
                            }
                        )*
                    }
                    $self.$usage.write(usage_addr, prev_value | $self.banks.$bank.read(bank_addr));
                }
            }
            $self.writeback.$usage[usage_addr_range.start / usize::BITS as usize
                ..usage_addr_range.end / usize::BITS as usize]
                .fill(0);
        }
    }};
    (a_bg $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        map_regions!(
            wb
            $self, $bank, a_bg, $bank_bit, 14, $regions;
            0 => a,
            1 => b,
            2 => c,
            3 => d,
            4 => e,
            5 => f,
            6 => g,
        )
    };
    (a_obj $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        map_regions!(
            wb
            $self, $bank, a_obj, $bank_bit, 14, $regions;
            0 => a,
            1 => b,
            2 => e,
            3 => f,
            4 => g,
        )
    };
    (a_bg_ext_pal $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        map_regions!(no_wb $self, $bank, a_bg_ext_pal, $bank_bit, 14, $regions)
    };
    (a_obj_ext_pal $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        map_regions!(no_wb $self, $bank, a_obj_ext_pal, $bank_bit, 13, $regions)
    };
    (b_bg $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {{
        map_regions!(
            wb
            $self, $bank, b_bg, $bank_bit, 15, $regions;
            0 => c,
            1 => h,
            2 => i,
        )
    }};
    (b_obj $self: expr, d, $bank_bit: expr) => {{
        map_regions!(
            wb
            $self, d, b_obj, $bank_bit, 17, once(0);
            0 => d,
            1 => i,
        )
    }};
    (texture $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        map_regions!(no_wb $self, $bank, texture, $bank_bit, 17, $regions)
    };
    (tex_pal $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        map_regions!(no_wb $self, $bank, tex_pal, $bank_bit, 14, $regions)
    };
    (arm7 $self: expr, $bank: ident, $bank_bit: expr, $regions: expr) => {
        map_regions!(
            wb
            $self, $bank, arm7, $bank_bit, 17, $regions;
            0 => c,
            1 => d,
        )
    }
}

impl Vram {
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
                self.lcdc_r_ptrs[region] = ptr;
                self.lcdc_w_ptrs[region] = ptr;
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
            self.lcdc_r_ptrs[region] = self.zero_buffer.as_ptr();
            self.lcdc_w_ptrs[region] = self.ignore_buffer.as_ptr();
        }
        for mirror_base in (0..0x80_0000).step_by(0x10_0000) {
            arm9.unmap_sys_bus_ptr_range((mirror_base | lower_bound, mirror_base | upper_bound));
            unsafe {
                arm9.map_sys_bus_ptr_range(
                    ptr_mask::R,
                    self.zero_buffer.as_ptr(),
                    0x8000,
                    (mirror_base | lower_bound, mirror_base | upper_bound),
                );
            }
        }
    }

    pub fn set_bank_control_a<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm9: &mut Arm9<E>,
    ) {
        value.0 &= 0x9B;
        let prev_value = self.bank_control[0];
        if value == prev_value {
            return;
        }
        self.bank_control[0] = value;
        unsafe {
            if prev_value.enabled() {
                match prev_value.mst() & 3 {
                    0 => self.unmap_lcdc(arm9, 0, 7),
                    1 => {
                        let base_region = (prev_value.offset() as usize) << 3;
                        unmap_regions!(a_bg self, a, 0, base_region..base_region + 8);
                    }
                    2 => {
                        let base_region = (prev_value.offset() as usize & 1) << 3;
                        unmap_regions!(a_obj self, a, 0, base_region..base_region + 8);
                    }
                    _ => {
                        let region = prev_value.offset() as usize;
                        unmap_regions!(texture self, a, 0, once(region));
                    }
                }
            }
            if value.enabled() {
                match value.mst() & 3 {
                    0 => self.map_lcdc(arm9, 0, 7, self.banks.a.as_ptr()),
                    1 => {
                        let base_region = (value.offset() as usize) << 3;
                        map_regions!(a_bg self, a, 0, base_region..base_region + 8);
                    }
                    2 => {
                        let base_region = (value.offset() as usize & 1) << 3;
                        map_regions!(a_obj self, a, 0, base_region..base_region + 8);
                    }
                    _ => {
                        let region = value.offset() as usize;
                        map_regions!(texture self, a, 0, once(region));
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
        let prev_value = self.bank_control[1];
        if value == prev_value {
            return;
        }
        self.bank_control[1] = value;
        unsafe {
            if prev_value.enabled() {
                match prev_value.mst() & 3 {
                    0 => self.unmap_lcdc(arm9, 8, 0xF),
                    1 => {
                        let base_region = (prev_value.offset() as usize) << 3;
                        unmap_regions!(a_bg self, b, 1, base_region..base_region + 8);
                    }
                    2 => {
                        let base_region = (prev_value.offset() as usize & 1) << 3;
                        unmap_regions!(a_obj self, b, 1, base_region..base_region + 8);
                    }
                    _ => {
                        let region = prev_value.offset() as usize;
                        unmap_regions!(texture self, b, 1, once(region));
                    }
                }
            }
            if value.enabled() {
                match value.mst() & 3 {
                    0 => self.map_lcdc(arm9, 8, 0xF, self.banks.b.as_ptr()),
                    1 => {
                        let base_region = (value.offset() as usize) << 3;
                        map_regions!(a_bg self, b, 1, base_region..base_region + 8);
                    }
                    2 => {
                        let base_region = (value.offset() as usize & 1) << 3;
                        map_regions!(a_obj self, b, 1, base_region..base_region + 8);
                    }
                    _ => {
                        let region = value.offset() as usize;
                        map_regions!(texture self, b, 1, once(region));
                    }
                }
            }
        }
    }

    pub fn set_bank_control_c<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm9: &mut Arm9<E>,
    ) {
        value.0 &= 0x9F;
        let prev_value = self.bank_control[2];
        if value == prev_value {
            return;
        }
        self.bank_control[2] = value;
        unsafe {
            if prev_value.enabled() {
                match prev_value.mst() {
                    0 => {
                        self.unmap_lcdc(arm9, 0x10, 0x17);
                    }
                    1 => {
                        let base_region = (prev_value.offset() as usize) << 3;
                        unmap_regions!(a_bg self, c, 2, base_region..base_region + 8);
                    }
                    2 => {
                        let region = prev_value.offset() as usize & 1;
                        unmap_regions!(arm7 self, c, 0, once(region));
                        self.arm7_status.set_c_used_as_arm7(false);
                    }
                    3 => {
                        let region = prev_value.offset() as usize;
                        unmap_regions!(texture self, c, 2, once(region));
                    }
                    4 => unmap_regions!(b_bg self, c, 0, 0..4),
                    _ => {
                        unimplemented!("Specified invalid mapping for bank C: {}", prev_value.mst())
                    }
                }
            }
            if value.enabled() {
                match value.mst() {
                    0 => {
                        self.map_lcdc(arm9, 0x10, 0x17, self.banks.c.as_ptr());
                    }
                    1 => {
                        let base_region = (value.offset() as usize) << 3;
                        map_regions!(a_bg self, c, 2, base_region..base_region + 8);
                    }
                    2 => {
                        let region = value.offset() as usize & 1;
                        map_regions!(arm7 self, c, 0, once(region));
                        self.arm7_status.set_c_used_as_arm7(true);
                    }
                    3 => {
                        let region = value.offset() as usize;
                        map_regions!(texture self, c, 2, once(region));
                    }
                    4 => map_regions!(b_bg self, c, 0, 0..4),
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
        arm9: &mut Arm9<E>,
    ) {
        value.0 &= 0x9F;
        unsafe {
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
                        unmap_regions!(a_bg self, d, 3, base_region..base_region + 8);
                    }
                    2 => {
                        let region = prev_value.offset() as usize & 1;
                        unmap_regions!(arm7 self, d, 1, once(region));
                        self.arm7_status.set_d_used_as_arm7(false);
                    }
                    3 => {
                        let region = prev_value.offset() as usize;
                        unmap_regions!(texture self, d, 3, once(region));
                    }
                    4 => unmap_regions!(b_obj self, d, 0),
                    _ => {
                        unimplemented!("Specified invalid mapping for bank D: {}", prev_value.mst())
                    }
                }
            }
            if value.enabled() {
                match value.mst() {
                    0 => self.map_lcdc(arm9, 0x18, 0x1F, self.banks.d.as_ptr()),
                    1 => {
                        let base_region = (value.offset() as usize) << 3;
                        map_regions!(a_bg self, d, 3, base_region..base_region + 8);
                    }
                    2 => {
                        let region = value.offset() as usize & 1;
                        map_regions!(arm7 self, d, 1, once(region));
                        self.arm7_status.set_d_used_as_arm7(true);
                    }
                    3 => {
                        let region = value.offset() as usize;
                        map_regions!(texture self, d, 3, once(region));
                    }
                    4 => map_regions!(b_obj self, d, 0),
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
    ) {
        value.0 &= 0x87;
        unsafe {
            let prev_value = self.bank_control[4];
            if value == prev_value {
                return;
            }
            self.bank_control[4] = value;
            if prev_value.enabled() {
                match prev_value.mst() {
                    0 => self.unmap_lcdc(arm9, 0x20, 0x23),
                    1 => unmap_regions!(a_bg self, e, 4, 0..4),
                    2 => unmap_regions!(a_obj self, e, 2, 0..4),
                    3 => unmap_regions!(tex_pal self, e, 0, 0..4),
                    4 => unmap_regions!(a_bg_ext_pal self, e, 0, 0..2),
                    _ => {
                        unimplemented!("Specified invalid mapping for bank E: {}", prev_value.mst())
                    }
                }
            }
            if value.enabled() {
                match value.mst() {
                    0 => self.map_lcdc(arm9, 0x20, 0x23, self.banks.e.as_ptr()),
                    1 => map_regions!(a_bg self, e, 4, 0..4),
                    2 => map_regions!(a_obj self, e, 2, 0..4),
                    3 => map_regions!(tex_pal self, e, 0, 0..4),
                    4 => map_regions!(a_bg_ext_pal self, e, 0, 0..2),
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
    ) {
        value.0 &= 0x9F;
        unsafe {
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
                        unmap_regions!(a_bg self, f, 5, [base_region, base_region | 2]);
                    }
                    2 => {
                        let base_region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        unmap_regions!(a_obj self, f, 3, [base_region, base_region | 2]);
                    }
                    3 => {
                        let region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        unmap_regions!(tex_pal self, f, 1, once(region));
                    }
                    4 => {
                        let region = prev_value.offset() as usize & 1;
                        unmap_regions!(a_bg_ext_pal self, f, 1, once(region));
                    }
                    5 => unmap_regions!(a_obj_ext_pal self, f, 0, 0..1),
                    _ => {
                        unimplemented!("Specified invalid mapping for bank F: {}", prev_value.mst())
                    }
                }
            }
            if value.enabled() {
                match value.mst() {
                    0 => self.map_lcdc(arm9, 0x24, 0x24, self.banks.f.as_ptr()),
                    1 => {
                        let base_region =
                            ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        map_regions!(a_bg self, f, 5, [base_region, base_region | 2]);
                    }
                    2 => {
                        let base_region =
                            ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        map_regions!(a_obj self, f, 3, [base_region, base_region | 2]);
                    }
                    3 => {
                        let region = ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        map_regions!(tex_pal self, f, 1, once(region));
                    }
                    4 => {
                        let region = value.offset() as usize & 1;
                        map_regions!(a_bg_ext_pal self, f, 1, once(region));
                    }
                    5 => map_regions!(a_obj_ext_pal self, f, 0, 0..1),
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
    ) {
        value.0 &= 0x9F;
        unsafe {
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
                        unmap_regions!(a_bg self, g, 6, [base_region, base_region | 2]);
                    }
                    2 => {
                        let base_region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        unmap_regions!(a_obj self, g, 4, [base_region, base_region | 2]);
                    }
                    3 => {
                        let region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        unmap_regions!(tex_pal self, g, 2, once(region));
                    }
                    4 => {
                        let region = prev_value.offset() as usize & 1;
                        unmap_regions!(a_bg_ext_pal self, g, 2, once(region));
                    }
                    5 => unmap_regions!(a_obj_ext_pal self, g, 1, 0..1),
                    _ => {
                        unimplemented!("Specified invalid mapping for bank G: {}", prev_value.mst())
                    }
                }
            }
            if value.enabled() {
                match value.mst() {
                    0 => self.map_lcdc(arm9, 0x25, 0x25, self.banks.g.as_ptr()),
                    1 => {
                        let base_region =
                            ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        map_regions!(a_bg self, g, 6, [base_region, base_region | 2]);
                    }
                    2 => {
                        let base_region =
                            ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        map_regions!(a_obj self, g, 4, [base_region, base_region | 2]);
                    }
                    3 => {
                        let region = ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        map_regions!(tex_pal self, g, 2, once(region));
                    }
                    4 => {
                        let region = value.offset() as usize & 1;
                        map_regions!(a_bg_ext_pal self, g, 2, once(region));
                    }
                    5 => map_regions!(a_obj_ext_pal self, g, 1, 0..1),
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
    ) {
        value.0 &= 0x83;
        unsafe {
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
                    1 => unmap_regions!(b_bg self, h, 1, [0, 2]),
                    2 => self.b_bg_ext_pal_ptr = self.zero_buffer.as_ptr(),
                    _ => {
                        unimplemented!("Specified invalid mapping for bank H: {}", prev_value.mst())
                    }
                }
            }
            if value.enabled() {
                match value.mst() & 3 {
                    0 => self.map_lcdc(arm9, 0x26, 0x27, self.banks.h.as_ptr()),
                    1 => map_regions!(b_bg self, h, 1, [0, 2]),
                    2 => self.b_bg_ext_pal_ptr = self.banks.h.as_ptr(),
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
    ) {
        // Bank I requires special code for mapping/unmapping, as it gets mirrored inside what is
        // considered a single region by other code.
        value.0 &= 0x83;
        unsafe {
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
                        if self.map.b_bg[1] == 1 << 2 {
                            self.map.b_bg[1] = 0;
                            self.map.b_bg[3] = 0;
                            let mut b_bg = self.b_bg.as_byte_mut_slice();
                            self.banks
                                .i
                                .as_byte_mut_slice()
                                .copy_from_slice(&b_bg[0x8000..0xC000]);
                            b_bg[0x8000..0x1_0000].fill(0);
                            b_bg[0x1_8000..0x2_0000].fill(0);
                        } else {
                            for region in [1, 3] {
                                self.map.b_bg[region] = 1;
                                for usage_addr in region << 15..(region + 1) << 15 {
                                    if self.writeback.b_bg[usage_addr / usize::BITS as usize]
                                        & 1 << (usage_addr & (usize::BITS - 1) as usize)
                                        == 0
                                    {
                                        self.b_bg.write(
                                            usage_addr,
                                            self.banks.c.read_unchecked(usage_addr),
                                        );
                                    } else {
                                        self.banks
                                            .i
                                            .write(usage_addr & 0x3FFF, self.b_bg.read(usage_addr));
                                    }
                                }
                            }
                        }
                    }
                    2 => {
                        let new = self.map.b_obj[0] & !(1 << 1);
                        self.map.b_obj[0] = new;
                        if new == 0 {
                            self.banks
                                .i
                                .as_byte_mut_slice()
                                .copy_from_slice(&self.b_obj.as_byte_slice()[..0x4000]);
                            self.b_obj.as_byte_mut_slice().fill(0);
                        } else {
                            for (usage_addr, byte) in
                                self.b_obj.as_byte_mut_slice().iter_mut().enumerate()
                            {
                                if self.writeback.b_obj[usage_addr / usize::BITS as usize]
                                    & 1 << (usage_addr & (usize::BITS - 1) as usize)
                                    == 0
                                {
                                    *byte = self.banks.d.read_unchecked(usage_addr);
                                } else {
                                    self.banks.i.write(usage_addr & 0x3FFF, *byte);
                                }
                            }
                        }
                    }
                    _ => self.b_obj_ext_pal_ptr = self.zero_buffer.as_ptr(),
                }
            }
            if value.enabled() {
                match value.mst() & 3 {
                    0 => self.map_lcdc(arm9, 0x28, 0x28, self.banks.i.as_ptr()),
                    1 => {
                        if self.map.b_bg[1] == 0 {
                            self.map.b_bg[1] = 1 << 2;
                            self.map.b_bg[3] = 1 << 2;
                            for base_usage_addr in [0x8000, 0xC000, 0x1_8000, 0x1_C000] {
                                self.b_bg.as_byte_mut_slice()
                                    [base_usage_addr..base_usage_addr + 0x4000]
                                    .copy_from_slice(&self.banks.i.as_byte_slice());
                            }
                        } else {
                            self.map.b_bg[1] = 5;
                            self.map.b_bg[3] = 5;
                            for usage_addr in (0x8000..0x1_0000).chain(0x1_8000..0x2_0000) {
                                let prev_value = self.b_bg.read(usage_addr);
                                if self.writeback.b_bg[usage_addr / usize::BITS as usize]
                                    & 1 << (usage_addr & (usize::BITS - 1) as usize)
                                    != 0
                                {
                                    self.banks.c.write_unchecked(usage_addr, prev_value);
                                }
                                self.b_bg.write(
                                    usage_addr,
                                    prev_value | self.banks.i.read(usage_addr & 0x3FFF),
                                );
                            }
                        }
                        self.writeback.b_bg
                            [0x8000 / usize::BITS as usize..0x1_0000 / usize::BITS as usize]
                            .fill(0);
                        self.writeback.b_bg
                            [0x1_8000 / usize::BITS as usize..0x2_0000 / usize::BITS as usize]
                            .fill(0);
                    }
                    2 => {
                        let prev = self.map.b_obj[0];
                        self.map.b_obj[0] = prev | 1 << 1;
                        if prev == 0 {
                            for base_usage_addr in (0..0x2_0000).step_by(0x4000) {
                                self.b_obj.as_byte_mut_slice()
                                    [base_usage_addr..base_usage_addr + 0x4000]
                                    .copy_from_slice(&self.banks.i.as_byte_slice());
                            }
                        } else {
                            for (usage_addr, byte) in
                                self.b_obj.as_byte_mut_slice().iter_mut().enumerate()
                            {
                                if self.writeback.b_obj[usage_addr / usize::BITS as usize]
                                    & 1 << (usage_addr & (usize::BITS - 1) as usize)
                                    != 0
                                {
                                    self.banks.d.write_unchecked(usage_addr, *byte);
                                }
                                *byte |= self.banks.i.read(usage_addr & 0x3FFF);
                            }
                        }
                        self.writeback.b_obj.fill(0);
                    }
                    _ => self.b_obj_ext_pal_ptr = self.banks.i.as_ptr(),
                }
            }
        }
    }
}
