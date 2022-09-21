use super::common::{read_bg_text_tiles, TextTiles};
use crate::common::{rgb5_to_rgb6_64, BgObjPixel, Buffers, RenderingData, Vram};
use dust_core::gpu::{
    engine_2d::{AffineBgIndex, BgIndex, Role},
    Scanline, SCREEN_WIDTH,
};

pub fn render_scanline_bgs_and_objs<
    R: Role,
    B: Buffers,
    D: RenderingData,
    V: Vram<R>,
    const BG_MODE: u8,
>(
    buffers: &mut B,
    vcount: u8,
    data: &mut D,
    vram: &V,
    scanline_3d: Option<&Scanline<u32, SCREEN_WIDTH>>,
) where
    [(); R::BG_VRAM_LEN]: Sized,
{
    macro_rules! incr_affine {
        ($i: literal) => {
            data.increase_affine_bg_pos(AffineBgIndex::new($i));
        };
    }

    macro_rules! affine_display_area_overflow {
        ($i: literal) => {
            data.bg_control(BgIndex::new($i))
                .affine_display_area_overflow()
        };
    }

    let render_scanline_bg_affine = [
        render_scanline_bg_affine::<_, _, _, _, false>,
        render_scanline_bg_affine::<_, _, _, _, true>,
    ];

    let render_scanline_bg_extended = [
        render_scanline_bg_extended::<_, _, _, _, false>,
        render_scanline_bg_extended::<_, _, _, _, true>,
    ];

    for priority in (0..4).rev() {
        if data.bg_priority(BgIndex::new(3)) == priority {
            match BG_MODE {
                0 => {
                    render_scanline_bg_text(buffers, BgIndex::new(3), vcount, data, vram);
                }
                1..=2 => {
                    render_scanline_bg_affine[affine_display_area_overflow!(3) as usize](
                        buffers,
                        AffineBgIndex::new(1),
                        data,
                        vram,
                    );
                    incr_affine!(1);
                }
                3..=5 => {
                    render_scanline_bg_extended[affine_display_area_overflow!(3) as usize](
                        buffers,
                        AffineBgIndex::new(1),
                        data,
                        vram,
                    );
                    incr_affine!(1);
                }
                _ => {}
            }
        }

        if data.bg_priority(BgIndex::new(2)) == priority {
            match BG_MODE {
                0..=1 | 3 => {
                    render_scanline_bg_text(buffers, BgIndex::new(2), vcount, data, vram);
                }
                2 | 4 => {
                    render_scanline_bg_affine[affine_display_area_overflow!(2) as usize](
                        buffers,
                        AffineBgIndex::new(0),
                        data,
                        vram,
                    );
                    incr_affine!(0);
                }
                5 => {
                    render_scanline_bg_extended[affine_display_area_overflow!(2) as usize](
                        buffers,
                        AffineBgIndex::new(0),
                        data,
                        vram,
                    );
                    incr_affine!(0);
                }
                6 => {
                    [
                        render_scanline_bg_large::<_, _, _, _, false>,
                        render_scanline_bg_large::<_, _, _, _, true>,
                    ][affine_display_area_overflow!(2) as usize](
                        buffers, data, vram
                    );
                    incr_affine!(0);
                }
                _ => {}
            }
        }

        if data.bg_priority(BgIndex::new(1)) == priority && BG_MODE != 6 {
            render_scanline_bg_text(buffers, BgIndex::new(1), vcount, data, vram);
        }

        if data.bg_priority(BgIndex::new(0)) == priority {
            if R::IS_A && data.control().bg0_3d() {
                if let Some(scanline_3d) = scanline_3d {
                    render_scanline_bg_3d(buffers, scanline_3d);
                }
            } else if BG_MODE != 6 {
                render_scanline_bg_text(buffers, BgIndex::new(0), vcount, data, vram);
            }
        }

        let scanline = unsafe { buffers.bg_obj_scanline() };
        let obj_scanline = unsafe { buffers.obj_scanline() };
        let window = unsafe { buffers.window() };
        let palette = vram.obj_palette();
        let obj_ext_pal = vram.obj_ext_palette();

        for i in 0..SCREEN_WIDTH {
            if window.0[i].0 & 1 << 4 == 0 {
                continue;
            }

            let obj_pixel = obj_scanline.0[i];
            if obj_pixel.priority() == priority {
                let pixel_attrs =
                    BgObjPixel((obj_pixel.0 & 0x03FC_0000) as u64).with_color_effects_mask(1 << 4);
                let color = unsafe {
                    rgb5_to_rgb6_64(if obj_pixel.use_raw_color() {
                        obj_pixel.raw_color()
                    } else if obj_pixel.use_ext_pal() {
                        obj_ext_pal
                            .read_le_aligned_unchecked::<u16>((obj_pixel.pal_color() as usize) << 1)
                    } else {
                        palette
                            .read_le_aligned_unchecked::<u16>((obj_pixel.pal_color() as usize) << 1)
                    })
                };
                scanline.0[i].0 = scanline.0[i].0 << 32 | color | pixel_attrs.0;
            }
        }
    }
}

