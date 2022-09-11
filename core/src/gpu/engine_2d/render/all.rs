use super::common::{read_bg_text_tiles, TextTiles};
use super::*;

pub fn apply_color_effects<R: Role, const EFFECT: u8>(engine: &mut Engine2d<R>) {
    #[inline]
    fn blend(pixel: u64, coeff_a: u32, coeff_b: u32) -> u32 {
        let top = pixel as u32;
        let bot = (pixel >> 32) as u32;
        let r = ((top & 0x3F) * coeff_a + (bot & 0x3F) * coeff_b).min(0x3F0);
        let g = ((top & 0xFC0) * coeff_a + (bot & 0xFC0) * coeff_b).min(0xFC00) & 0xFC00;
        let b =
            ((top & 0x3_F000) * coeff_a + (bot & 0x3_F000) * coeff_b).min(0x3F_0000) & 0x3F_0000;
        (r | g | b) >> 4
    }

    #[inline]
    fn blend_5bit_coeff(pixel: u64, coeff_a: u32, coeff_b: u32) -> u32 {
        let top = pixel as u32;
        let bot = (pixel >> 32) as u32;
        let r = ((top & 0x3F) * coeff_a + (bot & 0x3F) * coeff_b).min(0x7E0);
        let g = ((top & 0xFC0) * coeff_a + (bot & 0xFC0) * coeff_b).min(0x1F800) & 0x1F800;
        let b =
            ((top & 0x3_F000) * coeff_a + (bot & 0x3_F000) * coeff_b).min(0x7E_0000) & 0x7E_0000;
        (r | g | b) >> 5
    }

    let target_1_mask = engine.color_effects_control.target_1_mask();
    let target_2_mask = engine.color_effects_control.target_2_mask();
    let coeff_a = engine.blend_coeffs.0 as u32;
    let coeff_b = engine.blend_coeffs.1 as u32;
    let brightness_coeff = engine.brightness_coeff as u32;
    for i in 0..SCREEN_WIDTH {
        let pixel = engine.bg_obj_scanline.0[i];
        let top = BgObjPixel(pixel as u32);
        engine.bg_obj_scanline.0[i] = if engine.window.0[i].color_effects_enabled() {
            let top_mask = top.color_effects_mask();
            let bot_matches = (pixel >> 58) as u8 & target_2_mask != 0;
            if top.is_3d() && bot_matches {
                let a_coeff = (top.alpha() + 1) as u32;
                let b_coeff = (32 - a_coeff) as u32;
                blend_5bit_coeff(pixel, a_coeff, b_coeff)
            } else if top.force_blending() && bot_matches {
                let (a_coeff, b_coeff) = if top.custom_alpha() {
                    (top.alpha() as u32, 16 - top.alpha() as u32)
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
                            top.0
                        }
                    }

                    2 => {
                        let increment = {
                            let complement = 0x3_FFFF ^ top.0;
                            ((((complement & 0x3_F03F) * brightness_coeff) & 0x3F_03F0)
                                | (((complement & 0xFC0) * brightness_coeff) & 0xFC00))
                                >> 4
                        };
                        top.0 + increment
                    }

                    _ => {
                        let decrement = {
                            ((((top.0 & 0x3_F03F) * brightness_coeff) & 0x3F_03F0)
                                | (((top.0 & 0xFC0) * brightness_coeff) & 0xFC00))
                                >> 4
                        };
                        top.0 - decrement
                    }
                }
            } else {
                top.0
            }
        } else {
            top.0
        } as u64;
    }
}

