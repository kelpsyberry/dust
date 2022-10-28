// TODO: Possibly migrate to core::simd when masked loads/stores are supported

use super::common::{read_bg_text_tiles, TextTiles};
use crate::{rgb5_to_rgb6_64, BgObjPixel, Buffers, RenderingData, Vram};
use core::{arch::x86_64::*, mem::transmute, simd::u64x4};
use dust_core::gpu::{
    engine_2d::{AffineBgIndex, BgIndex, Role},
    Scanline, SCREEN_WIDTH,
};

#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn render_scanline_bgs_and_objs<
    R: Role,
    B: Buffers,
    D: RenderingData,
    V: Vram<R>,
    const BG_MODE: u8,
>(
    buffers: &mut B,
    vount: u8,
    data: &mut D,
    vram: &V,
    scanline_3d: Option<&Scanline<u32>>,
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
                    render_scanline_bg_text(buffers, BgIndex::new(3), vount, data, vram);
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
                    render_scanline_bg_text(buffers, BgIndex::new(2), vount, data, vram);
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
            render_scanline_bg_text(buffers, BgIndex::new(1), vount, data, vram);
        }

        if data.bg_priority(BgIndex::new(0)) == priority {
            if R::IS_A && data.control().bg0_3d() {
                if let Some(scanline_3d) = scanline_3d {
                    render_scanline_bg_3d(buffers, scanline_3d);
                }
            } else if BG_MODE != 6 {
                render_scanline_bg_text(buffers, BgIndex::new(0), vount, data, vram);
            }
        }

        let scanline = unsafe { buffers.bg_obj_scanline() };
        let obj_scanline = unsafe { buffers.obj_scanline() };
        let window = unsafe { buffers.window() };
        let palette = vram.obj_palette();
        let obj_ext_pal = vram.obj_ext_palette();

        // TODO: Vectorize this
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

static RGB5_TO_RGB6_DATA: [__m256i; 3] = unsafe {
    [
        transmute(u64x4::from_array([0x3E; 4])),
        transmute(u64x4::from_array([0xF80; 4])),
        transmute(u64x4::from_array([0x3_E000; 4])),
    ]
};

#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
unsafe fn rgb5_to_rgb6(values: __m256i) -> __m256i {
    _mm256_or_si256(
        _mm256_or_si256(
            _mm256_and_si256(_mm256_slli_epi64::<1>(values), RGB5_TO_RGB6_DATA[0]),
            _mm256_and_si256(_mm256_slli_epi64::<2>(values), RGB5_TO_RGB6_DATA[1]),
        ),
        _mm256_and_si256(_mm256_slli_epi64::<3>(values), RGB5_TO_RGB6_DATA[2]),
    )
}

