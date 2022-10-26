#![feature(
    maybe_uninit_uninit_array,
    maybe_uninit_slice,
    const_mut_refs,
    const_trait_impl,
    generic_const_exprs,
    new_uninit,
    portable_simd
)]
#![allow(incomplete_features)]

pub mod capture;
mod impls;
pub mod render;

use dust_core::{
    gpu::{
        engine_2d::{
            AffineBgIndex, BgControl, BgIndex, BrightnessControl, ColorEffectsControl, Control,
            Role,
        },
        Scanline, SCREEN_WIDTH,
    },
    utils::Bytes,
};

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub const struct ObjPixel(pub u32): Debug {
        pub pal_color: u16 @ 0..=11,
        pub raw_color: u16 @ 0..=15,
        pub use_raw_color: bool @ 16,
        pub use_ext_pal: bool @ 17,

        pub alpha: u8 @ 18..=22,
        pub force_blending: bool @ 24,
        pub custom_alpha: bool @ 25,

        pub priority: u8 @ 26..=28,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub const struct BgObjPixel(pub u64): Debug {
        pub rgb: u32 @ 0..=17,

        pub alpha: u8 @ 18..=22,
        pub is_3d: bool @ 23,
        pub force_blending: bool @ 24,
        pub custom_alpha: bool @ 25,

        pub color_effects_mask: u8 @ 26..=31,

        pub bot_rgb: u32 @ 32..=49,

        pub bot_alpha: u8 @ 50..=54,
        pub bot_is_3d: bool @ 55,
        pub bot_force_blending: bool @ 56,
        pub bot_custom_alpha: bool @ 57,

        pub bot_color_effects_mask: u8 @ 58..=63,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub const struct WindowPixel(pub u8): Debug {
        pub bg_obj_mask: u8 @ 0..=4,
        pub color_effects_enabled: bool @ 5,
    }
}

#[inline]
pub const fn rgb5_to_rgb6(value: u16) -> u32 {
    let value = value as u32;
    (value << 1 & 0x3E) | (value << 2 & 0xF80) | (value << 3 & 0x3_E000)
}

#[inline]
pub const fn rgb5_to_rgb6_64(value: u16) -> u64 {
    let value = value as u64;
    (value << 1 & 0x3E) | (value << 2 & 0xF80) | (value << 3 & 0x3_E000)
}

#[allow(clippy::mut_from_ref, clippy::missing_safety_doc)]
pub trait Buffers {
    unsafe fn obj_window(&self) -> &mut [u8; SCREEN_WIDTH / 8];
    unsafe fn obj_scanline(&self) -> &mut Scanline<ObjPixel>;
    unsafe fn window(&self) -> &mut Scanline<WindowPixel, { SCREEN_WIDTH + 7 }>;
    unsafe fn bg_obj_scanline(&self) -> &mut Scanline<BgObjPixel>;
}

#[allow(clippy::mut_from_ref)]
pub trait Vram<R: Role> {
    fn bg(&self) -> &Bytes<{ R::BG_VRAM_LEN }>;
    fn obj(&self) -> &Bytes<{ R::OBJ_VRAM_LEN }>;
    fn bg_palette(&self) -> &Bytes<0x206>;
    fn obj_palette(&self) -> &Bytes<0x206>;
    fn bg_ext_palette(&self) -> &Bytes<0x8006>;
    fn obj_ext_palette(&self) -> &Bytes<0x2006>;
    fn oam(&self) -> &Bytes<0x400>;
}

#[allow(clippy::mut_from_ref)]
pub trait RenderingData {
    fn control(&self) -> Control;

    fn master_brightness_control(&self) -> BrightnessControl;
    fn master_brightness_factor(&self) -> u32;

    fn bg_control(&self, i: BgIndex) -> BgControl;
    fn bg_priority(&self, i: BgIndex) -> u8;
    fn bg_scroll(&self, i: BgIndex) -> [u16; 2];

    fn affine_bg_x_incr(&self, i: AffineBgIndex) -> [i16; 2];
    fn affine_bg_pos(&self, i: AffineBgIndex) -> [i32; 2];
    fn increase_affine_bg_pos(&mut self, i: AffineBgIndex);

    fn color_effects_control(&self) -> ColorEffectsControl;
    fn blend_coeffs(&self) -> (u8, u8);
    fn brightness_coeff(&self) -> u8;

    fn engine_3d_enabled_in_frame(&self) -> bool;
}