pub fn render_scanline_bg_text<R: Role>(
    engine: &mut Engine2d<R>,
    bg_index: BgIndex,
    line: u8,
    vram: &Vram,
) {
    let bg = &engine.bgs[bg_index.get() as usize];

    let x_start = bg.scroll[0] as u32;
    let y = bg.scroll[1] as u32 + line as u32;

    let mut tiles = TextTiles::new_uninit();
    let tiles = read_bg_text_tiles(engine, &mut tiles, bg.control, y, vram);

    let tile_base = if R::IS_A {
        engine.control.a_tile_base() + bg.control.tile_base()
    } else {
        bg.control.tile_base()
    };

    let bg_mask = 1 << bg_index.get();
    let pixel_attrs = BgObjPixel(0).with_color_effects_mask(bg_mask);

    let tile_off_mask = tiles.len() - 1;
    let y_in_tile = y & 7;
    let mut pal_base = 0;
    let mut x = x_start;

    if bg.control.use_256_colors() {
        let (palette, pal_base_mask) = if engine.control.bg_ext_pal_enabled() {
            let slot = bg_index.get()
                | if bg_index.get() < 2 {
                    bg.control.bg01_ext_pal_slot() << 1
                } else {
                    0
                };
            (
                unsafe {
                    if R::IS_A {
                        vram.a_bg_ext_pal.as_ptr()
                    } else {
                        vram.b_bg_ext_pal_ptr
                    }
                    .add((slot as usize) << 13) as *const u16
                },
                0xF,
            )
        } else {
            (
                unsafe { vram.palette.as_ptr().add((!R::IS_A as usize) << 10) as *const u16 },
                0,
            )
        };

        let mut pixels = 0;

        macro_rules! read_pixels {
            () => {
                let tile = unsafe { *tiles.get_unchecked(x as usize >> 3 & tile_off_mask) };
                #[cfg(target_endian = "big")]
                {
                    tile = tile.swap_bytes();
                }
                let y_in_tile = if tile & 1 << 11 == 0 {
                    y_in_tile
                } else {
                    7 ^ y_in_tile
                };
                let tile_base = tile_base + ((tile as u32 & 0x3FF) << 6 | y_in_tile << 3);
                pal_base = ((tile >> 12 & pal_base_mask) << 8) as usize;
                pixels = if R::IS_A {
                    vram.read_a_bg::<u64>(tile_base)
                } else {
                    vram.read_b_bg::<u64>(tile_base)
                };
                if tile & 1 << 10 != 0 {
                    pixels = pixels.swap_bytes();
                }
            };
        }

        if x & 7 != 0 {
            read_pixels!();
        }
        for i in 0..SCREEN_WIDTH {
            if x & 7 == 0 {
                read_pixels!();
            }
            let color_index = pixels.wrapping_shr(x << 3) as u8;
            if color_index != 0 && engine.window.0[i].0 & bg_mask != 0 {
                let color = unsafe { palette.add(pal_base | color_index as usize).read() };
                engine.bg_obj_scanline.0[i] = (engine.bg_obj_scanline.0[i] as u64) << 32
                    | (rgb5_to_rgb6(color as u32) | pixel_attrs.0) as u64;
            }
            x += 1;
        }
    } else {
        let mut pixels = 0;

        macro_rules! read_pixels {
            () => {
                let tile = unsafe { *tiles.get_unchecked(x as usize >> 3 & tile_off_mask) };
                #[cfg(target_endian = "big")]
                {
                    tile = tile.swap_bytes();
                }
                let y_in_tile = if tile & 1 << 11 == 0 {
                    y_in_tile
                } else {
                    7 ^ y_in_tile
                };
                let tile_base = tile_base + ((tile as u32 & 0x3FF) << 5 | y_in_tile << 2);
                pal_base = tile as usize >> 12 << 5;
                pixels = if R::IS_A {
                    vram.read_a_bg::<u32>(tile_base)
                } else {
                    vram.read_b_bg::<u32>(tile_base)
                };
                if tile & 1 << 10 != 0 {
                    pixels = pixels.swap_bytes();
                    pixels = (pixels >> 4 & 0x0F0F_0F0F) | (pixels << 4 & 0xF0F0_F0F0);
                }
            };
        }

        if x & 7 != 0 {
            read_pixels!();
        }
        for i in 0..SCREEN_WIDTH {
            if x & 7 == 0 {
                read_pixels!();
            }
            let color_index = pixels.wrapping_shr(x << 2) & 0xF;
            if color_index != 0 && engine.window.0[i].0 & bg_mask != 0 {
                let color = unsafe {
                    vram.palette.read_le_aligned_unchecked::<u16>(
                        (!R::IS_A as usize) << 10 | pal_base | (color_index as usize) << 1,
                    )
                };
                engine.bg_obj_scanline.0[i] = (engine.bg_obj_scanline.0[i] as u64) << 32
                    | (rgb5_to_rgb6(color as u32) | pixel_attrs.0) as u64;
            }
            x += 1;
        }
    }
}

