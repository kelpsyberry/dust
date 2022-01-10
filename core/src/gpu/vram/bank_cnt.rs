use super::{BankControl, Vram};
use crate::{
    cpu::{
        self,
        arm7::{self, Arm7},
        arm9::{bus::ptrs::mask as ptr_mask, Arm9},
    },
    gpu::engine_3d::Engine3d,
    utils::OwnedBytesCellPtr,
};
use core::{iter::once, mem::size_of, ops::Range};

// TODO: Find out what happens with invalid VRAM bank MST values.
// TODO: `generic_arg_infer` isn't exactly stable right now; when it is, remove the bank lengths
// that were manually specified in some unmap_* calls

macro_rules! map_cpu_visible {
    (
        $cpu: expr, $mask: expr, $cpu_start_addr: expr, $cpu_end_addr: expr,
        $usage: expr, $region: expr, $region_shift: expr
    ) => {
        for mirror_base_addr in ($cpu_start_addr..$cpu_end_addr).step_by($usage.len()) {
            let region_base_addr = mirror_base_addr | ($region as u32) << $region_shift;
            $cpu.map_sys_bus_ptr_range(
                $mask,
                $usage.as_ptr().add($region << $region_shift),
                1 << $region_shift,
                (
                    region_base_addr,
                    region_base_addr | ((1 << $region_shift) - 1),
                ),
            );
        }
    };
}

unsafe fn copy_slice_wrapping_unchecked_with_dst_range<
    const DST_LEN: usize,
    const SRC_LEN: usize,
>(
    dst: &OwnedBytesCellPtr<DST_LEN>,
    src: &OwnedBytesCellPtr<SRC_LEN>,
    dst_range: Range<usize>,
) {
    let mut dst = dst.as_byte_mut_slice();
    let src = src.as_byte_slice();
    let src_base_addr = dst_range.start & (SRC_LEN - 1);
    let copy_len = ((dst_range.end - 1) & (SRC_LEN - 1)) - src_base_addr + 1;
    for dst_base_addr in dst_range.step_by(copy_len) {
        dst.get_unchecked_mut(dst_base_addr..dst_base_addr + copy_len)
            .copy_from_slice(src.get_unchecked(src_base_addr..src_base_addr + copy_len));
    }
}

unsafe fn or_assign_slice_wrapping_unchecked<const DST_LEN: usize, const SRC_LEN: usize>(
    dst: &OwnedBytesCellPtr<DST_LEN>,
    src: &OwnedBytesCellPtr<SRC_LEN>,
    dst_range: Range<usize>,
) {
    for dst_addr in dst_range.step_by(size_of::<usize>()) {
        dst.write_ne_aligned_unchecked(
            dst_addr,
            dst.read_ne_aligned_unchecked::<usize>(dst_addr)
                | src.read_ne_aligned_unchecked::<usize>(dst_addr & (SRC_LEN - 1)),
        );
    }
}

