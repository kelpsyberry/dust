mod io;
mod renderer;
pub use renderer::Renderer;

use crate::utils::{LoadableInPlace, Savestate, Storable};
use core::marker::PhantomData;

pub trait Role: LoadableInPlace + Storable {
    const IS_A: bool;
    const BG_VRAM_MASK: u32;
}

pub enum EngineA {}
impl Role for EngineA {
    const IS_A: bool = true;
    const BG_VRAM_MASK: u32 = 0x7_FFFF;
}

impl LoadableInPlace for EngineA {
    fn load_in_place<S: emu_utils::ReadSavestate>(
        &mut self,
        _save: &mut S,
    ) -> Result<(), S::Error> {
        Ok(())
    }
}

impl Storable for EngineA {
    fn store<S: emu_utils::WriteSavestate>(&mut self, _save: &mut S) -> Result<(), S::Error> {
        Ok(())
    }
}

pub enum EngineB {}
impl Role for EngineB {
    const IS_A: bool = false;
    const BG_VRAM_MASK: u32 = 0x1_FFFF;
}

impl LoadableInPlace for EngineB {
    fn load_in_place<S: emu_utils::ReadSavestate>(
        &mut self,
        _save: &mut S,
    ) -> Result<(), S::Error> {
        Ok(())
    }
}