#[allow(clippy::similar_names)]
pub fn render_scanline_bg_affine<R: Role, const DISPLAY_AREA_OVERFLOW: bool>(
    engine: &mut Engine2d<R>,
    bg_index: AffineBgIndex,
    vram: &Vram,
) {
    let bg_control = engine.bgs[bg_index.get() as usize | 2].control;
    let affine = &mut engine.affine_bg_data[bg_index.get() as usize];

    let map_base = if R::IS_A {
        engine.control.a_map_base() | bg_control.map_base()
    } else {
        bg_control.map_base()
    };
    let tile_base = if R::IS_A {
        engine.control.a_tile_base() + bg_control.tile_base()
    } else {
        bg_control.tile_base()
    };

    let bg_mask = 4 << bg_index.get();
    let pixel_attrs = BgObjPixel(0).with_color_effects_mask(bg_mask);

    let display_area_overflow_mask = !((0x8000 << bg_control.size_key()) - 1);

    let map_row_shift = 4 + bg_control.size_key();
    let pos_map_mask = ((1 << map_row_shift) - 1) << 11;
    let pos_y_to_map_y_shift = 11 - map_row_shift;

    let mut pos = affine.pos;

    for i in 0..SCREEN_WIDTH {
        if engine.window.0[i].0 & bg_mask != 0
            && (DISPLAY_AREA_OVERFLOW || (pos[0] | pos[1]) & display_area_overflow_mask == 0)
        {
            let tile_addr = map_base
                + ((pos[1] as u32 & pos_map_mask) >> pos_y_to_map_y_shift
                    | (pos[0] as u32 & pos_map_mask) >> 11);
            let tile = if R::IS_A {
                vram.read_a_bg::<u8>(tile_addr)
            } else {
                vram.read_b_bg::<u8>(tile_addr)
            };
            let pixel_addr = tile_base
                + ((tile as u32) << 6 | (pos[1] as u32 >> 5 & 0x38) | (pos[0] as u32 >> 8 & 7));
            let color_index = if R::IS_A {
                vram.read_a_bg::<u8>(pixel_addr)
            } else {
                vram.read_b_bg::<u8>(pixel_addr)
            };
            if color_index != 0 {
                let color = unsafe {
                    vram.palette.read_le_aligned_unchecked::<u16>(
                        (!R::IS_A as usize) << 10 | (color_index as usize) << 1,
                    )
                };
                engine.bg_obj_scanline.0[i] = (engine.bg_obj_scanline.0[i] as u64) << 32
                    | (rgb5_to_rgb6(color as u32) | pixel_attrs.0) as u64;
            }
        }

        pos[0] = pos[0].wrapping_add(affine.params[0] as i32);
        pos[1] = pos[1].wrapping_add(affine.params[2] as i32);
    }

    affine.pos[0] = affine.pos[0].wrapping_add(affine.params[1] as i32);
    affine.pos[1] = affine.pos[1].wrapping_add(affine.params[3] as i32);
}

