pub mod gfx;
pub mod render;

pub use dust_soft_2d_base::*;

use core::marker::PhantomData;
use dust_core::gpu::engine_2d::{BrightnessControl, ColorEffectsControl, Role};
use render::{bgs, effects};

#[allow(clippy::type_complexity)]
pub struct FnPtrs<R: Role, B: Buffers, D: RenderingData, V: Vram<R>> {
    pub apply_color_effects: [unsafe fn(&mut B, &D); 4],
    pub render_scanline_bgs_and_objs: [unsafe fn(&mut B, vcount: u8, &mut D, &V); 8],
    _marker: PhantomData<R>,
}

impl<R: Role, B: Buffers, D: RenderingData, V: Vram<R>> FnPtrs<R, B, D, V> {
    #[allow(unused_labels)]
    pub fn new() -> Self
    where
        [(); R::BG_VRAM_LEN]: Sized,
    {
        FnPtrs {
            apply_color_effects: [
                render_fn_ptr!(effects::apply_color_effects::<B, D, 0>),
                render_fn_ptr!(effects::apply_color_effects::<B, D, 1>),
                render_fn_ptr!(effects::apply_color_effects::<B, D, 2>),
                render_fn_ptr!(effects::apply_color_effects::<B, D, 3>),
            ],
            render_scanline_bgs_and_objs: [
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 0>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 1>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 2>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 3>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 4>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 5>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 6>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 7>),
            ],
            _marker: ::core::marker::PhantomData,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct ScanlineFlags {
    pub master_brightness_control: u32,
    pub color_effects_control: u32,
    pub blend_coeffs: u32,
    pub brightness_coeff: u32,
}

impl ScanlineFlags {
    pub fn master_brightness_only(master_brightness_control: BrightnessControl) -> Self {
        ScanlineFlags {
            master_brightness_control: master_brightness_control
                .with_factor(master_brightness_control.factor().min(16))
                .0 as u32,
            color_effects_control: 0,
            blend_coeffs: 0,
            brightness_coeff: 0,
        }
    }

    pub fn new(
        master_brightness_control: BrightnessControl,
        color_effects_control: ColorEffectsControl,
        blend_coeffs: (u8, u8),
        brightness_coeff: u8,
    ) -> Self {
        ScanlineFlags {
            color_effects_control: color_effects_control.0 as u32,
            blend_coeffs: blend_coeffs.0 as u32 | (blend_coeffs.1 as u32) << 16,
            brightness_coeff: brightness_coeff as u32,
            ..Self::master_brightness_only(master_brightness_control)
        }
    }
}