#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn render_scanline_bg_text<R: Role, B: Buffers, D: RenderingData, V: Vram<R>>(
    buffers: &mut B,
    bg_index: BgIndex,
    vount: u8,
    data: &D,
    vram: &V,
) where
    [(); R::BG_VRAM_LEN]: Sized,
{
    let control = data.control();
    let bg_control = data.bg_control(bg_index);

    let scroll = data.bg_scroll(bg_index);
    let x_start = scroll[0] as u32;
    let y = scroll[1] as u32 + vount as u32;

    let tile_base = if R::IS_A {
        control.a_tile_base() + bg_control.tile_base()
    } else {
        bg_control.tile_base()
    };

    let mut tiles = TextTiles::new_uninit();
    let tiles = read_bg_text_tiles::<R, V>(&mut tiles, control, bg_control, y, vram);

    let zero = _mm256_setzero_si256();

    let bg_mask = 1 << bg_index.get();
    let pixel_attrs = _mm256_set1_epi64x(BgObjPixel(0).with_color_effects_mask(bg_mask).0 as i64);

    let tile_off_mask = tiles.len() - 1;
    let y_in_tile = y & 7;
    let mut x = x_start;
    let mut tile_i = x_start as usize >> 3 & tile_off_mask;

    let bg_vram = vram.bg();
    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

    macro_rules! render {
        (
            $i_shift: expr,
            |$tile_ident: ident| $palette: expr,
            |$tile_base_ident: ident, $remaining_ident: ident| $color_indices: expr,
            |$half_color_indices_ident: ident| $half_color_indices: expr
        ) => {
            render!(
                @inner
                $i_shift,
                |$tile_ident| $palette,
                |$tile_base_ident, $remaining_ident| $color_indices,
                |$half_color_indices_ident| $half_color_indices,
                0, 1
            )
        };
        (
            @inner
            $i_shift: expr,
            |$tile_ident: ident| $palette: expr,
            |$tile_base_ident: ident, $remaining_ident: ident| $color_indices: expr,
            |$half_color_indices_ident: ident| $half_color_indices: expr,
            $($i: expr),*
        ) => {
            let mut screen_i = 0;
            while screen_i < SCREEN_WIDTH {
                let $tile_ident = *tiles.get_unchecked(tile_i);
                tile_i = (tile_i + 1) & tile_off_mask;

                let y_in_tile = if $tile_ident & 1 << 11 == 0 {
                    y_in_tile
                } else {
                    7 ^ y_in_tile
                };
                let $tile_base_ident = tile_base + (
                    ($tile_ident as u32 & 0x3FF) << (5 + $i_shift) | y_in_tile << (2 + $i_shift)
                );

                let palette = $palette;
                let $remaining_ident = SCREEN_WIDTH - screen_i;
                let color_indices = $color_indices;
                let scanline_ptr = scanline.0.as_mut_ptr().add(screen_i);
                let window_ptr = window.0.as_ptr().add(screen_i) as *const u32;
                $(
                    let half_scanline_ptr = scanline_ptr.add($i * 4);
                    let half_window_ptr = window_ptr.add($i);
                    let $half_color_indices_ident = color_indices >> ($i << (4 + $i_shift));
                    let half_color_indices = $half_color_indices;
                    let modify_mask = _mm256_andnot_si256(
                        _mm256_cmpeq_epi64(half_color_indices, zero),
                        _mm256_sll_epi64(
                            _mm256_cvtepu8_epi64(_mm_set_epi64x(
                                0,
                                half_window_ptr.read_unaligned() as i64,
                            )),
                            _mm_set_epi64x(0, (63 - bg_index.get()) as i64),
                        ),
                    );
                    let new_pixels = _mm256_or_si256(
                        _mm256_or_si256(
                            rgb5_to_rgb6(
                                _mm256_mask_i64gather_epi64::<2>(
                                    zero,
                                    palette as *const i64,
                                    half_color_indices,
                                    modify_mask,
                                ),
                            ),
                            pixel_attrs,
                        ),
                        _mm256_slli_epi64::<32>(_mm256_maskload_epi64(
                            half_scanline_ptr as *const i64,
                            modify_mask,
                        )),
                    );
                    _mm256_maskstore_epi64(
                        half_scanline_ptr as *mut i64,
                        modify_mask,
                        new_pixels,
                    );
                )*

                let new_x = (x & !7) + 8;
                screen_i += (new_x - x) as usize;
                x = new_x;
            }
        };
    }

    if bg_control.use_256_colors() {
        let (palette, pal_base_mask) = if control.bg_ext_pal_enabled() {
            let slot = bg_index.get()
                | if bg_index.get() < 2 {
                    bg_control.bg01_ext_pal_slot() << 1
                } else {
                    0
                };
            (
                vram.bg_ext_palette().as_ptr().add((slot as usize) << 13) as *const u16,
                0xF,
            )
        } else {
            (vram.bg_palette().as_ptr() as *const u16, 0)
        };

        render!(
            1,
            |tile| palette.add(((tile >> 12 & pal_base_mask) << 8) as usize),
            |tile_base, remaining| {
                let color_indices_mask = if remaining >= 8 {
                    u64::MAX
                } else {
                    (1_u64 << (remaining << 3)) - 1
                };
                let mut raw = unsafe {
                    bg_vram.read_le_aligned_unchecked::<u64>(
                        (tile_base & (R::BG_VRAM_MASK & !7)) as usize,
                    )
                };
                if tile & 1 << 10 != 0 {
                    raw = raw.swap_bytes();
                }
                (raw & color_indices_mask) >> ((x & 7) << 3)
            },
            |half_color_indices| _mm256_cvtepu8_epi64(_mm_set_epi64x(
                0,
                half_color_indices as u32 as i64,
            ))
        );
    } else {
        let colors_mask = _mm_set1_epi32(0xF);
        let palette = vram.bg_palette().as_ptr() as *const u16;

        render!(
            0,
            |tile| palette.add((tile >> 12 << 4) as usize),
            |tile_base, remaining| {
                let color_indices_mask = if remaining >= 8 {
                    u32::MAX
                } else {
                    (1_u32 << (remaining << 2)) - 1
                };
                let mut raw = unsafe {
                    bg_vram.read_le_aligned_unchecked::<u32>(
                        (tile_base & (R::BG_VRAM_MASK & !3)) as usize,
                    )
                };
                if tile & 1 << 10 != 0 {
                    raw = raw.swap_bytes();
                    raw = (raw >> 4 & 0x0F0F_0F0F) | (raw << 4 & 0xF0F0_F0F0);
                }
                (raw & color_indices_mask) >> ((x & 7) << 2)
            },
            |half_color_indices| {
                let intermediate = _mm_cvtepu8_epi64(_mm_set_epi64x(0, half_color_indices as i64));
                _mm256_cvtepu32_epi64(_mm_and_si128(
                    _mm_or_si128(intermediate, _mm_slli_epi64::<28>(intermediate)),
                    colors_mask,
                ))
            }
        );
    }
}