#[allow(clippy::similar_names)]
pub fn render_scanline_bg_large<R: Role, const DISPLAY_AREA_OVERFLOW: bool>(
    engine: &mut Engine2d<R>,
    vram: &Vram,
) {
    let bg_control = engine.bgs[2].control;

    let pixel_attrs = BgObjPixel(0).with_color_effects_mask(1 << 2);

    let (x_shift, y_shift) = match bg_control.size_key() {
        0 => (1, 2),
        1 => (2, 1),
        2 => (1, 0),
        _ => (1, 1),
    };

    let display_area_x_overflow_mask = !((0x1_0000 << x_shift) - 1);
    let display_area_y_overflow_mask = !((0x1_0000 << y_shift) - 1);

    let pos_x_map_mask = ((0x100 << x_shift) - 1) << 8;
    let pos_y_map_mask = ((0x100 << y_shift) - 1) << 8;

    let affine = &mut engine.affine_bg_data[0];
    let mut pos = affine.pos;

    for i in 0..SCREEN_WIDTH {
        if engine.window.0[i].0 & 1 << 2 != 0
            && (DISPLAY_AREA_OVERFLOW
                || (pos[0] & display_area_x_overflow_mask)
                    | (pos[1] & display_area_y_overflow_mask)
                    == 0)
        {
            let pixel_addr =
                (pos[1] as u32 & pos_y_map_mask) << x_shift | (pos[0] as u32 & pos_x_map_mask) >> 8;
            let color_index = if R::IS_A {
                vram.read_a_bg::<u8>(pixel_addr)
            } else {
                vram.read_b_bg::<u8>(pixel_addr)
            };
            if color_index != 0 {
                let color = unsafe {
                    vram.palette.read_le_aligned_unchecked::<u16>(
                        (!R::IS_A as usize) << 10 | (color_index as usize) << 1,
                    )
                };
                engine.bg_obj_scanline.0[i] = (engine.bg_obj_scanline.0[i] as u64) << 32
                    | (rgb5_to_rgb6(color as u32) | pixel_attrs.0) as u64;
            }
        }

        pos[0] = pos[0].wrapping_add(affine.params[0] as i32);
        pos[1] = pos[1].wrapping_add(affine.params[2] as i32);
    }

    affine.pos[0] = affine.pos[0].wrapping_add(affine.params[1] as i32);
    affine.pos[1] = affine.pos[1].wrapping_add(affine.params[3] as i32);
}

