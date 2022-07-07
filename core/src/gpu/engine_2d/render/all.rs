use super::common::{read_bg_text_tiles, TextTiles};
use super::*;

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
                    | (rgb_15_to_18(color as u32) | pixel_attrs.0) as u64;
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
                    | (rgb_15_to_18(color as u32) | pixel_attrs.0) as u64;
            }
            x += 1;
        }
    }
}
