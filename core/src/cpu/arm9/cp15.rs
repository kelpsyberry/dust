#[cfg(feature = "pu-checks")]
pub(in super::super) mod perms;
pub(super) mod ptrs;
pub(super) mod timings;
#[cfg(feature = "pu-checks")]
use perms::PermMap;

use super::Arm9;
use crate::{
    cpu::{Arm9Data, Engine},
    emu::Emu,
    utils::{bitfield_debug, OwnedBytesCellPtr},
};
use ptrs::Ptrs;
use timings::{Cycles, Timings};

type MapMask = u8;
mod map_mask {
    use super::MapMask;
    pub const R_CODE: MapMask = 1 << 0;
    pub const R_DATA: MapMask = 1 << 1;
    pub const W: MapMask = 1 << 2;
    pub const R_ALL: MapMask = R_CODE | R_DATA;
    pub const ALL: MapMask = R_ALL | W;
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Control(pub u32) {
        pub pu_enabled: bool @ 0,                     // x
        pub data_cache_enabled: bool @ 2,             // x
        pub big_endian: bool @ 7,                     // -
        pub code_cache_enabled: bool @ 12,            // x
        pub high_exc_vectors: bool @ 13,              // x
        pub round_robin_cache_replacement: bool @ 14, // -
        pub t_bit_load_disabled: bool @ 15,           // x
        pub dtcm_enabled: bool @ 16,                  // x
        pub dtcm_load_mode: bool @ 17,                // x
        pub itcm_enabled: bool @ 18,                  // x
        pub itcm_load_mode: bool @ 19,                // x
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct PuRegionControl(pub u32) {
        pub enabled: bool @ 0,
        pub size_shift: u8 @ 1..=5,
        pub raw_base_addr: u32 @ 12..=31,
    }
}

impl PuRegionControl {
    #[inline]
    pub fn base_addr(self) -> u32 {
        self.0 & 0xFFFF_F000
    }

    #[inline]
    pub fn size(self) -> u64 {
        2 << self.size_shift()
    }