#[allow(clippy::similar_names)]
pub fn render_scanline_bg_extended<R: Role, const DISPLAY_AREA_OVERFLOW: bool>(
    engine: &mut Engine2d<R>,
    bg_index: AffineBgIndex,
    vram: &Vram,
) {
    let bg_control = engine.bgs[bg_index.get() as usize | 2].control;

    let bg_mask = 4 << bg_index.get();
    let pixel_attrs = BgObjPixel(0).with_color_effects_mask(bg_mask);

    let affine = &mut engine.affine_bg_data[bg_index.get() as usize];
    let mut pos = affine.pos;

    if bg_control.use_bitmap_extended_bg() {
        let data_base = bg_control.map_base() << 3;

        let (x_shift, y_shift) = match bg_control.size_key() {
            0 => (0, 0),
            1 => (1, 1),
            2 => (2, 1),
            _ => (2, 2),
        };

        let display_area_x_overflow_mask = !((0x8000 << x_shift) - 1);
        let display_area_y_overflow_mask = !((0x8000 << y_shift) - 1);

        let pos_x_map_mask = ((0x80 << x_shift) - 1) << 8;
        let pos_y_map_mask = ((0x80 << y_shift) - 1) << 8;

        if bg_control.use_direct_color_extended_bg() {
            for i in 0..SCREEN_WIDTH {
                if engine.window.0[i].0 & bg_mask != 0
                    && (DISPLAY_AREA_OVERFLOW
                        || (pos[0] & display_area_x_overflow_mask)
                            | (pos[1] & display_area_y_overflow_mask)
                            == 0)
                {
                    let pixel_addr = data_base
                        + ((pos[1] as u32 & pos_y_map_mask) << x_shift
                            | (pos[0] as u32 & pos_x_map_mask) >> 7);
                    let color = if R::IS_A {
                        vram.read_a_bg::<u16>(pixel_addr)
                    } else {
                        vram.read_b_bg::<u16>(pixel_addr)
                    };
                    if color & 0x8000 != 0 {
                        engine.bg_obj_scanline.0[i] = (engine.bg_obj_scanline.0[i] as u64) << 32
                            | (rgb5_to_rgb6(color as u32) | pixel_attrs.0) as u64;
                    }
                }

                pos[0] = pos[0].wrapping_add(affine.params[0] as i32);
                pos[1] = pos[1].wrapping_add(affine.params[2] as i32);
            }
        } else {
            for i in 0..SCREEN_WIDTH {
                if engine.window.0[i].0 & bg_mask != 0
                    && (DISPLAY_AREA_OVERFLOW
                        || (pos[0] & display_area_x_overflow_mask)
                            | (pos[1] & display_area_y_overflow_mask)
                            == 0)
                {
                    let pixel_addr = data_base
                        + ((pos[1] as u32 & pos_y_map_mask) >> 1 << x_shift
                            | (pos[0] as u32 & pos_x_map_mask) >> 8);
                    let color_index = if R::IS_A {
                        vram.read_a_bg::<u8>(pixel_addr)
                    } else {
                        vram.read_b_bg::<u8>(pixel_addr)
                    };
                    if color_index != 0 {
                        let color = unsafe {
                            vram.palette.read_le_aligned_unchecked::<u16>(
                                (!R::IS_A as usize) << 10 | (color_index as usize) << 1,
                            )
                        };
                        engine.bg_obj_scanline.0[i] = (engine.bg_obj_scanline.0[i] as u64) << 32
                            | (rgb5_to_rgb6(color as u32) | pixel_attrs.0) as u64;
                    }
                }

                pos[0] = pos[0].wrapping_add(affine.params[0] as i32);
                pos[1] = pos[1].wrapping_add(affine.params[2] as i32);
            }
        }
    } else {
        let map_base = if R::IS_A {
            engine.control.a_map_base() | bg_control.map_base()
        } else {
            bg_control.map_base()
        };
        let tile_base = if R::IS_A {
            engine.control.a_tile_base() + bg_control.tile_base()
        } else {
            bg_control.tile_base()
        };

        let display_area_overflow_mask = !((0x8000 << bg_control.size_key()) - 1);

        let map_row_shift = 4 + bg_control.size_key();
        let pos_map_mask = ((1 << map_row_shift) - 1) << 11;
        let pos_y_to_map_y_shift = 10 - map_row_shift;

        let (palette, pal_base_mask) = if engine.control.bg_ext_pal_enabled() {
            (
                unsafe {
                    if R::IS_A {
                        vram.a_bg_ext_pal.as_ptr()
                    } else {
                        vram.b_bg_ext_pal_ptr
                    }
                    .add((bg_index.get() as usize | 2) << 13) as *const u16
                },
                0xF00,
            )
        } else {
            (
                unsafe { vram.palette.as_ptr().add((!R::IS_A as usize) << 10) as *const u16 },
                0,
            )
        };

        for i in 0..SCREEN_WIDTH {
            if engine.window.0[i].0 & bg_mask != 0
                && (DISPLAY_AREA_OVERFLOW || (pos[0] | pos[1]) & display_area_overflow_mask == 0)
            {
                let tile_addr = map_base
                    + ((pos[1] as u32 & pos_map_mask) >> pos_y_to_map_y_shift
                        | (pos[0] as u32 & pos_map_mask) >> 10);
                let tile = if R::IS_A {
                    vram.read_a_bg::<u16>(tile_addr)
                } else {
                    vram.read_b_bg::<u16>(tile_addr)
                };

                let x_offset = if tile & 1 << 10 == 0 {
                    pos[0] as u32 >> 8 & 7
                } else {
                    !pos[0] as u32 >> 8 & 7
                };
                let y_offset = if tile & 1 << 11 == 0 {
                    pos[1] as u32 >> 5 & 0x38
                } else {
                    !pos[1] as u32 >> 5 & 0x38
                };

                let pixel_addr = tile_base + ((tile as u32 & 0x3FF) << 6 | y_offset | x_offset);
                let color_index = if R::IS_A {
                    vram.read_a_bg::<u8>(pixel_addr)
                } else {
                    vram.read_b_bg::<u8>(pixel_addr)
                };

                if color_index != 0 {
                    let pal_base = (tile >> 4 & pal_base_mask) as usize;
                    let color = unsafe { palette.add(pal_base | color_index as usize).read() };
                    engine.bg_obj_scanline.0[i] = (engine.bg_obj_scanline.0[i] as u64) << 32
                        | (rgb5_to_rgb6(color as u32) | pixel_attrs.0) as u64;
                }
            }

            pos[0] = pos[0].wrapping_add(affine.params[0] as i32);
            pos[1] = pos[1].wrapping_add(affine.params[2] as i32);
        }
    }

    affine.pos[0] = affine.pos[0].wrapping_add(affine.params[1] as i32);
    affine.pos[1] = affine.pos[1].wrapping_add(affine.params[3] as i32);
}
