mod io;
mod render;

use super::{Scanline, SCREEN_WIDTH};
use crate::utils::bitfield_debug;
use core::{marker::PhantomData, ops::Range};

pub trait Role {
    const IS_A: bool;
    const BG_VRAM_MASK: u32;
}

pub enum EngineA {}
impl Role for EngineA {
    const IS_A: bool = true;
    const BG_VRAM_MASK: u32 = 0x7_FFFF;
}

pub enum EngineB {}
impl Role for EngineB {
    const IS_A: bool = false;
    const BG_VRAM_MASK: u32 = 0x1_FFFF;
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Control(pub u32) {
        pub bg_mode: u8 @ 0..=2,
        pub bg0_3d: bool @ 3,
        pub obj_tile_1d_mapping: bool @ 4,
        pub bitmap_objs_256x256: bool @ 5,
        pub obj_bitmap_1d_mapping: bool @ 6,
        pub forced_blank: bool @ 7,
        pub bg0_enabled: bool @ 8,
        pub bg1_enabled: bool @ 9,
        pub bg2_enabled: bool @ 10,
        pub bg3_enabled: bool @ 11,
        pub objs_enabled: bool @ 12,
        pub wins_enabled: u8 @ 13..=15,
        pub win01_enabled: u8 @ 13..=14,
        pub obj_win_enabled: bool @ 15,
        pub display_mode_a: u8 @ 16..=17,
        pub display_mode_b: u8 @ 16..=16,
        pub a_vram_bank: u8 @ 18..=19,
        pub obj_tile_1d_boundary: u8 @ 20..=21,
        pub a_obj_bitmap_1d_boundary: u8 @ 22..=22,
        pub hblank_interval_free: bool @ 23,
        pub a_tile_base_raw: u8 @ 24..=26,
        pub a_map_base_raw: u8 @ 27..=29,
        pub bg_ext_pal_enabled: bool @ 30,
        pub obj_ext_pal_enabled: bool @ 31,
    }
}

impl Control {
    #[inline]
    pub fn bg_enabled(self, i: BgIndex) -> bool {
        self.0 & 1 << (8 + i.get()) != 0
    }

    #[inline]
    pub fn a_tile_base(self) -> u32 {
        self.0 >> 8 & 0x7_0000
    }