    #[inline]
    pub fn bounds(self) -> (u32, u32) {
        let base_addr = self.base_addr();
        (base_addr, (base_addr as u64 + self.size() - 1) as u32)
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct PuRegionRawAccessPerms(pub u8) {
        pub data_2: u8 [read_only] @ 4..=5,
        pub data: u8 @ 4..=7,
        pub code_2: u8 [read_only] @ 0..=1,
        pub code: u8 @ 0..=3,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct PuRegionCacheAttrs(pub u8) {
        pub write_bufferable: bool @ 0,
        pub code_cachable: bool @ 1,
        pub data_cachable: bool @ 2,
        pub code_cache_active: bool @ 3,
        pub data_cache_active: bool @ 4,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PuRegion {
    pub active: bool,
    pub raw_perms: PuRegionRawAccessPerms,
    #[cfg(feature = "pu-checks")]
    perms: perms::Perms,
    pub cache_attrs: PuRegionCacheAttrs,
    pub bounds: (u32, u32),
    pub control: PuRegionControl,
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct TcmControl(pub u32) {
        pub size_shift: u8 @ 1..=5,
        pub raw_base_addr: u32 @ 12..=31,
    }
}

impl TcmControl {
    #[inline]
    pub fn base_addr(self) -> u32 {
        self.0 & 0xFFFF_F000
    }

    #[inline]
    pub fn size(self) -> u64 {
        0x200 << self.size_shift()
    }

    #[inline]
    pub fn bounds(self) -> (u32, u32) {
        let base_addr = self.base_addr();
        (base_addr, (base_addr as u64 + self.size() - 1) as u32)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TcmMode {
    Disabled,
    Load,
    Normal,
}

impl TcmMode {
    fn from_control_bits(bits: u32) -> Self {
        match bits & 3 {
            0b01 => TcmMode::Normal,
            0b11 => TcmMode::Load,
            _ => TcmMode::Disabled,
        }
    }

    fn rw_load_mode_mask(self) -> MapMask {
        map_mask::W
            | if self == TcmMode::Normal {
                map_mask::R_DATA
            } else {
                0
            }
    }

    fn rwx_load_mode_mask(self) -> MapMask {
        map_mask::W
            | if self == TcmMode::Normal {
                map_mask::R_ALL
            } else {
                0
            }
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct CacheLockdownControl(pub u32) {
        pub segment: u8 @ 0..=1,
        pub load: bool @ 31,
    }
}

pub struct Cp15 {
    itcm: OwnedBytesCellPtr<0x8000>,
    dtcm: OwnedBytesCellPtr<0x4000>,
    control: Control,
    pu_regions: [PuRegion; 8],
    dtcm_mode: TcmMode,
    dtcm_control: TcmControl,
    dtcm_bounds: (u32, u32),
    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    dtcm_addr_check_mask: u32,
    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    dtcm_addr_check_value: u32,
    itcm_mode: TcmMode,
    itcm_control: TcmControl,
    itcm_upper_bound: u32,
    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    itcm_addr_check_mask: u32,
    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    itcm_addr_check_value: u32,
    data_cache_lockdown_control: CacheLockdownControl,
    code_cache_lockdown_control: CacheLockdownControl,
    pub trace_process_id: u32,
    #[cfg(feature = "pu-checks")]
    pub(in super::super) perm_map: Box<PermMap>,
    pub(super) ptrs: Box<Ptrs>,
    pub(in super::super) timings: Box<Timings>,
}

impl Cp15 {
    const CODE_CACHE_AVG_TIMING: Cycles = Cycles::repeat(1);
    const DATA_CACHE_AVG_TIMING: Cycles = Cycles {
        r_n16_data: 3,
        r_n32_data: 3,
        r_s32_data: 1,
        w_n16_data: 3,
        w_n32_data: 3,
        w_s32_data: 1,
        code: 1,
    };

    pub(super) fn new() -> Self {
        Cp15 {
            itcm: OwnedBytesCellPtr::new_zeroed(),
            dtcm: OwnedBytesCellPtr::new_zeroed(),
            control: Control(0x0000_2078),
            pu_regions: [PuRegion {
                active: false,
                raw_perms: PuRegionRawAccessPerms(0),
                #[cfg(feature = "pu-checks")]
                perms: 0,
                cache_attrs: PuRegionCacheAttrs(0),
                bounds: (0, 0),
                control: PuRegionControl(0),
            }; 8],
            dtcm_mode: TcmMode::Disabled,
            dtcm_control: TcmControl(0),
            dtcm_bounds: (0, 0),
            #[cfg(any(feature = "bft-r", feature = "bft-w"))]
            dtcm_addr_check_mask: 0,
            #[cfg(any(feature = "bft-r", feature = "bft-w"))]
            dtcm_addr_check_value: 0xFFFF_FFFF,
            itcm_mode: TcmMode::Disabled,
            itcm_control: TcmControl(0),
            itcm_upper_bound: 0,
            #[cfg(any(feature = "bft-r", feature = "bft-w"))]
            itcm_addr_check_mask: 0,
            #[cfg(any(feature = "bft-r", feature = "bft-w"))]
            itcm_addr_check_value: 0xFFFF_FFFF,
            data_cache_lockdown_control: CacheLockdownControl(0),
            code_cache_lockdown_control: CacheLockdownControl(0),
            trace_process_id: 0,
            #[cfg(feature = "pu-checks")]
            perm_map: crate::utils::zeroed_box(),
            ptrs: Ptrs::new_boxed(),
            timings: Timings::new_boxed(),
        }
    }

    pub(super) fn setup<E: Engine>(emu: &mut Emu<E>) {
        emu.arm9.cp15.ptrs.copy_sys_bus(&emu.arm9.bus_ptrs);
        emu.arm9.cp15.timings.copy_sys_bus(&emu.arm9.bus_timings);
        #[cfg(feature = "pu-checks")]
        emu.arm9.cp15.perm_map.set_all(perms::perms::ALL);
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    #[inline]
    pub fn pu_regions(&self) -> &[PuRegion; 8] {
        &self.pu_regions
    }

    #[inline]
    pub fn dtcm_control(&self) -> TcmControl {
        self.dtcm_control
    }

    #[inline]
    pub fn dtcm_bounds(&self) -> (u32, u32) {
        self.dtcm_bounds
    }

    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    #[inline]
    pub(crate) fn dtcm_addr_check_mask(&self) -> u32 {
        self.dtcm_addr_check_mask
    }

    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    #[inline]
    pub(crate) fn dtcm_addr_check_value(&self) -> u32 {
        self.dtcm_addr_check_value
    }

    #[inline]
    pub fn itcm_control(&self) -> TcmControl {
        self.itcm_control
    }

    #[inline]
    pub fn itcm_upper_bound(&self) -> u32 {
        self.itcm_upper_bound
    }

    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    #[inline]
    pub(crate) fn itcm_addr_check_mask(&self) -> u32 {
        self.itcm_addr_check_mask
    }

    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    #[inline]
    pub(crate) fn itcm_addr_check_value(&self) -> u32 {
        self.itcm_addr_check_value
    }
}

impl<E: Engine> Arm9<E> {
    #[allow(clippy::similar_names)]
    pub fn set_cp15_control(emu: &mut Emu<E>, value: Control) {
        let prev_value = emu.arm9.cp15.control;
        emu.arm9.cp15.control.0 =
            (emu.arm9.cp15.control.0 & !0x000F_F085) | (value.0 & 0x000F_F085);

        let changed = Control(value.0 ^ prev_value.0);
        if changed.high_exc_vectors() {
            emu.arm9
                .engine_data
                .set_high_exc_vectors(value.high_exc_vectors());
        }
        if changed.t_bit_load_disabled() {
            emu.arm9
                .engine_data
                .set_t_bit_load_disabled(value.t_bit_load_disabled());
        }

        let prev_dtcm_mode = emu.arm9.cp15.dtcm_mode;
        let prev_itcm_mode = emu.arm9.cp15.itcm_mode;
        let dtcm_mode = TcmMode::from_control_bits(value.0 >> 16);
        let itcm_mode = TcmMode::from_control_bits(value.0 >> 18);
        emu.arm9.cp15.dtcm_mode = dtcm_mode;
        emu.arm9.cp15.itcm_mode = itcm_mode;

        if dtcm_mode != prev_dtcm_mode || itcm_mode != prev_itcm_mode {
            #[cfg(any(feature = "bft-r", feature = "bft-w"))]
            {
                if dtcm_mode == TcmMode::Disabled {
                    emu.arm9.cp15.dtcm_addr_check_mask = 0;
                    emu.arm9.cp15.dtcm_addr_check_value = 0xFFFF_FFFF;
                } else {
                    emu.arm9.cp15.dtcm_addr_check_mask =
                        !((emu.arm9.cp15.dtcm_control.size() - 1) as u32);
                    emu.arm9.cp15.dtcm_addr_check_value = emu.arm9.cp15.dtcm_bounds.0;
                }

                if itcm_mode == TcmMode::Disabled {
                    emu.arm9.cp15.itcm_addr_check_mask = 0;
                    emu.arm9.cp15.itcm_addr_check_value = 0xFFFF_FFFF;
                } else {
                    emu.arm9.cp15.itcm_addr_check_mask = !emu.arm9.cp15.itcm_upper_bound;
                    emu.arm9.cp15.itcm_addr_check_value = 0;
                }
            }

            let mut unmap = [(0, (0, 0)); 2];
            let mut overlay_dtcm = [(0, (0, 0)); 2];
            let mut overlay_itcm = [(0, (0, 0)); 2];

            if dtcm_mode != TcmMode::Disabled {
                Self::check_dtcm_size(emu);
                match (prev_dtcm_mode, dtcm_mode) {
                    (TcmMode::Disabled, _) => {
                        overlay_dtcm[0] =
                            (dtcm_mode.rw_load_mode_mask(), emu.arm9.cp15.dtcm_bounds);
                    }
                    (TcmMode::Load, TcmMode::Normal) => {
                        overlay_dtcm[0] = (map_mask::R_DATA, emu.arm9.cp15.dtcm_bounds);
                    }
                    (TcmMode::Normal, TcmMode::Load) => {
                        unmap[0] = (map_mask::R_DATA, emu.arm9.cp15.dtcm_bounds);
                    }
                    _ => {}
                }
            } else if prev_dtcm_mode != TcmMode::Disabled {
                unmap[0] = (
                    prev_dtcm_mode.rw_load_mode_mask(),
                    emu.arm9.cp15.dtcm_bounds,
                );
            }

            if itcm_mode != TcmMode::Disabled {
                Self::check_itcm_size(emu);
                overlay_itcm[0] = (
                    (overlay_dtcm[0].0 | unmap[0].0) & (itcm_mode.rw_load_mode_mask()),
                    emu.arm9.cp15.dtcm_bounds,
                );
                match (prev_itcm_mode, itcm_mode) {
                    (TcmMode::Disabled, _) => {
                        overlay_itcm[1] = (
                            itcm_mode.rwx_load_mode_mask(),
                            (0, emu.arm9.cp15.itcm_upper_bound),
                        );
                    }
                    (TcmMode::Load, TcmMode::Normal) => {
                        overlay_itcm[1] = (map_mask::R_ALL, (0, emu.arm9.cp15.itcm_upper_bound));
                    }
                    (TcmMode::Normal, TcmMode::Load) => {
                        unmap[1] = (map_mask::R_ALL, (0, emu.arm9.cp15.itcm_upper_bound));
                    }
                    _ => {}
                }
            } else if prev_itcm_mode != TcmMode::Disabled {
                unmap[1] = (
                    prev_itcm_mode.rwx_load_mode_mask(),
                    (0, emu.arm9.cp15.itcm_upper_bound),
                );
            }

            if unmap[1].0 != 0 && dtcm_mode != TcmMode::Disabled {
                overlay_dtcm[1] = (
                    unmap[1].0 & (dtcm_mode.rw_load_mode_mask()),
                    (0, emu.arm9.cp15.itcm_upper_bound),
                );
            }

            macro_rules! merge_ranges {
                ($arr: expr) => {
                    if $arr[0].0 == $arr[1].0
                        && $arr[1].1 .0 as u64 <= $arr[0].1 .1 as u64 + 1
                        && $arr[1].1 .1 as u64 + 1 >= $arr[0].1 .0 as u64
                    {
                        $arr[0].1 .0 = $arr[0].1 .0.min($arr[1].1 .0);
                        $arr[0].1 .1 = $arr[0].1 .1.max($arr[1].1 .1);
                        $arr[1].0 = 0;
                    }
                };
            }
            merge_ranges!(unmap);
            merge_ranges!(overlay_dtcm);
            merge_ranges!(overlay_itcm);

            for (map_mask, bounds) in unmap {
                if map_mask != 0 {
                    emu.arm9
                        .cp15
                        .ptrs
                        .unmap_cpu_local_range(map_mask, bounds, &emu.arm9.bus_ptrs);
                    emu.arm9.cp15.timings.unset_cpu_local_range(
                        map_mask,
                        bounds,
                        &emu.arm9.bus_timings,
                    );
                }
            }

            for (map_mask, bounds) in overlay_dtcm {
                if map_mask != 0 {
                    unsafe {
                        emu.arm9.cp15.ptrs.map_cpu_local_subrange(
                            map_mask,
                            emu.arm9.cp15.dtcm.as_ptr(),
                            0x4000,
                            emu.arm9.cp15.dtcm_bounds,
                            bounds,
                        );
                    }
                    emu.arm9.cp15.timings.set_cpu_local_subrange(
                        map_mask,
                        Cycles::repeat(1),
                        emu.arm9.cp15.dtcm_bounds,
                        bounds,
                    );
                }
            }

            for (map_mask, bounds) in overlay_itcm {
                if map_mask != 0 {
                    unsafe {
                        emu.arm9.cp15.ptrs.map_cpu_local_subrange(
                            map_mask,
                            emu.arm9.cp15.itcm.as_ptr(),
                            0x8000,
                            (0, emu.arm9.cp15.itcm_upper_bound),
                            bounds,
                        );
                    }
                    emu.arm9.cp15.timings.set_cpu_local_subrange(
                        map_mask,
                        Cycles::repeat(1),
                        (0, emu.arm9.cp15.itcm_upper_bound),
                        bounds,
                    );
                }
            }
        }

        let changed_cache_control_bits = Control((value.0 ^ prev_value.0) & 0x1005);
        if Control(prev_value.0 | value.0).pu_enabled() && changed_cache_control_bits.0 != 0 {
            // The cache configuration was changed, or the PU was switched on/off, so recalculate
            // all affected PU/cache-related attributes and re-apply them to the bus pointers and
            // timings.

            for region in &mut emu.arm9.cp15.pu_regions {
                region.cache_attrs.set_code_cache_active(
                    value.code_cache_enabled() && region.cache_attrs.code_cachable(),
                );
                region.cache_attrs.set_data_cache_active(
                    value.data_cache_enabled() && region.cache_attrs.data_cachable(),
                );
            }

            match (prev_value.pu_enabled(), value.pu_enabled()) {
                (true, true) => {
                    // Only the cache configuration changed, re-apply it
                    let mut cache_map_mask = 0;
                    if changed_cache_control_bits.code_cache_enabled() {
                        cache_map_mask |= map_mask::R_CODE;
                    }
                    if changed_cache_control_bits.data_cache_enabled() {
                        cache_map_mask |= map_mask::R_DATA | map_mask::W;
                    }
                    Self::remap_all_pu_region_cache_attrs(emu, cache_map_mask);
                }

                (false, true) => {
                    // PU was switched on, remap everything
                    for region in &mut emu.arm9.cp15.pu_regions {
                        region.active = region.control.enabled();
                    }
                    Self::remap_all_pu_regions(emu);
                }

                (true, false) => {
                    // PU was switched off, set all permissions to RWX and unmap everything
                    for region in &mut emu.arm9.cp15.pu_regions {
                        region.active = false;
                    }

                    #[cfg(feature = "pu-checks")]
                    emu.arm9.cp15.perm_map.set_all(perms::perms::ALL);

                    emu.arm9.cp15.timings.copy_sys_bus(&emu.arm9.bus_timings);

                    if emu.arm9.cp15.dtcm_mode != TcmMode::Disabled {
                        let dtcm_map_mask = emu.arm9.cp15.dtcm_mode.rw_load_mode_mask();
                        emu.arm9.cp15.timings.set_cpu_local_range(
                            dtcm_map_mask,
                            Cycles::repeat(1),
                            emu.arm9.cp15.dtcm_bounds,
                        );
                    }

                    if emu.arm9.cp15.itcm_mode != TcmMode::Disabled {
                        let itcm_map_mask = emu.arm9.cp15.itcm_mode.rwx_load_mode_mask();
                        emu.arm9.cp15.timings.set_cpu_local_range(
                            itcm_map_mask,
                            Cycles::repeat(1),
                            (0, emu.arm9.cp15.itcm_upper_bound),
                        );
                    }
                }

                _ => unreachable!(),
            }
        }
    }

    fn remap_all_pu_regions(emu: &mut Emu<E>) {
        #[cfg(feature = "pu-checks")]
        {
            emu.arm9.cp15.perm_map.set_all(0);
            for region in &emu.arm9.cp15.pu_regions {
                if region.active {
                    emu.arm9
                        .cp15
                        .perm_map
                        .set_range(region.perms, region.bounds);
                }
            }
        }
        Self::remap_all_pu_region_cache_attrs(emu, map_mask::ALL);
    }

    #[allow(clippy::similar_names)]
    fn remap_all_pu_region_cache_attrs(emu: &mut Emu<E>, map_mask: MapMask) {
        emu.arm9.cp15.timings.copy_sys_bus(&emu.arm9.bus_timings);

        for region in &emu.arm9.cp15.pu_regions {
            if region.active {
                if map_mask & map_mask::R_CODE != 0 {
                    if region.cache_attrs.code_cache_active() {
                        emu.arm9.cp15.timings.set_cpu_local_range(
                            map_mask::R_CODE,
                            Cp15::CODE_CACHE_AVG_TIMING,
                            region.bounds,
                        );
                    } else {
                        emu.arm9.cp15.timings.unset_cpu_local_range(
                            map_mask::R_CODE,
                            region.bounds,
                            &emu.arm9.bus_timings,
                        );
                    }
                }

                let data_cache_map_mask = map_mask & (map_mask::R_DATA | map_mask::W);
                if data_cache_map_mask != 0 {
                    if region.cache_attrs.data_cache_active() {
                        emu.arm9.cp15.timings.set_cpu_local_range(
                            data_cache_map_mask,
                            Cp15::DATA_CACHE_AVG_TIMING,
                            region.bounds,
                        );
                    } else {
                        emu.arm9.cp15.timings.unset_cpu_local_range(
                            data_cache_map_mask,
                            region.bounds,
                            &emu.arm9.bus_timings,
                        );
                    }
                }
            }
        }

        let dtcm_map_mask = map_mask & emu.arm9.cp15.dtcm_mode.rw_load_mode_mask();
        if emu.arm9.cp15.dtcm_mode != TcmMode::Disabled && dtcm_map_mask != 0 {
            emu.arm9.cp15.timings.set_cpu_local_range(
                dtcm_map_mask,
                Cycles::repeat(1),
                emu.arm9.cp15.dtcm_bounds,
            );
        }

        let itcm_map_mask = map_mask & emu.arm9.cp15.itcm_mode.rwx_load_mode_mask();
        if emu.arm9.cp15.itcm_mode != TcmMode::Disabled && itcm_map_mask != 0 {
            emu.arm9.cp15.timings.set_cpu_local_range(
                itcm_map_mask,
                Cycles::repeat(1),
                (0, emu.arm9.cp15.itcm_upper_bound),
            );
        }
    }

    #[allow(clippy::similar_names)]
    fn map_all_pu_region_subrange_cache_attrs(
        emu: &mut Emu<E>,
        map_mask: MapMask,
        bounds: (u32, u32),
    ) {
        for region in &emu.arm9.cp15.pu_regions {
            if region.active {
                if map_mask & map_mask::R_CODE != 0 {
                    if region.cache_attrs.code_cache_active() {
                        emu.arm9.cp15.timings.set_cpu_local_subrange(
                            map_mask::R_CODE,
                            Cp15::CODE_CACHE_AVG_TIMING,
                            region.bounds,
                            bounds,
                        );
                    } else {
                        emu.arm9.cp15.timings.unset_cpu_local_subrange(
                            map_mask::R_CODE,
                            region.bounds,
                            bounds,
                            &emu.arm9.bus_timings,
                        );
                    }
                }

                let data_cache_map_mask = map_mask & (map_mask::R_DATA | map_mask::W);
                if data_cache_map_mask != 0 {
                    if region.cache_attrs.data_cache_active() {
                        emu.arm9.cp15.timings.set_cpu_local_subrange(
                            data_cache_map_mask,
                            Cp15::DATA_CACHE_AVG_TIMING,
                            region.bounds,
                            bounds,
                        );
                    } else {
                        emu.arm9.cp15.timings.unset_cpu_local_subrange(
                            data_cache_map_mask,
                            region.bounds,
                            bounds,
                            &emu.arm9.bus_timings,
                        );
                    }
                }
            }
        }

        let dtcm_map_mask = map_mask & emu.arm9.cp15.dtcm_mode.rw_load_mode_mask();
        if emu.arm9.cp15.dtcm_mode != TcmMode::Disabled && dtcm_map_mask != 0 {
            emu.arm9.cp15.timings.set_cpu_local_subrange(
                dtcm_map_mask,
                Cycles::repeat(1),
                emu.arm9.cp15.dtcm_bounds,
                bounds,
            );
        }

        let itcm_map_mask = map_mask & emu.arm9.cp15.itcm_mode.rwx_load_mode_mask();
        if emu.arm9.cp15.itcm_mode != TcmMode::Disabled && itcm_map_mask != 0 {
            emu.arm9.cp15.timings.set_cpu_local_subrange(
                itcm_map_mask,
                Cycles::repeat(1),
                (0, emu.arm9.cp15.itcm_upper_bound),
                bounds,
            );
        }
    }

    fn check_dtcm_size(emu: &mut Emu<E>) {
        let size_shift = emu.arm9.cp15.dtcm_control.size_shift();
        assert!(
            (3..=23).contains(&size_shift),
            "Unpredictable DTCM size shift specified: {}",
            size_shift
        );
        let base_addr = emu.arm9.cp15.dtcm_control.base_addr();
        let size = emu.arm9.cp15.dtcm_control.size();
        assert!(
            base_addr & (size - 1) as u32 == 0,
            "Unpredictable misaligned DTCM base address specified: {:#010X} (size is {:#X})",
            base_addr,
            size,
        );
        assert!(
            size >= Ptrs::PAGE_SIZE.max(Timings::PAGE_SIZE) as u64,
            concat!(
                "Specified a DTCM size that can't be handled by the emulator: {:#X} (the ",
                "minimum TCM size is defined as {:#X})",
            ),
            size,
            Ptrs::PAGE_SIZE.max(Timings::PAGE_SIZE),
        );
    }

    #[allow(clippy::similar_names)]
    pub fn set_cp15_dtcm_control(emu: &mut Emu<E>, value: TcmControl) {
        let prev_control = emu.arm9.cp15.dtcm_control;
        emu.arm9.cp15.dtcm_control.0 = value.0 & 0xFFFF_F03E;

        if emu.arm9.cp15.dtcm_control == prev_control {
            return;
        }

        let prev_bounds = emu.arm9.cp15.dtcm_bounds;
        emu.arm9.cp15.dtcm_bounds = emu.arm9.cp15.dtcm_control.bounds();

        if !emu.arm9.cp15.control.dtcm_enabled() {
            return;
        }

        Self::check_dtcm_size(emu);

        #[cfg(any(feature = "bft-r", feature = "bft-w"))]
        {
            emu.arm9.cp15.dtcm_addr_check_mask = !((emu.arm9.cp15.dtcm_control.size() - 1) as u32);
            emu.arm9.cp15.dtcm_addr_check_value = emu.arm9.cp15.dtcm_bounds.0;
        }

        let dtcm_map_mask = emu.arm9.cp15.dtcm_mode.rw_load_mode_mask();
        let dtcm_bounds = emu.arm9.cp15.dtcm_bounds;
        unsafe {
            emu.arm9.cp15.ptrs.unmap_cpu_local_range(
                dtcm_map_mask,
                prev_bounds,
                &emu.arm9.bus_ptrs,
            );
            if emu.arm9.cp15.control.itcm_enabled() {
                let itcm_map_mask = dtcm_map_mask & emu.arm9.cp15.itcm_mode.rw_load_mode_mask();
                let itcm_bounds = (0, emu.arm9.cp15.itcm_upper_bound);
                emu.arm9.cp15.ptrs.map_cpu_local_subrange(
                    itcm_map_mask,
                    emu.arm9.cp15.itcm.as_ptr(),
                    0x8000,
                    itcm_bounds,
                    prev_bounds,
                );
                emu.arm9.cp15.ptrs.map_cpu_local_range(
                    dtcm_map_mask,
                    emu.arm9.cp15.dtcm.as_ptr(),
                    0x4000,
                    dtcm_bounds,
                );
                emu.arm9.cp15.ptrs.map_cpu_local_subrange(
                    itcm_map_mask,
                    emu.arm9.cp15.itcm.as_ptr(),
                    0x8000,
                    itcm_bounds,
                    dtcm_bounds,
                );
            } else {
                emu.arm9.cp15.ptrs.map_cpu_local_range(
                    dtcm_map_mask,
                    emu.arm9.cp15.dtcm.as_ptr(),
                    0x4000,
                    dtcm_bounds,
                );
            }
            emu.arm9.cp15.timings.unset_cpu_local_range(
                dtcm_map_mask,
                prev_bounds,
                &emu.arm9.bus_timings,
            );
            // ITCM timings will be applied to `prev_bounds` together with cache timings
            emu.arm9.cp15.timings.set_cpu_local_range(
                dtcm_map_mask,
                Cycles::repeat(1),
                dtcm_bounds,
            );
        }
        Self::map_all_pu_region_subrange_cache_attrs(emu, dtcm_map_mask, prev_bounds);
    }

    fn check_itcm_size(emu: &mut Emu<E>) {
        let size_shift = emu.arm9.cp15.itcm_control.size_shift();
        assert!(
            (3..=23).contains(&size_shift),
            "Unpredictable ITCM size shift specified: {}",
            size_shift
        );
        let size = emu.arm9.cp15.itcm_control.size();
        assert!(
            size >= Ptrs::PAGE_SIZE.max(Timings::PAGE_SIZE) as u64,
            concat!(
                "Specified an ITCM size that can't be handled by the emulator: {:#X} (the ",
                "minimum TCM size is defined as {:#X})",
            ),
            size,
            Ptrs::PAGE_SIZE.max(Timings::PAGE_SIZE),
        );
    }

    #[allow(clippy::similar_names)]
    pub fn set_cp15_itcm_control(emu: &mut Emu<E>, value: TcmControl) {
        let prev_control = emu.arm9.cp15.itcm_control;
        emu.arm9.cp15.itcm_control.0 = value.0 & 0x3E;

        if emu.arm9.cp15.itcm_control == prev_control {
            return;
        }

        let prev_upper_bound = emu.arm9.cp15.itcm_upper_bound;
        emu.arm9.cp15.itcm_upper_bound = (emu.arm9.cp15.itcm_control.size() - 1) as u32;

        if !emu.arm9.cp15.control.itcm_enabled() {
            return;
        }

        Self::check_itcm_size(emu);

        #[cfg(any(feature = "bft-r", feature = "bft-w"))]
        {
            emu.arm9.cp15.itcm_addr_check_mask = !emu.arm9.cp15.itcm_upper_bound;
            emu.arm9.cp15.itcm_addr_check_value = 0;
        }

        let itcm_map_mask = emu.arm9.cp15.itcm_mode.rwx_load_mode_mask();
        let prev_bounds = (0, prev_upper_bound);
        let itcm_bounds = (0, emu.arm9.cp15.itcm_upper_bound);
        unsafe {
            emu.arm9.cp15.ptrs.unmap_cpu_local_range(
                itcm_map_mask,
                prev_bounds,
                &emu.arm9.bus_ptrs,
            );
            if emu.arm9.cp15.control.dtcm_enabled() {
                let dtcm_map_mask = itcm_map_mask & emu.arm9.cp15.dtcm_mode.rw_load_mode_mask();
                let dtcm_bounds = emu.arm9.cp15.dtcm_bounds;
                emu.arm9.cp15.ptrs.map_cpu_local_subrange(
                    dtcm_map_mask,
                    emu.arm9.cp15.dtcm.as_ptr(),
                    0x4000,
                    dtcm_bounds,
                    prev_bounds,
                );
            }
            emu.arm9.cp15.ptrs.map_cpu_local_range(
                itcm_map_mask,
                emu.arm9.cp15.itcm.as_ptr(),
                0x8000,
                itcm_bounds,
            );
            emu.arm9.cp15.timings.unset_cpu_local_range(
                itcm_map_mask,
                prev_bounds,
                &emu.arm9.bus_timings,
            );
            emu.arm9.cp15.timings.set_cpu_local_range(
                itcm_map_mask,
                Cycles::repeat(1),
                itcm_bounds,
            );
        }
        Self::map_all_pu_region_subrange_cache_attrs(emu, itcm_map_mask, prev_bounds);
    }

    pub fn read_cp15_reg(emu: &mut Emu<E>, opcode_1: u8, cn: u8, cm: u8, opcode_2: u8) -> u32 {
        if opcode_1 != 0 {
            #[cfg(feature = "log")]
            slog::warn!(
                emu.arm9.logger,
                "Unknown CP15 reg read @ {},C{},C{},{}",
                opcode_1,
                cn,
                cm,
                opcode_2
            );
            return 0;
        }
        #[cfg(feature = "log")]
        slog::trace!(emu.arm9.logger, "Read CP15 C{},C{},{}", cn, cm, opcode_2);
        match (cn, cm, opcode_2) {
            (0, 0, 0 | 3..=7) => 0x4105_9461, // ID code (mirrored)
            (0, 0, 1) => 0x0F0D_2112,         // Cache type
            (0, 0, 2) => 0x0014_0180,         // TCM size

            (1, 0, 0) => emu.arm9.cp15.control.0, // Control

            // Data cache configuration
            (2, 0, 0) => emu
                .arm9
                .cp15
                .pu_regions
                .iter()
                .enumerate()
                .fold(0, |acc, (i, region)| {
                    acc | (region.cache_attrs.data_cachable() as u32) << i
                }),

            // Instruction cache configuration
            (2, 0, 1) => emu
                .arm9
                .cp15
                .pu_regions
                .iter()
                .enumerate()
                .fold(0, |acc, (i, region)| {
                    acc | (region.cache_attrs.code_cachable() as u32) << i
                }),

            // Write buffer control
            (3, 0, 0) => emu
                .arm9
                .cp15
                .pu_regions
                .iter()
                .enumerate()
                .fold(0, |acc, (i, region)| {
                    acc | (region.cache_attrs.write_bufferable() as u32) << i
                }),

            // Data access permission bits (for backwards compatibility)
            (5, 0, 0) => emu
                .arm9
                .cp15
                .pu_regions
                .iter()
                .enumerate()
                .fold(0, |acc, (i, region)| {
                    acc | (region.raw_perms.data_2() as u32) << (i << 1)
                }),

            // Instruction access permission bits (for backwards compatibility)
            (5, 0, 1) => emu
                .arm9
                .cp15
                .pu_regions
                .iter()
                .enumerate()
                .fold(0, |acc, (i, region)| {
                    acc | (region.raw_perms.code_2() as u32) << (i << 1)
                }),

            // Data access permission bits
            (5, 0, 2) => emu
                .arm9
                .cp15
                .pu_regions
                .iter()
                .enumerate()
                .fold(0, |acc, (i, region)| {
                    acc | (region.raw_perms.data() as u32) << (i << 2)
                }),

            // Instruction access permission bits
            (5, 0, 3) => emu
                .arm9
                .cp15
                .pu_regions
                .iter()
                .enumerate()
                .fold(0, |acc, (i, region)| {
                    acc | (region.raw_perms.code() as u32) << (i << 2)
                }),

            // Protection region base and size
            (6, region @ 0..=7, 0..=1) => emu.arm9.cp15.pu_regions[region as usize].control.0,

            (9, 0, 0) => emu.arm9.cp15.data_cache_lockdown_control.0, // Data cache lockdown
            (9, 0, 1) => emu.arm9.cp15.code_cache_lockdown_control.0, // Instruction cache lockdown

            (9, 1, 0) => emu.arm9.cp15.dtcm_control.0, // DTCM base and size
            (9, 1, 1) => emu.arm9.cp15.itcm_control.0, // ITCM base and size

            (13, 0..=1, 1) => emu.arm9.cp15.trace_process_id, // Trace process identifier

            // TODO?: register 15 (BIST and cache debug)
            _ => {
                #[cfg(feature = "log")]
                slog::warn!(
                    emu.arm9.logger,
                    "Unknown CP15 reg read @ C{},C{},{}",
                    cn,
                    cm,
                    opcode_2
                );
                0
            }
        }
    }

    pub fn write_cp15_reg(
        emu: &mut Emu<E>,
        opcode_1: u8,
        cn: u8,
        cm: u8,
        opcode_2: u8,
        value: u32,
    ) {
        if opcode_1 != 0 {
            #[cfg(feature = "log")]
            slog::warn!(
                emu.arm9.logger,
                "Unknown CP15 reg write @ {},C{},C{},{}: {:#010X}",
                opcode_1,
                cn,
                cm,
                opcode_2,
                value
            );
            return;
        }
        #[cfg(feature = "log")]
        slog::trace!(
            emu.arm9.logger,
            "Write CP15 C{},C{},{}: {:#010X}",
            cn,
            cm,
            opcode_2,
            value,
        );
        match (cn, cm, opcode_2) {
            (1, 0, 0) => Self::set_cp15_control(emu, Control(value)), // Control

            // Data cache configuration
            (2, 0, 0) => {
                let mut changed_start_i = 8;
                for i in (0..8).rev() {
                    let region = &mut emu.arm9.cp15.pu_regions[i];
                    let prev_active = region.cache_attrs.data_cache_active();
                    let new_cachable = value & 1 << i != 0;
                    let new_active = new_cachable && emu.arm9.cp15.control.data_cache_enabled();
                    region.cache_attrs = region
                        .cache_attrs
                        .with_data_cachable(new_cachable)
                        .with_data_cache_active(new_active);
                    if region.active && new_active != prev_active {
                        changed_start_i = i;
                    }
                }
                for region in &emu.arm9.cp15.pu_regions[changed_start_i..] {
                    if region.active {
                        if region.cache_attrs.data_cache_active() {
                            emu.arm9.cp15.timings.set_cpu_local_range(
                                map_mask::R_DATA | map_mask::W,
                                Cp15::DATA_CACHE_AVG_TIMING,
                                region.bounds,
                            );
                        } else {
                            emu.arm9.cp15.timings.unset_cpu_local_range(
                                map_mask::R_DATA | map_mask::W,
                                region.bounds,
                                &emu.arm9.bus_timings,
                            );
                        }
                        if emu.arm9.cp15.dtcm_mode != TcmMode::Disabled {
                            emu.arm9.cp15.timings.set_cpu_local_subrange(
                                emu.arm9.cp15.dtcm_mode.rw_load_mode_mask(),
                                Cycles::repeat(1),
                                emu.arm9.cp15.dtcm_bounds,
                                region.bounds,
                            );
                        }
                        if emu.arm9.cp15.itcm_mode != TcmMode::Disabled {
                            emu.arm9.cp15.timings.set_cpu_local_subrange(
                                emu.arm9.cp15.itcm_mode.rw_load_mode_mask(),
                                Cycles::repeat(1),
                                (0, emu.arm9.cp15.itcm_upper_bound),
                                region.bounds,
                            );
                        }
                    }
                }
            }

            // Instruction cache configuration
            (2, 0, 1) => {
                let mut changed_start_i = 8;
                for i in (0..8).rev() {
                    let region = &mut emu.arm9.cp15.pu_regions[i];
                    let prev_active = region.cache_attrs.code_cache_active();
                    let new_cachable = value & 1 << i != 0;
                    let new_active = new_cachable && emu.arm9.cp15.control.code_cache_enabled();
                    region.cache_attrs = region
                        .cache_attrs
                        .with_code_cachable(new_cachable)
                        .with_code_cache_active(new_active);
                    if region.active && new_active != prev_active {
                        changed_start_i = i;
                    }
                }
                for region in &emu.arm9.cp15.pu_regions[changed_start_i..] {
                    if region.active {
                        if region.cache_attrs.code_cache_active() {
                            emu.arm9.cp15.timings.set_cpu_local_range(
                                map_mask::R_CODE,
                                Cp15::CODE_CACHE_AVG_TIMING,
                                region.bounds,
                            );
                        } else {
                            emu.arm9.cp15.timings.unset_cpu_local_range(
                                map_mask::R_CODE,
                                region.bounds,
                                &emu.arm9.bus_timings,
                            );
                        }
                        if emu.arm9.cp15.itcm_mode == TcmMode::Normal {
                            emu.arm9.cp15.timings.set_cpu_local_subrange(
                                map_mask::R_CODE,
                                Cycles::repeat(1),
                                (0, emu.arm9.cp15.itcm_upper_bound),
                                region.bounds,
                            );
                        }
                    }
                }
            }

            // Write buffer control
            (3, 0, 0) => {
                for i in (0..8).rev() {
                    let region = &mut emu.arm9.cp15.pu_regions[i];
                    region.cache_attrs.set_write_bufferable(value & 1 << i != 0);
                }
            }

            // Data access permission bits (for backwards compatibility)
            (5, 0, 0) => {
                #[cfg(feature = "pu-checks")]
                let mut changed_start_i = 8;
                for i in (0..8).rev() {
                    let region = &mut emu.arm9.cp15.pu_regions[i];
                    let new_setting = (value >> (i << 1) & 3) as u8;
                    region.raw_perms.set_data(new_setting);
                    #[cfg(feature = "pu-checks")]
                    {
                        let prev_perms = region.perms;
                        region.perms = perms::perms::set_data_from_raw(region.perms, new_setting);
                        if region.active && region.perms != prev_perms {
                            changed_start_i = i;
                        }
                    }
                }
                #[cfg(feature = "pu-checks")]
                for region in &emu.arm9.cp15.pu_regions[changed_start_i..] {
                    if region.active {
                        emu.arm9
                            .cp15
                            .perm_map
                            .set_range(region.perms, region.bounds);
                    }
                }
            }

            // Code access permission bits (for backwards compatibility)
            (5, 0, 1) => {
                #[cfg(feature = "pu-checks")]
                let mut changed_start_i = 8;
                for i in (0..8).rev() {
                    let region = &mut emu.arm9.cp15.pu_regions[i];
                    let new_setting = (value >> (i << 1) & 3) as u8;
                    region.raw_perms.set_code(new_setting);
                    #[cfg(feature = "pu-checks")]
                    {
                        let prev_perms = region.perms;
                        region.perms = perms::perms::set_code_from_raw(region.perms, new_setting);
                        if region.active && region.perms != prev_perms {
                            changed_start_i = i;
                        }
                    }
                }
                #[cfg(feature = "pu-checks")]
                for region in &emu.arm9.cp15.pu_regions[changed_start_i..] {
                    if region.active {
                        emu.arm9
                            .cp15
                            .perm_map
                            .set_range(region.perms, region.bounds);
                    }
                }
            }

            // Data access permission bits
            (5, 0, 2) => {
                #[cfg(feature = "pu-checks")]
                let mut changed_start_i = 8;
                for i in (0..8).rev() {
                    let region = &mut emu.arm9.cp15.pu_regions[i];
                    let new_setting = (value >> (i << 2) & 0xF) as u8;
                    region.raw_perms.set_data(new_setting);
                    #[cfg(feature = "pu-checks")]
                    {
                        let prev_perms = region.perms;
                        region.perms = perms::perms::set_data_from_raw(region.perms, new_setting);
                        if region.active && region.perms != prev_perms {
                            changed_start_i = i;
                        }
                    }
                }
                #[cfg(feature = "pu-checks")]
                for region in &emu.arm9.cp15.pu_regions[changed_start_i..] {
                    if region.active {
                        emu.arm9
                            .cp15
                            .perm_map
                            .set_range(region.perms, region.bounds);
                    }
                }
            }

            // Code access permission bits
            (5, 0, 3) => {
                #[cfg(feature = "pu-checks")]
                let mut changed_start_i = 8;
                for i in (0..8).rev() {
                    let region = &mut emu.arm9.cp15.pu_regions[i];
                    let new_setting = (value >> (i << 2) & 0xF) as u8;
                    region.raw_perms.set_code(new_setting);
                    #[cfg(feature = "pu-checks")]
                    {
                        let prev_perms = region.perms;
                        region.perms = perms::perms::set_code_from_raw(region.perms, new_setting);
                        if region.active && region.perms != prev_perms {
                            changed_start_i = i;
                        }
                    }
                }
                #[cfg(feature = "pu-checks")]
                for region in &emu.arm9.cp15.pu_regions[changed_start_i..] {
                    if region.active {
                        emu.arm9
                            .cp15
                            .perm_map
                            .set_range(region.perms, region.bounds);
                    }
                }
            }

            // Protection region base and size
            (6, region_i @ 0..=7, 0..=1) => {
                let new_setting = PuRegionControl(value & 0xFFFF_F03F);
                if new_setting.enabled() {
                    let size_shift = new_setting.size_shift();
                    assert!(
                        size_shift >= 11,
                        "Unpredictable PU region {} size shift specified: {}",
                        region_i,
                        size_shift,
                    );
                    let base_addr = new_setting.base_addr();
                    let size = new_setting.size();
                    assert!(
                        base_addr & (size - 1) as u32 == 0,
                        concat!(
                            "Unpredictable misaligned PU region {} base address specified: ",
                            "{:#010X} (size is {:#X})"
                        ),
                        region_i,
                        base_addr,
                        size,
                    );
                    #[cfg(feature = "pu-checks")]
                    assert!(
                        size >= PermMap::PAGE_SIZE.max(Timings::PAGE_SIZE) as u64,
                        concat!(
                            "Specified a PU region size that can't be handled by the emulator: ",
                            "{:#X} (the minimum PU region size is defined as {:#X})",
                        ),
                        size,
                        PermMap::PAGE_SIZE.max(Timings::PAGE_SIZE),
                    );
                }
                let region = &mut emu.arm9.cp15.pu_regions[region_i as usize];
                let prev_setting = region.control;
                region.control = new_setting;
                region.bounds = new_setting.bounds();
                if emu.arm9.cp15.control.pu_enabled() && new_setting != prev_setting {
                    region.active = new_setting.enabled();
                    if !region.active {
                        if prev_setting.enabled() {
                            // Unmapping anything requires remapping all lower-priority regions,
                            // but since that will in turn overwrite the settings of higher-priority
                            // regions, everything needs to be remapped
                            Self::remap_all_pu_regions(emu);
                        }
                        return;
                    }
                    if prev_setting.enabled() {
                        // The region bounds changed, remap everything
                        Self::remap_all_pu_regions(emu);
                    } else {
                        // Just enabled, overlay all higher-priority regions
                        #[cfg(feature = "pu-checks")]
                        {
                            for region in &emu.arm9.cp15.pu_regions[region_i as usize..] {
                                if region.active {
                                    emu.arm9
                                        .cp15
                                        .perm_map
                                        .set_range(region.perms, region.bounds);
                                }
                            }
                        }
                        for region in &emu.arm9.cp15.pu_regions[region_i as usize..] {
                            if !region.active {
                                continue;
                            }
                            if region.cache_attrs.code_cache_active() {
                                emu.arm9.cp15.timings.set_cpu_local_range(
                                    map_mask::R_CODE,
                                    Cp15::CODE_CACHE_AVG_TIMING,
                                    region.bounds,
                                );
                            } else {
                                emu.arm9.cp15.timings.unset_cpu_local_range(
                                    map_mask::R_CODE,
                                    region.bounds,
                                    &emu.arm9.bus_timings,
                                );
                            }
                            if emu.arm9.cp15.itcm_mode == TcmMode::Normal {
                                emu.arm9.cp15.timings.set_cpu_local_subrange(
                                    map_mask::R_CODE,
                                    Cycles::repeat(1),
                                    (0, emu.arm9.cp15.itcm_upper_bound),
                                    region.bounds,
                                );
                            }
                            if region.cache_attrs.data_cache_active() {
                                emu.arm9.cp15.timings.set_cpu_local_range(
                                    map_mask::R_DATA,
                                    Cp15::DATA_CACHE_AVG_TIMING,
                                    region.bounds,
                                );
                            } else {
                                emu.arm9.cp15.timings.unset_cpu_local_range(
                                    map_mask::R_DATA,
                                    region.bounds,
                                    &emu.arm9.bus_timings,
                                );
                            }
                            if emu.arm9.cp15.dtcm_mode == TcmMode::Normal {
                                emu.arm9.cp15.timings.set_cpu_local_subrange(
                                    map_mask::R_DATA,
                                    Cycles::repeat(1),
                                    emu.arm9.cp15.dtcm_bounds,
                                    region.bounds,
                                );
                            }
                            if emu.arm9.cp15.itcm_mode == TcmMode::Normal {
                                emu.arm9.cp15.timings.set_cpu_local_subrange(
                                    map_mask::R_DATA,
                                    Cycles::repeat(1),
                                    (0, emu.arm9.cp15.itcm_upper_bound),
                                    region.bounds,
                                );
                            }
                        }
                    }
                }
            }

            // Wait for interrupt
            (7, 0, 4) | (7, 8, 2) => emu.arm9.irqs.halt(&mut emu.arm9.schedule),

            // Cache operations
            (7, _, _) => {
                #[cfg(feature = "log")]
                slog::trace!(
                    emu.arm9.logger,
                    "Unimplemented CP15 cache command reg write @ C7,C{},{}: {:#010X}",
                    cm,
                    opcode_2,
                    value
                );
            }

            // Data cache lockdown
            (9, 0, 0) => {
                emu.arm9.cp15.data_cache_lockdown_control.0 = value & 0x8000_0003;
            }

            // Instruction cache lockdown
            (9, 0, 1) => {
                emu.arm9.cp15.code_cache_lockdown_control.0 = value & 0x8000_0003;
            }

            (9, 1, 0) => Self::set_cp15_dtcm_control(emu, TcmControl(value)), // DTCM base and size
            (9, 1, 1) => Self::set_cp15_itcm_control(emu, TcmControl(value)), // ITCM base and size

            (13, 0..=1, 1) => emu.arm9.cp15.trace_process_id = value, // Trace process identifier

            // TODO?: register 15 (BIST and cache debug)
            _ => {
                #[cfg(feature = "log")]
                slog::warn!(
                    emu.arm9.logger,
                    "Unknown CP15 reg write @ C{},C{},{}: {:#010X}",
                    cn,
                    cm,
                    opcode_2,
                    value
                );
            }
        }
    }
}