macro_rules! map_region {
    (
        no_wb $self: expr,
        $usage: ident, $region_shift: expr,
        $bank: expr, $bank_bit: expr, $region: expr
    ) => {{
        let prev = $self.map.$usage[$region].get();
        $self.map.$usage[$region].set(prev | 1 << $bank_bit);
        let usage_addr_range = $region << $region_shift..($region + 1) << $region_shift;
        if prev == 0 {
            copy_slice_wrapping_unchecked_with_dst_range(
                &$self.$usage,
                $bank,
                usage_addr_range,
            );
        } else {
            or_assign_slice_wrapping_unchecked(&$self.$usage, $bank, usage_addr_range);
        }
    }};
    (
        wb $self: expr,
        $usage: ident,
        $region_shift: expr,
        $mirrored_banks_mask: expr,
        ($($bit: literal => $mappable_bank: ident),*$(,)?),

        $cpu: expr,
        $cpu_r_mask: expr,
        $cpu_rw_mask: expr,
        $cpu_start_addr: expr,
        $cpu_end_addr: expr,
        $bank: ident,
        $bank_bit: expr,
        $region: expr
    ) => {{
        let prev = $self.map.$usage[$region].get();
        $self.map.$usage[$region].set(prev | 1 << $bank_bit);
        let usage_addr_range = $region << $region_shift..($region + 1) << $region_shift;
        let writeback_arr = &mut *$self.writeback.$usage.get();
        #[allow(clippy::bad_bit_mask)]
        if prev == 0 {
            copy_slice_wrapping_unchecked_with_dst_range(
                &$self.$usage,
                $bank,
                usage_addr_range.clone(),
            );
            if 1 << $bank_bit & $mirrored_banks_mask == 0 {
                map_cpu_visible!(
                    $cpu, $cpu_rw_mask, $cpu_start_addr, $cpu_end_addr,
                    $self.$usage, $region, $region_shift
                );
            }
        } else {
            if prev & (prev - 1) == 0 && prev & $mirrored_banks_mask == 0 {
                map_cpu_visible!(
                    $cpu, $cpu_r_mask, $cpu_start_addr, $cpu_end_addr,
                    $self.$usage, $region, $region_shift
                );
                'writeback_to_prev_bank: {
                    $(
                        if $bit != $bank_bit && prev & 1 << $bit != 0 {
                            let bank_len_mask = $self.banks.$mappable_bank.len() - 1;
                            let bank_base_addr = usage_addr_range.start & bank_len_mask;
                            let copy_len =
                                ((usage_addr_range.end - 1) & bank_len_mask) - bank_base_addr + 1;
                            $self.banks.$mappable_bank.as_byte_mut_slice()
                                .get_unchecked_mut(bank_base_addr..bank_base_addr + copy_len)
                                .copy_from_slice($self.$usage.as_byte_slice().get_unchecked(
                                    usage_addr_range.start..usage_addr_range.start + copy_len
                                ));
                            break 'writeback_to_prev_bank;
                        }
                    )*
                }
            } else {
                for usage_addr in usage_addr_range.clone() {
                    let prev_value = $self.$usage.read_unchecked(usage_addr);
                    if *writeback_arr.get_unchecked(usage_addr / usize::BITS as usize)
                        & 1 << (usage_addr & (usize::BITS - 1) as usize)
                        != 0
                    {
                        $(
                            if $bit != $bank_bit && prev & 1 << $bit != 0 {
                                $self.banks.$mappable_bank.write_unchecked(
                                    usage_addr & ($self.banks.$mappable_bank.len() - 1),
                                    prev_value,
                                );
                            }
                        )*
                    }
                }
            }
            or_assign_slice_wrapping_unchecked(&$self.$usage, $bank, usage_addr_range.clone());
        }
        #[allow(clippy::bad_bit_mask)]
        if prev != 0 || 1 << $bank_bit & $mirrored_banks_mask != 0 {
            writeback_arr.get_unchecked_mut(
                usage_addr_range.start / usize::BITS as usize
                    ..usage_addr_range.end / usize::BITS as usize
            ).fill(0);
        }
    }};
}