#[allow(clippy::similar_names)]
#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn render_scanline_bg_affine<
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

    let zero = _mm256_setzero_si256();

    let bg_mask = 4 << bg_index.get();
    let pixel_attrs = _mm256_set1_epi64x(BgObjPixel(0).with_color_effects_mask(bg_mask).0 as i64);
    let addr_mask = _mm256_set1_epi64x(if R::IS_A { 0x7_FFFF } else { 0x1_FFFF });

    let pos = data.affine_bg_pos(bg_index);
    let pos_incr = {
        let value = data.affine_bg_pos(bg_index);
        [value[0] as i64, value[1] as i64]
    };
    let mut x = _mm256_add_epi64(
        _mm256_set_epi64x(pos_incr[0] * 3, pos_incr[0] * 2, pos_incr[0], 0),
        _mm256_set1_epi64x(pos[0] as i64),
    );
    let x_incr = _mm256_set1_epi64x(pos_incr[0] << 2);
    let mut y = _mm256_add_epi64(
        _mm256_set_epi64x(pos_incr[1] * 3, pos_incr[1] * 2, pos_incr[1], 0),
        _mm256_set1_epi64x(pos[1] as i64),
    );
    let y_incr = _mm256_set1_epi64x(pos_incr[1] << 2);

    let map_base = _mm256_set1_epi64x(if R::IS_A {
        control.a_map_base() | bg_control.map_base()
    } else {
        bg_control.map_base()
    } as i64);
    let tile_base = _mm256_set1_epi64x(if R::IS_A {
        control.a_tile_base() + bg_control.tile_base()
    } else {
        bg_control.tile_base()
    } as i64);

    let display_area_overflow_mask = _mm256_set1_epi64x(!((0x8000 << bg_control.size_key()) - 1));

    let map_row_shift = 4 + bg_control.size_key();
    let pos_map_mask = _mm256_set1_epi64x(((1 << map_row_shift) - 1) << 11);
    let pos_y_to_map_y_shift = _mm256_set1_epi64x((11 - map_row_shift) as i64);
    let x_offset_mask = _mm256_set1_epi64x(7);
    let y_offset_mask = _mm256_set1_epi64x(0x38);
    let byte_mask = _mm256_set1_epi64x(0xFF);

    let palette = vram.bg_palette();
    let bg_vram = vram.bg();
    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

    for i in (0..SCREEN_WIDTH).step_by(4) {
        let scanline_ptr = scanline.0.as_mut_ptr().add(i);
        let window_ptr = window.0.as_ptr().add(i) as *const u32;

        let mut modify_mask = _mm256_sll_epi64(
            _mm256_cvtepu8_epi64(_mm_set_epi64x(0, window_ptr.read() as i64)),
            _mm_set_epi64x(0, (61 - bg_index.get()) as i64),
        );
        if !DISPLAY_AREA_OVERFLOW {
            modify_mask = _mm256_and_si256(
                modify_mask,
                _mm256_cmpeq_epi64(
                    _mm256_and_si256(_mm256_or_si256(x, y), display_area_overflow_mask),
                    zero,
                ),
            );
        }

        let tile_addrs = _mm256_add_epi64(
            map_base,
            _mm256_or_si256(
                _mm256_srlv_epi64(_mm256_and_si256(y, pos_map_mask), pos_y_to_map_y_shift),
                _mm256_srli_epi64::<11>(_mm256_and_si256(x, pos_map_mask)),
            ),
        );
        let tiles = _mm256_mask_i64gather_epi64::<1>(
            zero,
            bg_vram.as_ptr() as *const i64,
            tile_addrs,
            modify_mask,
        );

        let pixel_addrs = _mm256_and_si256(
            _mm256_add_epi64(
                tile_base,
                _mm256_or_si256(
                    _mm256_or_si256(
                        _mm256_slli_epi64::<6>(_mm256_and_si256(tiles, byte_mask)),
                        _mm256_and_si256(_mm256_srli_epi64::<5>(y), y_offset_mask),
                    ),
                    _mm256_and_si256(_mm256_srli_epi64::<8>(x), x_offset_mask),
                ),
            ),
            addr_mask,
        );
        let color_indices = _mm256_and_si256(
            _mm256_mask_i64gather_epi64::<1>(
                zero,
                bg_vram.as_ptr() as *const i64,
                pixel_addrs,
                modify_mask,
            ),
            byte_mask,
        );
        modify_mask = _mm256_andnot_si256(_mm256_cmpeq_epi64(color_indices, zero), modify_mask);

        let new_pixels = _mm256_or_si256(
            _mm256_or_si256(
                rgb5_to_rgb6(_mm256_mask_i64gather_epi64::<2>(
                    zero,
                    palette.as_ptr() as *const i64,
                    color_indices,
                    modify_mask,
                )),
                pixel_attrs,
            ),
            _mm256_slli_epi64::<32>(_mm256_maskload_epi64(
                scanline_ptr as *const i64,
                modify_mask,
            )),
        );
        _mm256_maskstore_epi64(scanline_ptr as *mut i64, modify_mask, new_pixels);

        x = _mm256_add_epi64(x, x_incr);
        y = _mm256_add_epi64(y, y_incr);
    }
}

