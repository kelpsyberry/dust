use crate::{BgObjPixel, Buffers, RenderingData};
use dust_core::gpu::{Scanline, SCREEN_WIDTH};

pub fn apply_color_effects<B: Buffers, D: RenderingData, const EFFECT: u8>(
    buffers: &mut B,
    data: &D,
) {
    #[inline]
    fn blend(pixel: BgObjPixel, coeff_a: u32, coeff_b: u32) -> u64 {
        let top = pixel.0 as u32;
        let bot = (pixel.0 >> 32) as u32;
        let r = ((top & 0x3F) * coeff_a + (bot & 0x3F) * coeff_b).min(0x3F0);
        let g = ((top & 0xFC0) * coeff_a + (bot & 0xFC0) * coeff_b).min(0xFC00) & 0xFC00;
        let b =
            ((top & 0x3_F000) * coeff_a + (bot & 0x3_F000) * coeff_b).min(0x3F_0000) & 0x3F_0000;
        ((r | g | b) >> 4) as u64
    }

    #[inline]
    fn blend_5bit_coeff(pixel: BgObjPixel, coeff_a: u32, coeff_b: u32) -> u64 {
        let top = pixel.0 as u32;
        let bot = (pixel.0 >> 32) as u32;
        let r = ((top & 0x3F) * coeff_a + (bot & 0x3F) * coeff_b).min(0x7E0);
        let g = ((top & 0xFC0) * coeff_a + (bot & 0xFC0) * coeff_b).min(0x1F800) & 0x1F800;
        let b =
            ((top & 0x3_F000) * coeff_a + (bot & 0x3_F000) * coeff_b).min(0x7E_0000) & 0x7E_0000;
        ((r | g | b) >> 5) as u64
    }

    let color_effects_control = data.color_effects_control();
    let blend_coeffs = data.blend_coeffs();
    let target_1_mask = color_effects_control.target_1_mask();
    let target_2_mask = color_effects_control.target_2_mask();
    let coeff_a = blend_coeffs.0 as u32;
    let coeff_b = blend_coeffs.1 as u32;
    let brightness_coeff = data.brightness_coeff() as u32;

    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

    for i in 0..SCREEN_WIDTH {
        let pixel = scanline.0[i];
        scanline.0[i].0 = if window.0[i].color_effects_enabled() {
            let top_mask = pixel.color_effects_mask();
            let bot_matches = pixel.bot_color_effects_mask() & target_2_mask != 0;
            if pixel.is_3d() && bot_matches {
                let a_coeff = (pixel.alpha() + 1) as u32;
                let b_coeff = 32 - a_coeff;
                blend_5bit_coeff(pixel, a_coeff, b_coeff)
            } else if pixel.force_blending() && bot_matches {
                let (a_coeff, b_coeff) = if pixel.custom_alpha() {
                    (pixel.alpha() as u32, 16 - pixel.alpha() as u32)
                } else {
                    (coeff_a, coeff_b)
                };
                blend(pixel, a_coeff, b_coeff)
            } else if EFFECT != 0 && top_mask & target_1_mask != 0 {
                match EFFECT {
                    1 => {
                        if bot_matches {
                            blend(pixel, coeff_a, coeff_b)
                        } else {
                            pixel.0
                        }
                    }

                    2 => {
                        let pixel = pixel.0 as u32;
                        let increment = {
                            let complement = 0x3_FFFF ^ pixel;
                            ((((complement & 0x3_F03F) * brightness_coeff) & 0x3F_03F0)
                                | (((complement & 0xFC0) * brightness_coeff) & 0xFC00))
                                >> 4
                        };
                        (pixel + increment) as u64
                    }

                    _ => {
                        let pixel = pixel.0 as u32;
                        let decrement = {
                            ((((pixel & 0x3_F03F) * brightness_coeff) & 0x3F_03F0)
                                | (((pixel & 0xFC0) * brightness_coeff) & 0xFC00))
                                >> 4
                        };
                        (pixel - decrement) as u64
                    }
                }
            } else {
                pixel.0
            }
        } else {
            pixel.0
        };
    }
}

#[inline]
fn rgb6_to_rgba8(value: u32) -> u32 {
    let rgb6_8 = (value & 0x3F) | (value << 2 & 0x3F00) | (value << 4 & 0x3F_0000);
    0xFF00_0000 | rgb6_8 << 2 | (rgb6_8 >> 4 & 0x0003_0303)
}

pub fn apply_brightness<D: RenderingData>(scanline_buffer: &mut Scanline<u32>, data: &D) {
    let brightness_factor = data.master_brightness_factor();
    match data.master_brightness_control().mode() {
        1 if brightness_factor != 0 => {
            for pixel in &mut scanline_buffer.0 {
                let increment = {
                    let complement = 0x3_FFFF ^ *pixel;
                    ((((complement & 0x3_F03F) * brightness_factor) & 0x3F_03F0)
                        | (((complement & 0xFC0) * brightness_factor) & 0xFC00))
                        >> 4
                };
                *pixel = rgb6_to_rgba8(*pixel + increment);
            }
        }

        2 if brightness_factor != 0 => {
            for pixel in &mut scanline_buffer.0 {
                let decrement = {
                    ((((*pixel & 0x3_F03F) * brightness_factor) & 0x3F_03F0)
                        | (((*pixel & 0xFC0) * brightness_factor) & 0xFC00))
                        >> 4
                };
                *pixel = rgb6_to_rgba8(*pixel - decrement);
            }
        }

        _ => {
            for pixel in &mut scanline_buffer.0 {
                *pixel = rgb6_to_rgba8(*pixel);
            }
        }
    }
}