fn render_scanline_bg_text<R: Role, B: Buffers, D: RenderingData, V: Vram<R>>(
    buffers: &mut B,
    bg_index: BgIndex,
    vcount: u8,
    data: &D,
    vram: &V,
) where
    [(); R::BG_VRAM_LEN]: Sized,
{
    let control = data.control();
    let bg_control = data.bg_control(bg_index);

    let scroll = data.bg_scroll(bg_index);
    let x_start = scroll[0] as u32;
    let y = scroll[1] as u32 + vcount as u32;

    let mut tiles = TextTiles::new_uninit();
    let tiles = read_bg_text_tiles::<R, V>(&mut tiles, control, bg_control, y, vram);

    let tile_base = if R::IS_A {
        control.a_tile_base() + bg_control.tile_base()
    } else {
        bg_control.tile_base()
    };

    let bg_mask = 1 << bg_index.get();
    let pixel_attrs = BgObjPixel(0).with_color_effects_mask(bg_mask);

    let tile_off_mask = tiles.len() - 1;
    let y_in_tile = y & 7;
    let mut pal_base = 0;
    let mut x = x_start;

    if bg_control.use_256_colors() {
        let (palette, pal_base_mask) = if control.bg_ext_pal_enabled() {
            let slot = bg_index.get()
                | if bg_index.get() < 2 {
                    bg_control.bg01_ext_pal_slot() << 1
                } else {
                    0
                };
            (
                unsafe { vram.bg_ext_palette().as_ptr().add((slot as usize) << 13) as *const u16 },
                0xF,
            )
        } else {
            (vram.bg_palette().as_ptr() as *const u16, 0)
        };

        let mut pixels = 0;
        let bg_vram = vram.bg();
        let scanline = unsafe { buffers.bg_obj_scanline() };
        let window = unsafe { buffers.window() };

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
                pixels = unsafe {
                    bg_vram.read_le_aligned_unchecked::<u64>(
                        (tile_base & (R::BG_VRAM_MASK & !7)) as usize,
                    )
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
            if color_index != 0 && window.0[i].0 & bg_mask != 0 {
                let color = unsafe { palette.add(pal_base | color_index as usize).read() };
                scanline.0[i].0 = scanline.0[i].0 << 32 | rgb5_to_rgb6_64(color) | pixel_attrs.0;
            }
            x += 1;
        }
    } else {
        let mut pixels = 0;
        let palette = vram.bg_palette();
        let bg_vram = vram.bg();
        let scanline = unsafe { buffers.bg_obj_scanline() };
        let window = unsafe { buffers.window() };

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
                pixels = unsafe {
                    bg_vram.read_le_aligned_unchecked::<u32>(
                        (tile_base & (R::BG_VRAM_MASK & !3)) as usize,
                    )
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
            if color_index != 0 && window.0[i].0 & bg_mask != 0 {
                let color = unsafe {
                    palette.read_le_aligned_unchecked::<u16>(pal_base | (color_index as usize) << 1)
                };
                scanline.0[i].0 = scanline.0[i].0 << 32 | rgb5_to_rgb6_64(color) | pixel_attrs.0;
            }
            x += 1;
        }
    }
}

#[allow(clippy::similar_names)]
fn render_scanline_bg_affine<
    R: Role,
    B: Buffers,
    D: RenderingData,
    V: Vram<R>,
    const DISPLAY_AREA_OVERFLOW: bool,