    #[inline]
    pub fn a_map_base(self) -> u32 {
        self.0 >> 11 & 0x7_0000
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct BrightnessControl(pub u16) {
        pub factor: u8 @ 0..=4,
        pub mode: u8 @ 14..=15,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct BgControl(pub u16) {
        pub priority: u8 @ 0..=1,
        pub use_direct_color_extended_bg: bool @ 2,
        pub tile_base_raw: u8 @ 2..=5,
        pub mosaic: bool @ 6,
        pub use_256_colors: bool @ 7,
        pub use_bitmap_extended_bg: bool @ 7,
        pub map_base_raw: u8 @ 8..=12,
        pub bg01_ext_pal_slot: u8 @ 13..=13,
        pub affine_display_area_overflow: bool @ 13,
        pub size_key: u8 @ 14..=15,
    }
}

impl BgControl {
    #[inline]
    pub fn tile_base(self) -> u32 {
        (self.0 as u32) << 12 & 0x3_C000
    }

    #[inline]
    pub fn map_base(self) -> u32 {
        (self.0 as u32) << 3 & 0xF800
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct ColorEffectsControl(pub u16) {
        pub target_1_mask: u8 @ 0..=5,
        pub color_effect: u8 @ 6..=7,
        pub target_2_mask: u8 @ 8..=13,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct BlendCoeffsRaw(pub u16) {
        pub a_coeff: u8 @ 0..=4,
        pub b_coeff: u8 @ 8..=12,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct OamAttr0(u16) {
        pub y_start: u8 @ 0..=7,
        pub rot_scale: bool @ 8,
        pub double_size: bool @ 9, // Rot/scale sprites
        pub disabled: bool @ 9, // Normal sprites
        pub mode: u8 @ 10..=11,
        pub mosaic_enabled: bool @ 12,
        pub use_256_colors: bool @ 13,
        pub shape: u8 @ 14..=15,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct OamAttr1(u16) {
        pub x_start_raw: u16 @ 0..=8,
        pub rot_scale_params_index: u8 @ 9..=13, // Rot/scale sprite,
        pub x_flip: bool @ 12, // Normal sprites
        pub y_flip: bool @ 13, // Normal sprites
        pub size: u8 @ 14..=15,
    }
}

impl OamAttr1 {
    #[inline]
    pub fn x_start(self) -> i16 {
        (self.0 as i16) << 7 >> 7
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct OamAttr2(u16) {
        pub tile_number: u16 @ 0..=9,
        pub bg_priority: u8 @ 10..=11,
        pub palette_number: u8 @ 12..=15,
    }
}

mod bounded {
    use crate::utils::bounded_int_lit;
    bounded_int_lit!(pub struct BgIndex(u8), max 3);
    bounded_int_lit!(pub struct AffineBgIndex(u8), max 1);

    impl From<BgIndex> for AffineBgIndex {
        fn from(i: BgIndex) -> Self {
            assert!(i.get() & 2 == 0);
            Self::new(i.get() & 1)
        }
    }

    impl From<AffineBgIndex> for BgIndex {
        fn from(i: AffineBgIndex) -> Self {
            Self::new(i.get() | 2)
        }
    }
}
pub use bounded::{AffineBgIndex, BgIndex};

#[derive(Clone, Copy, Debug)]
pub struct Bg {
    control: BgControl,
    priority: u8,
    pub scroll: [u16; 2],
}

#[allow(clippy::trivially_copy_pass_by_ref)]
impl Bg {
    #[inline]
    pub fn control(&self) -> BgControl {
        self.control
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AffineBgData {
    ref_points: [i32; 2],
    pub params: [i16; 4],
    pos: [i32; 2],
}

impl AffineBgData {
    #[inline]
    pub fn ref_points(&self) -> [i32; 2] {
        self.ref_points
    }

    #[inline]
    pub fn set_ref_points(&mut self, value: [i32; 2]) {
        self.ref_points = value;
        self.pos = value;
    }

    #[inline]
    pub fn pos(&self) -> [i32; 2] {
        self.pos
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct WindowControl(u8) {
        pub bg_obj_mask: u8 @ 0..=4,
        pub color_effects_enabled: bool @ 5,
    }
}

#[derive(Clone, Debug)]
pub struct WindowRanges {
    pub x: Range<u8>,
    pub y: Range<u8>,
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct ObjPixel(u32) {
        pub pal_color: u16 @ 0..=11,
        pub raw_color: u16 @ 0..=15,
        pub priority: u8 @ 16..=18,
        pub alpha: u8 @ 19..=23,
        pub force_blending: bool @ 24,
        pub custom_alpha: bool @ 25,
        pub use_raw_color: bool @ 26,
        pub use_ext_pal: bool @ 27,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct BgObjPixel(u32) {
        pub rgb: u32 @ 0..=17,
        pub alpha: u8 @ 19..=23,
        pub force_blending: bool @ 24,
        pub custom_alpha: bool @ 25,
        pub color_effects_mask: u8 @ 26..=31,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct WindowPixel(u8) {
        pub bg_obj_mask: u8 @ 0..=4,
        pub color_effects_enabled: bool @ 5,
    }
}

pub struct Engine2d<R: Role> {
    #[cfg(feature = "log")]
    logger: slog::Logger,
    _role: PhantomData<R>,
    render_fns: render::FnPtrs<R>,
    pub(super) enabled: bool,
    pub(super) engine_3d_enabled: bool,
    engine_3d_enabled_in_frame: bool,
    control: Control,
    master_brightness_control: BrightnessControl,
    master_brightness_factor: u32,
    pub bgs: [Bg; 4],
    pub affine_bg_data: [AffineBgData; 2],
    pub window_ranges: [WindowRanges; 2],
    pub window_control: [WindowControl; 4],
    color_effects_control: ColorEffectsControl,
    blend_coeffs_raw: BlendCoeffsRaw,
    blend_coeffs: (u8, u8),
    brightness_coeff: u8,
    windows_active: [bool; 2],
    obj_window: [u8; SCREEN_WIDTH / 8],
    bg_obj_scanline: Scanline<u64>,
    obj_scanline: Scanline<ObjPixel>,
    // Allow for slightly out-of-bounds SIMD accesses
    window: Scanline<WindowPixel, { SCREEN_WIDTH + 7 }>,
}

impl<R: Role> Engine2d<R> {
    pub(super) fn new(#[cfg(feature = "log")] logger: slog::Logger) -> Self {
        Engine2d {
            #[cfg(feature = "log")]
            logger,
            _role: PhantomData,
            render_fns: render::FnPtrs::new(),
            enabled: false,
            engine_3d_enabled: false,
            engine_3d_enabled_in_frame: false,
            control: Control(0),
            master_brightness_control: BrightnessControl(0),
            master_brightness_factor: 0,
            bgs: [Bg {
                control: BgControl(0),
                scroll: [0; 2],
                priority: 4,
            }; 4],
            affine_bg_data: [AffineBgData {
                ref_points: [0; 2],
                params: [0; 4],
                pos: [0; 2],
            }; 2],
            window_ranges: [
                WindowRanges { x: 0..0, y: 0..0 },
                WindowRanges { x: 0..0, y: 0..0 },
            ],
            window_control: [
                WindowControl(0),
                WindowControl(0),
                WindowControl(0x3F),
                WindowControl(0),
            ],
            color_effects_control: ColorEffectsControl(0),
            blend_coeffs_raw: BlendCoeffsRaw(0),
            blend_coeffs: (0, 0),
            brightness_coeff: 0,
            windows_active: [false; 2],
            obj_window: [0; SCREEN_WIDTH / 8],
            bg_obj_scanline: Scanline([0; SCREEN_WIDTH]),
            obj_scanline: Scanline([ObjPixel(0); SCREEN_WIDTH]),
            window: Scanline([WindowPixel(0); SCREEN_WIDTH + 7]),
        }
    }

    #[inline]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    #[inline]
    pub fn set_control(&mut self, value: Control) {
        if R::IS_A {
            self.control = value;
        } else {
            // TODO: Check whether all unused bits are masked out for engine B
            self.control.0 = value.0 & 0xC0B3_FFF7;
        }
        for i in 0..4 {
            let bg = &mut self.bgs[i];
            bg.priority = if value.bg_enabled(BgIndex::new(i as u8)) {
                bg.control.priority()
            } else {
                4
            };
        }
    }

    #[inline]
    pub fn master_brightness_control(&self) -> BrightnessControl {
        self.master_brightness_control
    }

    #[inline]
    pub fn set_master_brightness_control(&mut self, value: BrightnessControl) {
        self.master_brightness_control.0 = value.0 & 0xC01F;
        self.master_brightness_factor = (value.factor() as u32).min(16);
    }

    #[inline]
    pub fn set_bg_control(&mut self, i: BgIndex, value: BgControl) {
        let bg = &mut self.bgs[i.get() as usize];
        bg.control = value;
        if bg.priority != 4 {
            bg.priority = value.priority();
        }
    }

    #[inline]
    pub fn window_control(&self, i: usize) -> WindowControl {
        self.window_control[i]
    }

    #[inline]
    pub fn set_window_control(&mut self, i: usize, value: WindowControl) {
        self.window_control[i].0 = value.0 & 0x3F;
    }

    #[inline]
    pub fn color_effects_control(&self) -> ColorEffectsControl {
        self.color_effects_control
    }

    #[inline]
    pub fn set_color_effects_control(&mut self, value: ColorEffectsControl) {
        self.color_effects_control.0 = value.0 & 0x3FFF;
    }

    #[inline]
    pub fn blend_coeffs_raw(&self) -> BlendCoeffsRaw {
        self.blend_coeffs_raw
    }

    #[inline]
    pub fn set_blend_coeffs_raw(&mut self, value: BlendCoeffsRaw) {
        self.blend_coeffs_raw.0 = value.0 & 0x1F1F;
        self.blend_coeffs = (
            self.blend_coeffs_raw.a_coeff().min(16),
            self.blend_coeffs_raw.b_coeff().min(16),
        );
    }

    #[inline]
    pub fn blend_coeffs(&self) -> (u8, u8) {
        self.blend_coeffs
    }

    #[inline]
    pub fn set_blend_coeffs(&mut self, value: (u8, u8)) {
        self.blend_coeffs = value;
        self.blend_coeffs_raw = self
            .blend_coeffs_raw
            .with_a_coeff(value.0)
            .with_b_coeff(value.1);
    }

    #[inline]
    pub fn brightness_coeff(&self) -> u8 {
        self.brightness_coeff
    }

    #[inline]
    pub fn set_brightness_coeff(&mut self, value: u8) {
        self.brightness_coeff = (value & 0x1F).min(value);
    }

    pub(super) fn end_vblank(&mut self) {
        if R::IS_A {
            self.engine_3d_enabled_in_frame = self.engine_3d_enabled;
        }
        // TODO: When does this happen? This is just what might make the most sense, but GBATEK
        // doesn't say.
        for affine_bg in &mut self.affine_bg_data {
            affine_bg.pos = affine_bg.ref_points;
        }
    }
}