#[allow(clippy::similar_names)]
#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn render_scanline_bg_large<
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

    let zero = _mm256_setzero_si256();

    let pixel_attrs = _mm256_set1_epi64x(BgObjPixel(0).with_color_effects_mask(1 << 2).0 as i64);
    let addr_mask = _mm256_set1_epi64x(if R::IS_A { 0x7_FFFF } else { 0x1_FFFF });

    let pos = data.affine_bg_pos(AffineBgIndex::new(0));
    let pos_incr = {
        let value = data.affine_bg_pos(AffineBgIndex::new(0));
        [value[0] as i64, value[1] as i64]
    };
    let mut x = _mm256_add_epi64(
        _mm256_set_epi64x(pos_incr[0] * 3, pos_incr[0] * 2, pos_incr[0], 0),
        _mm256_set1_epi64x(pos[0] as i64),
    );
    let x_incr = _mm256_set1_epi64x(pos_incr[0] << 2);
    let mut y = _mm256_add_epi64(
        _mm256_set_epi64x(pos_incr[1] * 3, pos_incr[1] * 2, pos_incr[1], 0),
        _mm256_set1_epi64x(pos[1] as i64),
    );
    let y_incr = _mm256_set1_epi64x(pos_incr[1] << 2);

    let (x_shift, y_shift) = match bg_control.size_key() {
        0 => (1, 2),
        1 => (2, 1),
        2 => (1, 0),
        _ => (1, 1),
    };

    let display_area_x_overflow_mask = _mm256_set1_epi64x(!((0x1_0000 << x_shift) - 1));
    let display_area_y_overflow_mask = _mm256_set1_epi64x(!((0x1_0000 << y_shift) - 1));

    let pos_x_map_mask = _mm256_set1_epi64x(((0x100 << x_shift) - 1) << 8);
    let pos_y_map_mask = _mm256_set1_epi64x(((0x100 << y_shift) - 1) << 8);

    let x_shift = _mm256_set1_epi64x(x_shift);
    let color_indices_mask = _mm256_set1_epi64x(0xFF);

    let palette = vram.bg_palette();
    let bg_vram = vram.bg();
    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

    for i in (0..SCREEN_WIDTH).step_by(4) {
        let scanline_ptr = scanline.0.as_mut_ptr().add(i);
        let window_ptr = window.0.as_ptr().add(i) as *const u32;

        let mut modify_mask = _mm256_slli_epi64::<61>(_mm256_cvtepu8_epi64(_mm_set_epi64x(
            0,
            window_ptr.read() as i64,
        )));
        if !DISPLAY_AREA_OVERFLOW {
            modify_mask = _mm256_and_si256(
                modify_mask,
                _mm256_cmpeq_epi64(
                    _mm256_or_si256(
                        _mm256_and_si256(x, display_area_x_overflow_mask),
                        _mm256_and_si256(y, display_area_y_overflow_mask),
                    ),
                    zero,
                ),
            );
        }

        let pixel_addrs = _mm256_and_si256(
            _mm256_or_si256(
                _mm256_sllv_epi64(_mm256_and_si256(y, pos_y_map_mask), x_shift),
                _mm256_srli_epi64::<8>(_mm256_and_si256(x, pos_x_map_mask)),
            ),
            addr_mask,
        );
        let color_indices = _mm256_and_si256(
            _mm256_mask_i64gather_epi64::<1>(
                zero,
                bg_vram.as_ptr() as *const i64,
                pixel_addrs,
                modify_mask,
            ),
            color_indices_mask,
        );
        modify_mask = _mm256_andnot_si256(_mm256_cmpeq_epi64(color_indices, zero), modify_mask);

        let new_pixels = _mm256_or_si256(
            _mm256_or_si256(
                rgb5_to_rgb6(_mm256_mask_i64gather_epi64::<2>(
                    zero,
                    palette.as_ptr() as *const i64,
                    color_indices,
                    modify_mask,
                )),
                pixel_attrs,
            ),
            _mm256_slli_epi64::<32>(_mm256_maskload_epi64(
                scanline_ptr as *const i64,
                modify_mask,
            )),
        );
        _mm256_maskstore_epi64(scanline_ptr as *mut i64, modify_mask, new_pixels);

        x = _mm256_add_epi64(x, x_incr);
        y = _mm256_add_epi64(y, y_incr);
    }
}