>(
    buffers: &mut B,
    bg_index: AffineBgIndex,
    data: &D,
    vram: &V,
) where
    [(); R::BG_VRAM_LEN]: Sized,
{
    let control = data.control();
    let bg_control = data.bg_control(bg_index.into());

    let map_base = if R::IS_A {
        control.a_map_base() | bg_control.map_base()
    } else {
        bg_control.map_base()
    };
    let tile_base = if R::IS_A {
        control.a_tile_base() + bg_control.tile_base()
    } else {
        bg_control.tile_base()
    };

    let bg_mask = 4 << bg_index.get();
    let pixel_attrs = BgObjPixel(0).with_color_effects_mask(bg_mask);

    let display_area_overflow_mask = !((0x8000 << bg_control.size_key()) - 1);

    let map_row_shift = 4 + bg_control.size_key();
    let pos_map_mask = ((1 << map_row_shift) - 1) << 11;
    let pos_y_to_map_y_shift = 11 - map_row_shift;

    let mut pos = data.affine_bg_pos(bg_index);
    let pos_incr = {
        let value = data.affine_bg_x_incr(bg_index);
        [value[0] as i32, value[1] as i32]
    };

    let palette = vram.bg_palette();
    let bg_vram = vram.bg();
    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

    for i in 0..SCREEN_WIDTH {
        if window.0[i].0 & bg_mask != 0
            && (DISPLAY_AREA_OVERFLOW || (pos[0] | pos[1]) & display_area_overflow_mask == 0)
        {
            let tile_addr = map_base
                + ((pos[1] as u32 & pos_map_mask) >> pos_y_to_map_y_shift
                    | (pos[0] as u32 & pos_map_mask) >> 11);
            let tile = unsafe { bg_vram.read_unchecked((tile_addr & R::BG_VRAM_MASK) as usize) };

            let pixel_addr = tile_base
                + ((tile as u32) << 6 | (pos[1] as u32 >> 5 & 0x38) | (pos[0] as u32 >> 8 & 7));
            let color_index =
                unsafe { bg_vram.read_unchecked((pixel_addr & R::BG_VRAM_MASK) as usize) };

            if color_index != 0 {
                let color = unsafe {
                    palette.read_le_aligned_unchecked::<u16>((color_index as usize) << 1)
                };
                scanline.0[i].0 = scanline.0[i].0 << 32 | rgb5_to_rgb6_64(color) | pixel_attrs.0;
            }
        }

        pos[0] = pos[0].wrapping_add(pos_incr[0]);
        pos[1] = pos[1].wrapping_add(pos_incr[1]);
    }
}

#[allow(clippy::similar_names)]
fn render_scanline_bg_large<
    R: Role,
    B: Buffers,
    D: RenderingData,
    V: Vram<R>,
    const DISPLAY_AREA_OVERFLOW: bool,
>(
    buffers: &mut B,
    data: &D,
    vram: &V,
) where
    [(); R::BG_VRAM_LEN]: Sized,
{
    let bg_control = data.bg_control(BgIndex::new(2));

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

    let mut pos = data.affine_bg_pos(AffineBgIndex::new(0));
    let pos_incr = {
        let value = data.affine_bg_x_incr(AffineBgIndex::new(0));
        [value[0] as i32, value[1] as i32]
    };

    let palette = vram.bg_palette();
    let bg_vram = vram.bg();
    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

    for i in 0..SCREEN_WIDTH {
        if window.0[i].0 & 1 << 2 != 0
            && (DISPLAY_AREA_OVERFLOW
                || (pos[0] & display_area_x_overflow_mask)
                    | (pos[1] & display_area_y_overflow_mask)
                    == 0)
        {
            let pixel_addr =
                (pos[1] as u32 & pos_y_map_mask) << x_shift | (pos[0] as u32 & pos_x_map_mask) >> 8;
            let color_index =
                unsafe { bg_vram.read_unchecked((pixel_addr & R::BG_VRAM_MASK) as usize) };
            if color_index != 0 {
                let color = unsafe {
                    palette.read_le_aligned_unchecked::<u16>((color_index as usize) << 1)
                };
                scanline.0[i].0 = scanline.0[i].0 << 32 | rgb5_to_rgb6_64(color) | pixel_attrs.0;
            }
        }

        pos[0] = pos[0].wrapping_add(pos_incr[0]);
        pos[1] = pos[1].wrapping_add(pos_incr[1]);
    }
}

#[allow(clippy::similar_names)]
fn render_scanline_bg_extended<
    R: Role,
    B: Buffers,
    D: RenderingData,
    V: Vram<R>,
    const DISPLAY_AREA_OVERFLOW: bool,