macro_rules! unmap_region {
    (
        no_wb $self: expr,
        $usage: ident, $region_shift: expr, ($($bit: literal => $mappable_bank: ident),*$(,)?),
        $bank_bit: expr, $region: expr
    ) => {{
        let new = $self.map.$usage[$region].get() & !(1 << $bank_bit);
        $self.map.$usage[$region].set(new);
        let usage_addr_range = $region << $region_shift..($region + 1) << $region_shift;
        if new == 0 {
            $self.$usage.as_byte_mut_slice().get_unchecked_mut(usage_addr_range).fill(0);
        } else {
            for usage_addr in usage_addr_range.step_by(size_of::<usize>()) {
                let mut value = 0_usize;
                $(
                    if $bit != $bank_bit && new & 1 << $bit != 0 {
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
    }};
    (
        wb $self: expr,

        $usage: ident,
        $region_shift: expr,
        $mirrored_banks_mask: expr,
        ($($bit: literal => $mappable_bank: ident),*$(,)?),

        $cpu: expr,
        $cpu_r_mask: expr,
        $cpu_rw_mask: expr,
        $cpu_start_addr: expr,
        $cpu_end_addr: expr,

        $bank: expr,
        $bank_bit: expr,
        $is_mirror: expr,
        $region: expr
    ) => {{
        let prev = $self.map.$usage[$region].get();
        let new = prev & !(1 << $bank_bit);
        $self.map.$usage[$region].set(new);
        let usage_addr_range = $region << $region_shift..($region + 1) << $region_shift;
        #[allow(clippy::bad_bit_mask)]
        if new == 0 {
            if !$is_mirror {
                let bank_len_mask = $bank.len() - 1;
                $bank.as_byte_mut_slice().get_unchecked_mut(
                    usage_addr_range.start & bank_len_mask
                        ..=(usage_addr_range.end - 1) & bank_len_mask
                ).copy_from_slice(
                    &$self.$usage.as_byte_slice().get_unchecked(usage_addr_range.clone())
                );
            }
            $self.$usage.as_byte_mut_slice().get_unchecked_mut(usage_addr_range).fill(0);
            if prev & $mirrored_banks_mask == 0 {
                map_cpu_visible!(
                    $cpu, $cpu_r_mask, $cpu_start_addr, $cpu_end_addr,
                    $self.$usage, $region, $region_shift
                );
            }
        } else {
            if new & (new - 1) == 0 && new & $mirrored_banks_mask == 0 {
                map_cpu_visible!(
                    $cpu, $cpu_rw_mask, $cpu_start_addr, $cpu_end_addr,
                    $self.$usage, $region, $region_shift
                );
            }
            let writeback_arr = &*$self.writeback.$usage.get();
            for usage_addr in usage_addr_range {
                if *writeback_arr.get_unchecked(usage_addr / usize::BITS as usize)
                    & 1 << (usage_addr & (usize::BITS - 1) as usize)
                    == 0
                {
                    let mut value = 0;
                    $(
                        if $bit != $bank_bit && new & 1 << $bit != 0 {
                            value |= $self.banks.$mappable_bank.read_unchecked(
                                usage_addr & ($self.banks.$mappable_bank.len() - 1),
                            );
                        }
                    )*
                    $self.$usage.write_unchecked(usage_addr, value);
                } else if !$is_mirror {
                    $bank.write_unchecked(
                        usage_addr & ($bank.len() - 1),
                        $self.$usage.read_unchecked(usage_addr),
                    );
                }
            }
        }
    }};
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

    unsafe fn map_a_bg<
        E: cpu::Engine,
        R: IntoIterator<Item = usize>,
        const LEN: usize,
        const BANK_BIT: u8,
    >(
        &self,
        arm9: &mut Arm9<E>,
        bank: &OwnedBytesCellPtr<LEN>,
        regions: R,
    ) {
        for region in regions {
            map_region!(
                wb self,
                a_bg, 14, 0x60,
                (
                    0 => a,
                    1 => b,
                    2 => c,
                    3 => d,
                    4 => e,
                    5 => f,
                    6 => g,
                ),
                arm9, ptr_mask::R, ptr_mask::R | ptr_mask::W_16_32, 0x0600_0000, 0x0620_0000,
                bank, BANK_BIT, region
            );
        }
    }

    unsafe fn unmap_a_bg<
        E: cpu::Engine,
        R: IntoIterator<Item = usize>,
        const LEN: usize,
        const IS_MIRROR: bool,
        const BANK_BIT: u8,
    >(
        &self,
        arm9: &mut Arm9<E>,
        bank: &OwnedBytesCellPtr<LEN>,
        regions: R,
    ) {
        for region in regions {
            unmap_region!(
                wb self,
                a_bg, 14, 0x60,
                (
                    0 => a,
                    1 => b,
                    2 => c,
                    3 => d,
                    4 => e,
                    5 => f,
                    6 => g,
                ),
                arm9, ptr_mask::R, ptr_mask::R | ptr_mask::W_16_32, 0x0600_0000, 0x0620_0000,
                bank, BANK_BIT, IS_MIRROR, region
            );
        }
    }

    unsafe fn map_a_obj<
        E: cpu::Engine,
        R: IntoIterator<Item = usize>,
        const LEN: usize,
        const BANK_BIT: u8,
    >(
        &self,
        arm9: &mut Arm9<E>,
        bank: &OwnedBytesCellPtr<LEN>,
        regions: R,
    ) {
        for region in regions {
            map_region!(
                wb self,
                a_obj, 14, 0x18,
                (
                    0 => a,
                    1 => b,
                    2 => e,
                    3 => f,
                    4 => g,
                ),
                arm9, ptr_mask::R, ptr_mask::R | ptr_mask::W_16_32, 0x0640_0000, 0x0660_0000,
                bank, BANK_BIT, region
            );
        }
    }

    unsafe fn unmap_a_obj<
        E: cpu::Engine,
        R: IntoIterator<Item = usize>,
        const LEN: usize,
        const IS_MIRROR: bool,
        const BANK_BIT: u8,
    >(
        &self,
        arm9: &mut Arm9<E>,
        bank: &OwnedBytesCellPtr<LEN>,
        regions: R,
    ) {
        for region in regions {
            unmap_region!(
                wb self,
                a_obj, 14, 0x18,
                (
                    0 => a,
                    1 => b,
                    2 => e,
                    3 => f,
                    4 => g,
                ),
                arm9, ptr_mask::R, ptr_mask::R | ptr_mask::W_16_32, 0x0640_0000, 0x0660_0000,
                bank, BANK_BIT, IS_MIRROR, region
            );
        }
    }

    unsafe fn map_a_bg_ext_pal<
        R: IntoIterator<Item = usize>,
        const LEN: usize,
        const BANK_BIT: u8,
    >(
        &self,
        bank: &OwnedBytesCellPtr<LEN>,
        regions: R,
    ) {
        for region in regions {
            map_region!(no_wb self, a_bg_ext_pal, 14, bank, BANK_BIT, region);
        }
    }

    unsafe fn unmap_a_bg_ext_pal<R: IntoIterator<Item = usize>, const BANK_BIT: u8>(
        &self,
        regions: R,
    ) {
        for region in regions {
            unmap_region!(
                no_wb self,
                a_bg_ext_pal, 14,
                (
                    0 => e,
                    1 => f,
                    2 => g,
                ),
                BANK_BIT, region
            );
        }
    }

    unsafe fn map_a_obj_ext_pal<const LEN: usize, const BANK_BIT: u8>(
        &self,
        bank: &OwnedBytesCellPtr<LEN>,
    ) {
        map_region!(no_wb self, a_obj_ext_pal, 13, bank, BANK_BIT, 0);
    }

    unsafe fn unmap_a_obj_ext_pal<const BANK_BIT: u8>(&self) {
        unmap_region!(
            no_wb self,
            a_obj_ext_pal, 13,
            (
                0 => f,
                1 => g,
            ),
            BANK_BIT, 0
        );
    }

    unsafe fn map_b_bg<
        E: cpu::Engine,
        R: IntoIterator<Item = usize>,
        const LEN: usize,
        const BANK_BIT: u8,
    >(
        &self,
        arm9: &mut Arm9<E>,
        bank: &OwnedBytesCellPtr<LEN>,
        regions: R,
    ) {
        for region in regions {
            map_region!(
                wb self,
                b_bg, 15, 6,
                (
                    0 => c,
                    1 => h,
                    2 => i,
                ),
                arm9, ptr_mask::R, ptr_mask::R | ptr_mask::W_16_32, 0x0620_0000, 0x0640_0000,
                bank, BANK_BIT, region
            );
        }
    }

    unsafe fn unmap_b_bg<
        E: cpu::Engine,
        R: IntoIterator<Item = usize>,
        const LEN: usize,
        const IS_MIRROR: bool,
        const BANK_BIT: u8,
    >(
        &self,
        arm9: &mut Arm9<E>,
        bank: &OwnedBytesCellPtr<LEN>,
        regions: R,
    ) {
        for region in regions {
            unmap_region!(
                wb self,
                b_bg, 15, 6,
                (
                    0 => c,
                    1 => h,
                    2 => i,
                ),
                arm9, ptr_mask::R, ptr_mask::R | ptr_mask::W_16_32, 0x0620_0000, 0x0640_0000,
                bank, BANK_BIT, IS_MIRROR, region
            );
        }
    }

    unsafe fn map_b_obj<E: cpu::Engine, const LEN: usize, const BANK_BIT: u8>(
        &self,
        arm9: &mut Arm9<E>,
        bank: &OwnedBytesCellPtr<LEN>,
    ) {
        map_region!(
            wb self,
            b_obj, 17, 2,
            (
                0 => d,
                1 => i,
            ),
            arm9, ptr_mask::R, ptr_mask::R | ptr_mask::W_16_32, 0x0660_0000, 0x0680_0000,
            bank, BANK_BIT, 0
        );
    }

    unsafe fn unmap_b_obj<E: cpu::Engine, const LEN: usize, const BANK_BIT: u8>(
        &self,
        arm9: &mut Arm9<E>,
        bank: &OwnedBytesCellPtr<LEN>,
    ) {
        unmap_region!(
            wb self,
            b_obj, 17, 2,
            (
                0 => d,
                1 => i,
            ),
            arm9, ptr_mask::R, ptr_mask::R | ptr_mask::W_16_32, 0x0660_0000, 0x0680_0000,
            bank, BANK_BIT, false, 0
        );
    }

    unsafe fn map_texture<const LEN: usize, const BANK_BIT: u8>(
        &self,
        bank: &OwnedBytesCellPtr<LEN>,
        region: usize,
    ) {
        map_region!(no_wb self, texture, 17, bank, BANK_BIT, region);
    }

    unsafe fn unmap_texture<const BANK_BIT: u8>(&self, region: usize) {
        unmap_region!(
            no_wb self,
            texture, 17,
            (
                0 => a,
                1 => b,
                2 => c,
                3 => d,
            ),
            BANK_BIT, region
        );
    }

    unsafe fn map_tex_pal<R: IntoIterator<Item = usize>, const LEN: usize, const BANK_BIT: u8>(
        &self,
        bank: &OwnedBytesCellPtr<LEN>,
        regions: R,
    ) {
        for region in regions {
            map_region!(no_wb self, tex_pal, 14, bank, BANK_BIT, region);
        }
    }

    unsafe fn unmap_tex_pal<R: IntoIterator<Item = usize>, const BANK_BIT: u8>(&self, regions: R) {
        for region in regions {
            unmap_region!(
                no_wb self,
                tex_pal, 14,
                (
                    0 => e,
                    1 => f,
                    2 => g
                ),
                BANK_BIT, region
            );
        }
    }

    unsafe fn map_arm7<E: cpu::Engine, const LEN: usize, const BANK_BIT: u8>(
        &self,
        arm7: &mut Arm7<E>,
        bank: &OwnedBytesCellPtr<LEN>,
        region: usize,
    ) {
        use arm7::bus::ptrs::mask as ptr_mask;
        map_region!(
            wb self,
            arm7, 17, 0,
            (
                0 => c,
                1 => d,
            ),
            arm7, ptr_mask::R, ptr_mask::ALL, 0x0600_0000, 0x0700_0000,
            bank, BANK_BIT, region
        );
    }

    unsafe fn unmap_arm7<E: cpu::Engine, const LEN: usize, const BANK_BIT: u8>(
        &self,
        arm7: &mut Arm7<E>,
        bank: &OwnedBytesCellPtr<LEN>,
        region: usize,
    ) {
        use arm7::bus::ptrs::mask as ptr_mask;
        unmap_region!(
            wb self,
            arm7, 17, 0,
            (
                0 => c,
                1 => d,
            ),
            arm7, ptr_mask::R, ptr_mask::ALL, 0x0600_0000, 0x0700_0000,
            bank, BANK_BIT, false, region
        );
    }

    pub fn set_bank_control_a<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm9: &mut Arm9<E>,
        engine_3d: &mut Engine3d,
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
                        self.unmap_a_bg::<_, _, 0x2_0000, false, 0>(
                            arm9,
                            &self.banks.a,
                            base_region..base_region + 8,
                        );
                    }
                    2 => {
                        let base_region = (prev_value.offset() as usize & 1) << 3;
                        self.unmap_a_obj::<_, _, 0x2_0000, false, 0>(
                            arm9,
                            &self.banks.a,
                            base_region..base_region + 8,
                        );
                    }
                    _ => {
                        let region = prev_value.offset() as usize;
                        engine_3d.set_texture_dirty(1 << region);
                        self.unmap_texture::<0>(region);
                    }
                }
            }
            if value.enabled() {
                match value.mst() & 3 {
                    0 => self.map_lcdc(arm9, 0, 7, self.banks.a.as_ptr()),
                    1 => {
                        let base_region = (value.offset() as usize) << 3;
                        self.map_a_bg::<_, _, _, 0>(
                            arm9,
                            &self.banks.a,
                            base_region..base_region + 8,
                        );
                    }
                    2 => {
                        let base_region = (value.offset() as usize & 1) << 3;
                        self.map_a_obj::<_, _, _, 0>(
                            arm9,
                            &self.banks.a,
                            base_region..base_region + 8,
                        );
                    }
                    _ => {
                        let region = value.offset() as usize;
                        engine_3d.set_texture_dirty(1 << region);
                        self.map_texture::<_, 0>(&self.banks.a, region);
                    }
                }
            }
        }
    }

    pub fn set_bank_control_b<E: cpu::Engine>(
        &mut self,
        mut value: BankControl,
        arm9: &mut Arm9<E>,
        engine_3d: &mut Engine3d,
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
                        self.unmap_a_bg::<_, _, 0x2_0000, false, 1>(
                            arm9,
                            &self.banks.b,
                            base_region..base_region + 8,
                        );
                    }
                    2 => {
                        let base_region = (prev_value.offset() as usize & 1) << 3;
                        self.unmap_a_obj::<_, _, 0x2_0000, false, 1>(
                            arm9,
                            &self.banks.b,
                            base_region..base_region + 8,
                        );
                    }
                    _ => {
                        let region = prev_value.offset() as usize;
                        engine_3d.set_texture_dirty(1 << region);
                        self.unmap_texture::<1>(region);
                    }
                }
            }
            if value.enabled() {
                match value.mst() & 3 {
                    0 => self.map_lcdc(arm9, 8, 0xF, self.banks.b.as_ptr()),
                    1 => {
                        let base_region = (value.offset() as usize) << 3;
                        self.map_a_bg::<_, _, _, 1>(
                            arm9,
                            &self.banks.b,
                            base_region..base_region + 8,
                        );
                    }
                    2 => {
                        let base_region = (value.offset() as usize & 1) << 3;
                        self.map_a_obj::<_, _, _, 1>(
                            arm9,
                            &self.banks.b,
                            base_region..base_region + 8,
                        );
                    }
                    _ => {
                        let region = value.offset() as usize;
                        engine_3d.set_texture_dirty(1 << region);
                        self.map_texture::<_, 1>(&self.banks.b, region);
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
        engine_3d: &mut Engine3d,
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
                        self.unmap_a_bg::<_, _, 0x2_0000, false, 2>(
                            arm9,
                            &self.banks.c,
                            base_region..base_region + 8,
                        );
                    }
                    2 => {
                        let region = prev_value.offset() as usize & 1;
                        self.unmap_arm7::<_, _, 0>(arm7, &self.banks.c, region);
                        self.arm7_status.set_c_used_as_arm7(false);
                    }
                    3 => {
                        let region = prev_value.offset() as usize;
                        engine_3d.set_texture_dirty(1 << region);
                        self.unmap_texture::<2>(region);
                    }
                    4 => self.unmap_b_bg::<_, _, 0x2_0000, false, 0>(arm9, &self.banks.c, 0..4),
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
                        self.map_a_bg::<_, _, _, 2>(
                            arm9,
                            &self.banks.c,
                            base_region..base_region + 8,
                        );
                    }
                    2 => {
                        let region = value.offset() as usize & 1;
                        self.map_arm7::<_, _, 0>(arm7, &self.banks.c, region);
                        self.arm7_status.set_c_used_as_arm7(true);
                    }
                    3 => {
                        let region = value.offset() as usize;
                        engine_3d.set_texture_dirty(1 << region);
                        self.map_texture::<_, 2>(&self.banks.c, region);
                    }
                    4 => self.map_b_bg::<_, _, _, 0>(arm9, &self.banks.c, 0..4),
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
        engine_3d: &mut Engine3d,
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
                        self.unmap_a_bg::<_, _, 0x2_0000, false, 3>(
                            arm9,
                            &self.banks.d,
                            base_region..base_region + 8,
                        );
                    }
                    2 => {
                        let region = prev_value.offset() as usize & 1;
                        self.unmap_arm7::<_, _, 1>(arm7, &self.banks.d, region);
                        self.arm7_status.set_d_used_as_arm7(false);
                    }
                    3 => {
                        let region = prev_value.offset() as usize;
                        engine_3d.set_texture_dirty(1 << region);
                        self.unmap_texture::<3>(region);
                    }
                    4 => self.unmap_b_obj::<_, _, 0>(arm9, &self.banks.d),
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
                        self.map_a_bg::<_, _, _, 3>(
                            arm9,
                            &self.banks.d,
                            base_region..base_region + 8,
                        );
                    }
                    2 => {
                        let region = value.offset() as usize & 1;
                        self.map_arm7::<_, _, 1>(arm7, &self.banks.d, region);
                        self.arm7_status.set_d_used_as_arm7(true);
                    }
                    3 => {
                        let region = value.offset() as usize;
                        engine_3d.set_texture_dirty(1 << region);
                        self.map_texture::<_, 3>(&self.banks.d, region);
                    }
                    4 => self.map_b_obj::<_, _, 0>(arm9, &self.banks.d),
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
        engine_3d: &mut Engine3d,
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
                    1 => self.unmap_a_bg::<_, _, 0x1_0000, false, 4>(arm9, &self.banks.e, 0..4),
                    2 => self.unmap_a_obj::<_, _, 0x1_0000, false, 2>(arm9, &self.banks.e, 0..4),
                    3 => {
                        engine_3d.set_tex_pal_dirty(0xF);
                        self.unmap_tex_pal::<_, 0>(0..4);
                    }
                    4 => self.unmap_a_bg_ext_pal::<_, 0>(0..2),
                    _ => {
                        unimplemented!("Specified invalid mapping for bank E: {}", prev_value.mst())
                    }
                }
            }
            if value.enabled() {
                match value.mst() {
                    0 => self.map_lcdc(arm9, 0x20, 0x23, self.banks.e.as_ptr()),
                    1 => self.map_a_bg::<_, _, _, 4>(arm9, &self.banks.e, 0..4),
                    2 => self.map_a_obj::<_, _, _, 2>(arm9, &self.banks.e, 0..4),
                    3 => {
                        engine_3d.set_tex_pal_dirty(0xF);
                        self.map_tex_pal::<_, _, 0>(&self.banks.e, 0..4);
                    }
                    4 => self.map_a_bg_ext_pal::<_, _, 0>(&self.banks.e, 0..2),
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
        engine_3d: &mut Engine3d,
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
                        self.unmap_a_bg::<_, _, 0x4000, false, 5>(
                            arm9,
                            &self.banks.f,
                            once(base_region),
                        );
                        self.unmap_a_bg::<_, _, 0x4000, true, 5>(
                            arm9,
                            &self.banks.f,
                            once(base_region | 2),
                        );
                    }
                    2 => {
                        let base_region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        self.unmap_a_obj::<_, _, 0x4000, false, 3>(
                            arm9,
                            &self.banks.f,
                            once(base_region),
                        );
                        self.unmap_a_obj::<_, _, 0x4000, true, 3>(
                            arm9,
                            &self.banks.f,
                            once(base_region | 2),
                        );
                    }
                    3 => {
                        let region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        engine_3d.set_tex_pal_dirty(1 << region);
                        self.unmap_tex_pal::<_, 1>(once(region));
                    }
                    4 => {
                        let region = prev_value.offset() as usize & 1;
                        self.unmap_a_bg_ext_pal::<_, 1>(once(region));
                    }
                    5 => self.unmap_a_obj_ext_pal::<0>(),
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
                        self.map_a_bg::<_, _, _, 5>(
                            arm9,
                            &self.banks.f,
                            [base_region, base_region | 2],
                        );
                    }
                    2 => {
                        let base_region =
                            ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        self.map_a_obj::<_, _, _, 3>(
                            arm9,
                            &self.banks.f,
                            [base_region, base_region | 2],
                        );
                    }
                    3 => {
                        let region = ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        engine_3d.set_tex_pal_dirty(1 << region);
                        self.map_tex_pal::<_, _, 1>(&self.banks.f, once(region));
                    }
                    4 => {
                        let region = value.offset() as usize & 1;
                        self.map_a_bg_ext_pal::<_, _, 1>(&self.banks.f, once(region));
                    }
                    5 => self.map_a_obj_ext_pal::<_, 0>(&self.banks.f),
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
        engine_3d: &mut Engine3d,
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
                        self.unmap_a_bg::<_, _, 0x4000, false, 6>(
                            arm9,
                            &self.banks.g,
                            once(base_region),
                        );
                        self.unmap_a_bg::<_, _, 0x4000, true, 6>(
                            arm9,
                            &self.banks.g,
                            once(base_region | 2),
                        );
                    }
                    2 => {
                        let base_region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        self.unmap_a_obj::<_, _, 0x4000, false, 4>(
                            arm9,
                            &self.banks.g,
                            once(base_region),
                        );
                        self.unmap_a_obj::<_, _, 0x4000, true, 4>(
                            arm9,
                            &self.banks.g,
                            once(base_region | 2),
                        );
                    }
                    3 => {
                        let region =
                            ((prev_value.offset() & 1) | (prev_value.offset() & 2) << 1) as usize;
                        engine_3d.set_tex_pal_dirty(1 << region);
                        self.unmap_tex_pal::<_, 2>(once(region));
                    }
                    4 => {
                        let region = prev_value.offset() as usize & 1;
                        self.unmap_a_bg_ext_pal::<_, 2>(once(region));
                    }
                    5 => self.unmap_a_obj_ext_pal::<1>(),
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
                        self.map_a_bg::<_, _, _, 6>(
                            arm9,
                            &self.banks.g,
                            [base_region, base_region | 2],
                        );
                    }
                    2 => {
                        let base_region =
                            ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        self.map_a_obj::<_, _, _, 4>(
                            arm9,
                            &self.banks.g,
                            [base_region, base_region | 2],
                        );
                    }
                    3 => {
                        let region = ((value.offset() & 1) | (value.offset() & 2) << 1) as usize;
                        engine_3d.set_tex_pal_dirty(1 << region);
                        self.map_tex_pal::<_, _, 2>(&self.banks.g, once(region));
                    }
                    4 => {
                        let region = value.offset() as usize & 1;
                        self.map_a_bg_ext_pal::<_, _, 2>(&self.banks.g, once(region));
                    }
                    5 => self.map_a_obj_ext_pal::<_, 1>(&self.banks.g),
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
                    1 => {
                        self.unmap_b_bg::<_, _, 0x8000, false, 1>(arm9, &self.banks.h, once(0));
                        self.unmap_b_bg::<_, _, 0x8000, true, 1>(arm9, &self.banks.h, once(2));
                    }
                    2 => self.b_bg_ext_pal_ptr = self.zero_buffer.as_ptr(),
                    _ => {
                        unimplemented!("Specified invalid mapping for bank H: {}", prev_value.mst())
                    }
                }
            }
            if value.enabled() {
                match value.mst() & 3 {
                    0 => self.map_lcdc(arm9, 0x26, 0x27, self.banks.h.as_ptr()),
                    1 => self.map_b_bg::<_, _, _, 1>(arm9, &self.banks.h, [0, 2]),
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
        // Bank I requires special code for unmapping, as it gets mirrored inside what is
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
                        if self.map.b_bg[1].get() == 1 << 2 {
                            self.map.b_bg[1].set(0);
                            self.map.b_bg[3].set(0);
                            let mut b_bg = self.b_bg.as_byte_mut_slice();
                            self.banks
                                .i
                                .as_byte_mut_slice()
                                .copy_from_slice(&b_bg[0x8000..0xC000]);
                            b_bg[0x8000..0x1_0000].fill(0);
                            b_bg[0x1_8000..0x2_0000].fill(0);
                        } else {
                            for region in [1, 3] {
                                map_cpu_visible!(
                                    arm9,
                                    ptr_mask::R | ptr_mask::W_16_32,
                                    0x0620_0000,
                                    0x0640_0000,
                                    self.b_bg,
                                    region,
                                    15
                                );
                            }
                            self.map.b_bg[1].set(1);
                            let writeback_arr = self.writeback.b_bg.get_mut();
                            for usage_addr in 1 << 15..2 << 15 {
                                if writeback_arr[usage_addr / usize::BITS as usize]
                                    & 1 << (usage_addr & (usize::BITS - 1) as usize)
                                    == 0
                                {
                                    self.b_bg.write_unchecked(
                                        usage_addr,
                                        self.banks.c.read_unchecked(usage_addr),
                                    );
                                } else {
                                    self.banks.i.write_unchecked(
                                        usage_addr & 0x3FFF,
                                        self.b_bg.read_unchecked(usage_addr),
                                    );
                                }
                            }
                            for usage_addr in 3 << 15..4 << 15 {
                                if writeback_arr[usage_addr / usize::BITS as usize]
                                    & 1 << (usage_addr & (usize::BITS - 1) as usize)
                                    == 0
                                {
                                    self.b_bg.write_unchecked(
                                        usage_addr,
                                        self.banks.c.read_unchecked(usage_addr),
                                    );
                                }
                            }
                        }
                    }
                    2 => {
                        let new = self.map.b_obj[0].get() & !(1 << 1);
                        self.map.b_obj[0].set(new);
                        if new == 0 {
                            self.banks
                                .i
                                .as_byte_mut_slice()
                                .copy_from_slice(&self.b_obj.as_byte_slice()[..0x4000]);
                            self.b_obj.as_byte_mut_slice().fill(0);
                        } else {
                            arm9.map_sys_bus_ptr_range(
                                ptr_mask::R | ptr_mask::W_16_32,
                                self.b_obj.as_ptr(),
                                0x2_0000,
                                (0x0660_0000, 0x0680_0000),
                            );
                            let writeback_arr = self.writeback.b_obj.get_mut();
                            for (usage_addr, byte) in
                                self.b_obj.as_byte_mut_slice().iter_mut().enumerate()
                            {
                                if writeback_arr[usage_addr / usize::BITS as usize]
                                    & 1 << (usage_addr & (usize::BITS - 1) as usize)
                                    == 0
                                {
                                    *byte = self.banks.d.read_unchecked(usage_addr);
                                } else {
                                    self.banks.i.write_unchecked(usage_addr & 0x3FFF, *byte);
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
                    1 => self.map_b_bg::<_, _, _, 2>(arm9, &self.banks.i, [1, 3]),
                    2 => self.map_b_obj::<_, _, 1>(arm9, &self.banks.i),
                    _ => self.b_obj_ext_pal_ptr = self.banks.i.as_ptr(),
                }
            }
        }
    }
}