#[allow(clippy::similar_names)]
#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn render_scanline_bg_extended<
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

    let zero = _mm256_setzero_si256();

    let bg_mask = 4 << bg_index.get();
    let pixel_attrs = _mm256_set1_epi64x(BgObjPixel(0).with_color_effects_mask(bg_mask).0 as i64);
    let window_mask_shift = _mm_set_epi64x(0, (61 - bg_index.get()) as i64);
    let addr_mask = _mm256_set1_epi64x(if R::IS_A { 0x7_FFFF } else { 0x1_FFFF });

    let pos = data.affine_bg_pos(bg_index);
    let pos_incr = {
        let value = data.affine_bg_x_incr(bg_index);
        [value[0] as i64, value[1] as i64]
    };
    let mut x = _mm256_add_epi64(
        _mm256_set_epi64x(pos_incr[0] * 3, pos_incr[0] * 2, pos_incr[0], 0),
        _mm256_set1_epi64x(pos[0] as i64),
    );
    let x_incr = _mm256_set1_epi64x(pos_incr[0] << 2);
    let mut y = _mm256_add_epi64(
        _mm256_set_epi64x(pos_incr[1] * 3, pos_incr[1] * 2, pos_incr[1], 0),
        _mm256_set1_epi64x(pos[1] as i64),
    );
    let y_incr = _mm256_set1_epi64x(pos_incr[1] << 2);

    let bg_vram = vram.bg();
    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

    if bg_control.use_bitmap_extended_bg() {
        let data_base = _mm256_set1_epi64x((bg_control.map_base() << 3) as i64);

        let (x_shift, y_shift) = match bg_control.size_key() {
            0 => (0, 0),
            1 => (1, 1),
            2 => (2, 1),
            _ => (2, 2),
        };

        let display_area_x_overflow_mask = _mm256_set1_epi64x(!((0x8000 << x_shift) - 1));
        let display_area_y_overflow_mask = _mm256_set1_epi64x(!((0x8000 << y_shift) - 1));

        let pos_x_map_mask = _mm256_set1_epi64x(((0x80 << x_shift) - 1) << 8);
        let pos_y_map_mask = _mm256_set1_epi64x(((0x80 << y_shift) - 1) << 8);

        let x_shift = _mm256_set1_epi64x(x_shift);

        if bg_control.use_direct_color_extended_bg() {
            for i in (0..SCREEN_WIDTH).step_by(4) {
                let scanline_ptr = scanline.0.as_mut_ptr().add(i);
                let window_ptr = window.0.as_ptr().add(i) as *const u32;

                let mut modify_mask = _mm256_sll_epi64(
                    _mm256_cvtepu8_epi64(_mm_set_epi64x(0, window_ptr.read() as i64)),
                    window_mask_shift,
                );
                if !DISPLAY_AREA_OVERFLOW {
                    modify_mask = _mm256_and_si256(
                        modify_mask,
                        _mm256_cmpeq_epi64(
                            _mm256_or_si256(
                                _mm256_and_si256(x, display_area_x_overflow_mask),
                                _mm256_and_si256(y, display_area_y_overflow_mask),
                            ),
                            zero,
                        ),
                    );
                }

                let pixel_addrs = _mm256_and_si256(
                    _mm256_add_epi64(
                        data_base,
                        _mm256_or_si256(
                            _mm256_sllv_epi64(_mm256_and_si256(y, pos_y_map_mask), x_shift),
                            _mm256_srli_epi64::<7>(_mm256_and_si256(x, pos_x_map_mask)),
                        ),
                    ),
                    addr_mask,
                );
                let raw_colors = _mm256_mask_i64gather_epi64::<1>(
                    zero,
                    bg_vram.as_ptr() as *const i64,
                    pixel_addrs,
                    modify_mask,
                );

                modify_mask = _mm256_and_si256(modify_mask, _mm256_slli_epi64::<48>(raw_colors));

                let new_pixels = _mm256_or_si256(
                    _mm256_or_si256(rgb5_to_rgb6(raw_colors), pixel_attrs),
                    _mm256_slli_epi64::<32>(_mm256_maskload_epi64(
                        scanline_ptr as *const i64,
                        modify_mask,
                    )),
                );
                _mm256_maskstore_epi64(scanline_ptr as *mut i64, modify_mask, new_pixels);

                x = _mm256_add_epi64(x, x_incr);
                y = _mm256_add_epi64(y, y_incr);
            }
        } else {
            let color_indices_mask = _mm256_set1_epi64x(0xFF);
            let palette = vram.bg_palette();

            for i in (0..SCREEN_WIDTH).step_by(4) {
                let scanline_ptr = scanline.0.as_mut_ptr().add(i);
                let window_ptr = window.0.as_ptr().add(i) as *const u32;

                let mut modify_mask = _mm256_sll_epi64(
                    _mm256_cvtepu8_epi64(_mm_set_epi64x(0, window_ptr.read() as i64)),
                    window_mask_shift,
                );
                if !DISPLAY_AREA_OVERFLOW {
                    modify_mask = _mm256_and_si256(
                        modify_mask,
                        _mm256_cmpeq_epi64(
                            _mm256_or_si256(
                                _mm256_and_si256(x, display_area_x_overflow_mask),
                                _mm256_and_si256(y, display_area_y_overflow_mask),
                            ),
                            zero,
                        ),
                    );
                }

                let pixel_addrs = _mm256_and_si256(
                    _mm256_add_epi64(
                        data_base,
                        _mm256_or_si256(
                            _mm256_sllv_epi64(
                                _mm256_srli_epi64::<1>(_mm256_and_si256(y, pos_y_map_mask)),
                                x_shift,
                            ),
                            _mm256_srli_epi64::<8>(_mm256_and_si256(x, pos_x_map_mask)),
                        ),
                    ),
                    addr_mask,
                );
                let color_indices = _mm256_and_si256(
                    _mm256_mask_i64gather_epi64::<1>(
                        zero,
                        bg_vram.as_ptr() as *const i64,
                        pixel_addrs,
                        modify_mask,
                    ),
                    color_indices_mask,
                );
                modify_mask =
                    _mm256_andnot_si256(_mm256_cmpeq_epi64(color_indices, zero), modify_mask);

                let new_pixels = _mm256_or_si256(
                    _mm256_or_si256(
                        rgb5_to_rgb6(_mm256_mask_i64gather_epi64::<2>(
                            zero,
                            palette.as_ptr() as *const i64,
                            color_indices,
                            modify_mask,
                        )),
                        pixel_attrs,
                    ),
                    _mm256_slli_epi64::<32>(_mm256_maskload_epi64(
                        scanline_ptr as *const i64,
                        modify_mask,
                    )),
                );
                _mm256_maskstore_epi64(scanline_ptr as *mut i64, modify_mask, new_pixels);

                x = _mm256_add_epi64(x, x_incr);
                y = _mm256_add_epi64(y, y_incr);
            }
        }
    } else {
        let control = data.control();

        let map_base = _mm256_set1_epi64x(if R::IS_A {
            control.a_map_base() | bg_control.map_base()
        } else {
            bg_control.map_base()
        } as i64);
        let tile_base = _mm256_set1_epi64x(if R::IS_A {
            control.a_tile_base() + bg_control.tile_base()
        } else {
            bg_control.tile_base()
        } as i64);

        let display_area_overflow_mask =
            _mm256_set1_epi64x(!((0x8000 << bg_control.size_key()) - 1));

        let map_row_shift = 4 + bg_control.size_key();
        let pos_map_mask = _mm256_set1_epi64x(((1 << map_row_shift) - 1) << 11);
        let pos_y_to_map_y_shift = _mm256_set1_epi64x((10 - map_row_shift) as i64);
        let x_offset_mask = _mm256_set1_epi64x(7 << 8);
        let y_offset_mask = _mm256_set1_epi64x(0x38 << 5);
        let color_indices_mask = _mm256_set1_epi64x(0xFF);
        let tile_number_mask = _mm256_set1_epi64x(0x3FF);

        let (palette, pal_base_mask) = if control.bg_ext_pal_enabled() {
            (
                vram.bg_ext_palette()
                    .as_ptr()
                    .add((bg_index.get() as usize | 2) << 13),
                0xF00,
            )
        } else {
            (vram.bg_palette().as_ptr(), 0)
        };
        let pal_base_mask = _mm256_set1_epi64x(pal_base_mask);

        for i in (0..SCREEN_WIDTH).step_by(4) {
            let scanline_ptr = scanline.0.as_mut_ptr().add(i);
            let window_ptr = window.0.as_ptr().add(i) as *const u32;

            let mut modify_mask = _mm256_sll_epi64(
                _mm256_cvtepu8_epi64(_mm_set_epi64x(0, window_ptr.read() as i64)),
                window_mask_shift,
            );
            if !DISPLAY_AREA_OVERFLOW {
                modify_mask = _mm256_and_si256(
                    modify_mask,
                    _mm256_cmpeq_epi64(
                        _mm256_and_si256(_mm256_or_si256(x, y), display_area_overflow_mask),
                        zero,
                    ),
                );
            }

            let tile_addrs = _mm256_and_si256(
                _mm256_add_epi64(
                    map_base,
                    _mm256_or_si256(
                        _mm256_srlv_epi64(_mm256_and_si256(y, pos_map_mask), pos_y_to_map_y_shift),
                        _mm256_srli_epi64::<10>(_mm256_and_si256(x, pos_map_mask)),
                    ),
                ),
                addr_mask,
            );
            let tiles = _mm256_mask_i64gather_epi64::<1>(
                zero,
                bg_vram.as_ptr() as *const i64,
                tile_addrs,
                modify_mask,
            );

            let x_offsets = _mm256_srli_epi64::<8>(_mm256_and_si256(
                _mm256_castpd_si256(_mm256_blendv_pd(
                    _mm256_castsi256_pd(x),
                    _mm256_castsi256_pd(_mm256_xor_si256(x, x_offset_mask)),
                    _mm256_castsi256_pd(_mm256_slli_epi64::<53>(tiles)),
                )),
                x_offset_mask,
            ));
            let y_offsets = _mm256_srli_epi64::<5>(_mm256_and_si256(
                _mm256_castpd_si256(_mm256_blendv_pd(
                    _mm256_castsi256_pd(y),
                    _mm256_castsi256_pd(_mm256_xor_si256(y, y_offset_mask)),
                    _mm256_castsi256_pd(_mm256_slli_epi64::<52>(tiles)),
                )),
                y_offset_mask,
            ));

            let pixel_addrs = _mm256_and_si256(
                _mm256_add_epi64(
                    tile_base,
                    _mm256_or_si256(
                        _mm256_or_si256(
                            _mm256_slli_epi64::<6>(_mm256_and_si256(tiles, tile_number_mask)),
                            y_offsets,
                        ),
                        x_offsets,
                    ),
                ),
                addr_mask,
            );
            let color_indices = _mm256_and_si256(
                _mm256_mask_i64gather_epi64::<1>(
                    zero,
                    bg_vram.as_ptr() as *const i64,
                    pixel_addrs,
                    modify_mask,
                ),
                color_indices_mask,
            );
            modify_mask = _mm256_andnot_si256(_mm256_cmpeq_epi64(color_indices, zero), modify_mask);

            let new_pixels = _mm256_or_si256(
                _mm256_or_si256(
                    rgb5_to_rgb6(_mm256_mask_i64gather_epi64::<2>(
                        zero,
                        palette as *const i64,
                        _mm256_or_si256(
                            _mm256_and_si256(_mm256_srli_epi64::<4>(tiles), pal_base_mask),
                            color_indices,
                        ),
                        modify_mask,
                    )),
                    pixel_attrs,
                ),
                _mm256_slli_epi64::<32>(_mm256_maskload_epi64(
                    scanline_ptr as *const i64,
                    modify_mask,
                )),
            );
            _mm256_maskstore_epi64(scanline_ptr as *mut i64, modify_mask, new_pixels);

            x = _mm256_add_epi64(x, x_incr);
            y = _mm256_add_epi64(y, y_incr);
        }
    }
}