>(
    buffers: &mut B,
    bg_index: AffineBgIndex,
    data: &D,
    vram: &V,
) where
    [(); R::BG_VRAM_LEN]: Sized,
{
    let bg_control = data.bg_control(bg_index.into());

    let bg_mask = 4 << bg_index.get();
    let pixel_attrs = BgObjPixel(0).with_color_effects_mask(bg_mask);

    let mut pos = data.affine_bg_pos(bg_index);
    let pos_incr = {
        let value = data.affine_bg_x_incr(bg_index);
        [value[0] as i32, value[1] as i32]
    };

    let palette = vram.bg_palette();
    let bg_vram = vram.bg();
    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

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
                if window.0[i].0 & bg_mask != 0
                    && (DISPLAY_AREA_OVERFLOW
                        || (pos[0] & display_area_x_overflow_mask)
                            | (pos[1] & display_area_y_overflow_mask)
                            == 0)
                {
                    let pixel_addr = data_base
                        + ((pos[1] as u32 & pos_y_map_mask) << x_shift
                            | (pos[0] as u32 & pos_x_map_mask) >> 7);
                    let color = unsafe {
                        bg_vram.read_le_aligned_unchecked::<u16>(
                            (pixel_addr & (R::BG_VRAM_MASK & !1)) as usize,
                        )
                    };
                    if color & 0x8000 != 0 {
                        scanline.0[i].0 =
                            scanline.0[i].0 << 32 | rgb5_to_rgb6_64(color) | pixel_attrs.0;
                    }
                }

                pos[0] = pos[0].wrapping_add(pos_incr[0]);
                pos[1] = pos[1].wrapping_add(pos_incr[1]);
            }
        } else {
            for i in 0..SCREEN_WIDTH {
                if window.0[i].0 & bg_mask != 0
                    && (DISPLAY_AREA_OVERFLOW
                        || (pos[0] & display_area_x_overflow_mask)
                            | (pos[1] & display_area_y_overflow_mask)
                            == 0)
                {
                    let pixel_addr = data_base
                        + ((pos[1] as u32 & pos_y_map_mask) >> 1 << x_shift
                            | (pos[0] as u32 & pos_x_map_mask) >> 8);
                    let color_index =
                        unsafe { bg_vram.read_unchecked((pixel_addr & R::BG_VRAM_MASK) as usize) };
                    if color_index != 0 {
                        let color = unsafe {
                            palette.read_le_aligned_unchecked::<u16>((color_index as usize) << 1)
                        };
                        scanline.0[i].0 =
                            scanline.0[i].0 << 32 | rgb5_to_rgb6_64(color) | pixel_attrs.0;
                    }
                }

                pos[0] = pos[0].wrapping_add(pos_incr[0]);
                pos[1] = pos[1].wrapping_add(pos_incr[1]);
            }
        }
    } else {
        let control = data.control();

        let map_base = if R::IS_A {
            control.a_map_base() | bg_control.map_base()
        } else {
            bg_control.map_base()
        };
        let tile_base = if R::IS_A {
            control.a_tile_base() + bg_control.tile_base()
        } else {
            bg_control.tile_base()
        };

        let display_area_overflow_mask = !((0x8000 << bg_control.size_key()) - 1);

        let map_row_shift = 4 + bg_control.size_key();
        let pos_map_mask = ((1 << map_row_shift) - 1) << 11;
        let pos_y_to_map_y_shift = 10 - map_row_shift;

        let (palette, pal_base_mask) = if control.bg_ext_pal_enabled() {
            (
                unsafe {
                    vram.bg_ext_palette()
                        .as_ptr()
                        .add((bg_index.get() as usize | 2) << 13) as *const u16
                },
                0xF00,
            )
        } else {
            (vram.bg_palette().as_ptr() as *const u16, 0)
        };

        for i in 0..SCREEN_WIDTH {
            if window.0[i].0 & bg_mask != 0
                && (DISPLAY_AREA_OVERFLOW || (pos[0] | pos[1]) & display_area_overflow_mask == 0)
            {
                let tile_addr = map_base
                    + ((pos[1] as u32 & pos_map_mask) >> pos_y_to_map_y_shift
                        | (pos[0] as u32 & pos_map_mask) >> 10);
                let tile = unsafe {
                    bg_vram.read_le_aligned_unchecked::<u16>(
                        (tile_addr & (R::BG_VRAM_MASK & !1)) as usize,
                    )
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
                let color_index =
                    unsafe { bg_vram.read_unchecked((pixel_addr & R::BG_VRAM_MASK) as usize) };

                if color_index != 0 {
                    let pal_base = (tile >> 4 & pal_base_mask) as usize;
                    let color = unsafe { palette.add(pal_base | color_index as usize).read() };
                    scanline.0[i].0 =
                        scanline.0[i].0 << 32 | rgb5_to_rgb6_64(color) | pixel_attrs.0;
                }
            }

            pos[0] = pos[0].wrapping_add(pos_incr[0]);
            pos[1] = pos[1].wrapping_add(pos_incr[1]);
        }
    }
}

fn render_scanline_bg_3d<B: Buffers>(buffers: &mut B, scanline_3d: &Scanline<u32>) {
    // TODO: 3D layer scrolling

    let pixel_attrs = BgObjPixel(0).with_color_effects_mask(1).with_is_3d(true);

    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

    for i in 0..SCREEN_WIDTH {
        if window.0[i].0 & 1 != 0 {
            let pixel = scanline_3d.0[i];
            if pixel >> 18 != 0 {
                scanline.0[i].0 = scanline.0[i].0 << 32 | pixel as u64 | pixel_attrs.0;
            }
        }
    }
}