impl Storable for EngineB {
    fn store<S: emu_utils::WriteSavestate>(&mut self, _save: &mut S) -> Result<(), S::Error> {
        Ok(())
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct Control(pub u32): Debug {
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

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct BrightnessControl(pub u16): Debug {
        pub factor: u8 @ 0..=4,
        pub mode: u8 @ 14..=15,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct BgControl(pub u16): Debug {
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

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct ColorEffectsControl(pub u16): Debug {
        pub target_1_mask: u8 @ 0..=5,
        pub color_effect: u8 @ 6..=7,
        pub target_2_mask: u8 @ 8..=13,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct BlendCoeffsRaw(pub u16): Debug {
        pub a_coeff: u8 @ 0..=4,
        pub b_coeff: u8 @ 8..=12,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub const struct OamAttr0(pub u16): Debug {
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

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub const struct OamAttr1(pub u16): Debug {
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

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub const struct OamAttr2(pub u16): Debug {
        pub tile_number: u16 @ 0..=9,
        pub bg_priority: u8 @ 10..=11,
        pub palette_number: u8 @ 12..=15,
    }
}

mod bounded {
    use crate::utils::bounded_int_lit;
    bounded_int_lit!(pub struct BgIndex(u8), max 3);
    bounded_int_lit!(pub struct AffineBgIndex(u8), max 1);
    bounded_int_lit!(pub struct WindowIndex(u8), max 1);
    bounded_int_lit!(pub struct WindowControlIndex(u8), max 3);
}
pub use bounded::{AffineBgIndex, BgIndex, WindowControlIndex, WindowIndex};

#[derive(Clone, Debug, Savestate)]
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

    #[inline]
    pub fn write_control(&mut self, value: BgControl) {
        self.control = value;
        if self.priority != 4 {
            self.priority = value.priority();
        }
    }

    #[inline]
    pub fn priority(&self) -> u8 {
        self.priority
    }
}

#[derive(Clone, Debug, Savestate)]
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
    pub fn write_ref_points(&mut self, value: [i32; 2]) {
        self.ref_points = value;
        self.pos = value;
    }

    #[inline]
    pub fn pos(&self) -> [i32; 2] {
        self.pos
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct WindowControl(pub u8): Debug {
        pub bg_obj_mask: u8 @ 0..=4,
        pub color_effects_enabled: bool @ 5,
    }
}

#[derive(Clone, Copy, Debug, Savestate)]
pub struct WindowRanges {
    pub x: (u8, u8),
    pub y: (u8, u8),
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct CaptureControl(pub u32): Debug {
        pub factor_a: u8 @ 0..=4,
        pub factor_b: u8 @ 8..=12,
        pub dst_bank: u8 @ 16..=17,
        pub dst_offset_raw: u8 @ 18..=19,
        pub size: u8 @ 20..=21,
        pub src_a_3d_only: bool @ 24,
        pub src_b_display_fifo: bool @ 25,
        pub src_b_vram_offset_raw: u8 @ 26..=27,
        pub src: u8 @ 29..=30,
        pub enabled: bool @ 31,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct WindowsActive(pub u8): Debug {
        pub win0: bool @ 0,
        pub win1: bool @ 1,
    }
}

#[derive(Savestate)]
pub struct Data {
    pub(super) is_enabled: bool,
    pub(super) engine_3d_enabled: bool,
    engine_3d_enabled_in_frame: bool,
    control: Control,
    master_brightness_control: BrightnessControl,
    master_brightness_factor: u32,
    bgs: [Bg; 4],
    affine_bg_data: [AffineBgData; 2],
    window_ranges: [WindowRanges; 2],
    window_control: [WindowControl; 4],
    color_effects_control: ColorEffectsControl,
    blend_coeffs_raw: BlendCoeffsRaw,
    blend_coeffs: (u8, u8),
    brightness_coeff: u8,
    windows_active: WindowsActive,
    capture_control: CaptureControl,
    capture_enabled_in_frame: bool,
    capture_height: u8,
}

#[derive(Savestate)]
#[load(in_place_only, post = "self.post_load()")]
pub struct Engine2d<R: Role> {
    #[cfg(feature = "log")]
    #[savestate(skip)]
    logger: slog::Logger,
    #[savestate(skip)]
    _role: PhantomData<R>,
    #[savestate(skip)]
    pub renderer: Box<dyn Renderer>,
    pub(super) data: Data,
}

impl<R: Role> Engine2d<R> {
    pub(super) fn new(
        renderer: Box<dyn Renderer>,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Self {
        const BG: Bg = Bg {
            control: BgControl(0),
            scroll: [0; 2],
            priority: 4,
        };
        const AFFINE_BG_DATA: AffineBgData = AffineBgData {
            ref_points: [0; 2],
            params: [0; 4],
            pos: [0; 2],
        };
        Engine2d {
            #[cfg(feature = "log")]
            logger,
            _role: PhantomData,
            renderer,
            data: Data {
                is_enabled: false,
                engine_3d_enabled: false,
                engine_3d_enabled_in_frame: false,
                control: Control(0),
                master_brightness_control: BrightnessControl(0),
                master_brightness_factor: 0,
                bgs: [BG; 4],
                affine_bg_data: [AFFINE_BG_DATA; 2],
                window_ranges: [
                    WindowRanges {
                        x: (0, 0),
                        y: (0, 0),
                    },
                    WindowRanges {
                        x: (0, 0),
                        y: (0, 0),
                    },
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
                windows_active: WindowsActive(0),
                capture_control: CaptureControl(0),
                capture_enabled_in_frame: false,
                capture_height: 128,
            },
        }
    }

    fn post_load(&mut self) {
        self.renderer.post_load(&self.data);
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.data.is_enabled
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.data.control
    }

    #[inline]
    pub fn write_control(&mut self, value: Control) {
        if R::IS_A {
            self.data.control = value;
        } else {
            // TODO: Check whether all unused bits are masked out for engine B
            self.data.control.0 = value.0 & 0xC0B3_FFF7;
        }
        for i in 0..4 {
            let bg = &mut self.data.bgs[i];
            bg.priority = if value.bg_enabled(BgIndex::new(i as u8)) {
                bg.control.priority()
            } else {
                4
            };
        }
    }

    #[inline]
    pub fn master_brightness_control(&self) -> BrightnessControl {
        self.data.master_brightness_control
    }

    #[inline]
    pub fn write_master_brightness_control(&mut self, value: BrightnessControl) {
        self.data.master_brightness_control.0 = value.0 & 0xC01F;
        self.data.master_brightness_factor = (value.factor() as u32).min(16);
    }

    #[inline]
    pub fn bg(&self, i: BgIndex) -> &Bg {
        &self.data.bgs[i.get() as usize]
    }

    #[inline]
    pub fn bg_mut(&mut self, i: BgIndex) -> &mut Bg {
        &mut self.data.bgs[i.get() as usize]
    }

    #[inline]
    pub fn affine_bg_data(&self, i: AffineBgIndex) -> &AffineBgData {
        &self.data.affine_bg_data[i.get() as usize]
    }

    #[inline]
    pub fn affine_bg_data_mut(&mut self, i: AffineBgIndex) -> &mut AffineBgData {
        &mut self.data.affine_bg_data[i.get() as usize]
    }

    #[inline]
    pub fn window_ranges(&self, i: WindowIndex) -> WindowRanges {
        self.data.window_ranges[i.get() as usize]
    }

    #[inline]
    pub fn window_ranges_mut(&mut self, i: WindowIndex) -> &mut WindowRanges {
        &mut self.data.window_ranges[i.get() as usize]
    }

    #[inline]
    pub fn window_control(&self, i: WindowControlIndex) -> WindowControl {
        self.data.window_control[i.get() as usize]
    }

    #[inline]
    pub fn write_window_control(&mut self, i: WindowControlIndex, value: WindowControl) {
        self.data.window_control[i.get() as usize].0 = value.0 & 0x3F;
    }

    #[inline]
    pub fn color_effects_control(&self) -> ColorEffectsControl {
        self.data.color_effects_control
    }

    #[inline]
    pub fn write_color_effects_control(&mut self, value: ColorEffectsControl) {
        self.data.color_effects_control.0 = value.0 & 0x3FFF;
        self.renderer.update_color_effects_control(value);
    }

    #[inline]
    pub fn blend_coeffs_raw(&self) -> BlendCoeffsRaw {
        self.data.blend_coeffs_raw
    }

    #[inline]
    pub fn write_blend_coeffs_raw(&mut self, value: BlendCoeffsRaw) {
        self.data.blend_coeffs_raw.0 = value.0 & 0x1F1F;
        self.data.blend_coeffs = (
            self.data.blend_coeffs_raw.a_coeff().min(16),
            self.data.blend_coeffs_raw.b_coeff().min(16),
        );
    }

    #[inline]
    pub fn brightness_coeff(&self) -> u8 {
        self.data.brightness_coeff
    }

    #[inline]
    pub fn write_brightness_coeff(&mut self, value: u8) {
        self.data.brightness_coeff = (value & 0x1F).min(16);
    }

    #[inline]
    pub fn windows_active(&self) -> WindowsActive {
        self.data.windows_active
    }

    #[inline]
    pub fn capture_control(&self) -> CaptureControl {
        self.data.capture_control
    }

    #[inline]
    pub fn write_capture_control(&mut self, value: CaptureControl) {
        if R::IS_A {
            self.data.capture_control.0 = value.0 & 0xEF3F_1F1F;
            self.data.capture_height = [128, 64, 128, 192][value.size() as usize];
        }
    }

    pub(super) fn start_vblank(&mut self) {
        if R::IS_A && self.data.capture_enabled_in_frame {
            self.data.capture_control.set_enabled(false);
        }
    }

    pub(super) fn end_vblank(&mut self) {
        if R::IS_A {
            self.data.engine_3d_enabled_in_frame = self.data.engine_3d_enabled;
            self.data.capture_enabled_in_frame = self.data.capture_control.enabled();
        }
        // TODO: When does this happen? This is just what might make the most sense, but GBATEK
        // doesn't say.
        for affine_bg in &mut self.data.affine_bg_data {
            affine_bg.pos = affine_bg.ref_points;
        }
    }

    pub(super) fn update_windows(&mut self, vcount: u16) {
        for i in 0..2 {
            let mask = 1 << i;

            if self.data.control.win01_enabled() & mask == 0 {
                self.data.windows_active.0 &= !mask;
                continue;
            }

            let y_range = &self.data.window_ranges[i].y;
            let y_start = y_range.0;
            let mut y_end = y_range.1;
            if y_end < y_start {
                y_end = 192;
            }
            if vcount as u8 == y_start {
                self.data.windows_active.0 |= mask;
            }
            if vcount as u8 == y_end {
                self.data.windows_active.0 &= !mask;
            }
        }
    }
}