#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn render_scanline_bg_3d<B: Buffers>(buffers: &mut B, scanline_3d: &Scanline<u32>) {
    // TODO: 3D layer scrolling

    let zero = _mm256_setzero_si256();

    let pixel_attrs =
        _mm256_set1_epi64x(BgObjPixel(0).with_color_effects_mask(1).with_is_3d(true).0 as i64);

    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

    for i in (0..SCREEN_WIDTH).step_by(4) {
        let scanline_ptr = scanline.0.as_mut_ptr().add(i);
        let window_ptr = window.0.as_ptr().add(i) as *const u32;

        let mut modify_mask = _mm256_slli_epi64::<63>(_mm256_cvtepu8_epi64(_mm_set_epi64x(
            0,
            window_ptr.read() as i64,
        )));

        let pixels = _mm256_cvtepu32_epi64(_mm_load_si128(
            scanline_3d.0.as_ptr().add(i) as *const __m128i
        ));

        modify_mask = _mm256_andnot_si256(
            _mm256_cmpeq_epi64(_mm256_srli_epi64::<18>(pixels), zero),
            modify_mask,
        );

        let new_pixels = _mm256_or_si256(
            _mm256_or_si256(pixels, pixel_attrs),
            _mm256_slli_epi64::<32>(_mm256_maskload_epi64(
                scanline_ptr as *const i64,
                modify_mask,
            )),
        );
        _mm256_maskstore_epi64(scanline_ptr as *mut i64, modify_mask, new_pixels);
    }
}
