// TODO: Possibly migrate to core::simd when masked loads/stores are supported

use super::common::{read_bg_text_tiles, TextTiles};
use super::*;
use core::{
    arch::x86_64::*,
    mem::transmute,
    simd::{u32x8, u64x4},
};

type BlendData = ([__m256i; 3], [__m256i; 3], [__m256i; 3]);

static BLEND_DATA: BlendData = unsafe {
    (
        [
            transmute(u64x4::from_array([0x3F; 4])),
            transmute(u64x4::from_array([0xFC0; 4])),
            transmute(u64x4::from_array([0x3_F000; 4])),
        ],
        [
            transmute(u64x4::from_array([0x3F0; 4])),
            transmute(u64x4::from_array([0xFC00; 4])),
            transmute(u64x4::from_array([0x3F_0000; 4])),
        ],
        [
            transmute(u64x4::from_array([0x7E0; 4])),
            transmute(u64x4::from_array([0x1_F800; 4])),
            transmute(u64x4::from_array([0x7E_0000; 4])),
        ],
    )
};

#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
#[inline]
unsafe fn blend(pixels: __m256i, coeffs_a: __m256i, coeffs_b: __m256i) -> __m256i {
    let bot = _mm256_srli_epi64::<32>(pixels);
    macro_rules! comp {
        ($i: literal) => {{
            let unclamped = _mm256_add_epi64(
                _mm256_mullo_epi32(_mm256_and_si256(pixels, BLEND_DATA.0[$i]), coeffs_a),
                _mm256_mullo_epi32(_mm256_and_si256(bot, BLEND_DATA.0[$i]), coeffs_b),
            );
            _mm256_castpd_si256(_mm256_blendv_pd(
                _mm256_castsi256_pd(unclamped),
                _mm256_castsi256_pd(BLEND_DATA.1[$i]),
                _mm256_castsi256_pd(_mm256_cmpgt_epi64(unclamped, BLEND_DATA.1[$i])),
            ))
        }};
    }
    _mm256_srli_epi64::<4>(_mm256_or_si256(
        _mm256_or_si256(comp!(0), _mm256_and_si256(comp!(1), BLEND_DATA.1[1])),
        _mm256_and_si256(comp!(2), BLEND_DATA.1[2]),
    ))
}

#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
#[inline]
unsafe fn blend_5bit_coeff(pixels: __m256i, coeffs_a: __m256i, coeffs_b: __m256i) -> __m256i {
    let bot = _mm256_srli_epi64::<32>(pixels);
    macro_rules! comp {
        ($i: literal) => {{
            let unclamped = _mm256_add_epi64(
                _mm256_mullo_epi32(_mm256_and_si256(pixels, BLEND_DATA.0[$i]), coeffs_a),
                _mm256_mullo_epi32(_mm256_and_si256(bot, BLEND_DATA.0[$i]), coeffs_b),
            );
            _mm256_castpd_si256(_mm256_blendv_pd(
                _mm256_castsi256_pd(unclamped),
                _mm256_castsi256_pd(BLEND_DATA.2[$i]),
                _mm256_castsi256_pd(_mm256_cmpgt_epi64(unclamped, BLEND_DATA.2[$i])),
            ))
        }};
    }
    _mm256_srli_epi64::<5>(_mm256_or_si256(
        _mm256_or_si256(comp!(0), _mm256_and_si256(comp!(1), BLEND_DATA.2[1])),
        _mm256_and_si256(comp!(2), BLEND_DATA.2[2]),
    ))
}

#[allow(clippy::similar_names)]
#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn apply_color_effects<R: Role, const EFFECT: u8>(
    renderer: &mut Renderer<R>,
    data: &Data,
) {
    let zero = _mm256_set1_epi64x(0);

    let target_1_mask = _mm256_set1_epi64x(data.color_effects_control().target_1_mask() as i64);
    let target_2_mask = _mm256_set1_epi64x(data.color_effects_control().target_2_mask() as i64);
    let coeff_a = _mm256_set1_epi64x(data.blend_coeffs().0 as i64);
    let coeff_b = _mm256_set1_epi64x(data.blend_coeffs().1 as i64);
    let brightness_coeff = _mm256_set1_epi64x(data.brightness_coeff() as i64);

    let alpha_mask = _mm256_set1_epi64x(0x1F);
    let coeffs_max_semitransparent = _mm256_set1_epi64x(0x10);
    let alpha_incr_3d = _mm256_set1_epi64x(1);
    let coeffs_max_3d = _mm256_set1_epi64x(0x20);

    let brightness_in_rb_mask = _mm256_set1_epi64x(0x3_F03F);
    let brightness_in_g_mask = _mm256_set1_epi64x(0xFC0);
    let brightness_out_rb_mask = _mm256_set1_epi64x(0x3F_03F0);
    let brightness_out_g_mask = _mm256_set1_epi64x(0xFC00);
    let brightness_incr_max = _mm256_set1_epi64x(0x3_FFFF);

    for i in (0..SCREEN_WIDTH).step_by(4) {
        let scanline_pixels_ptr = renderer.bg_obj_scanline.0.as_mut_ptr().add(i);
        let window_ptr = renderer.window.0.as_ptr().add(i) as *const u32;

        let modify_mask = _mm256_slli_epi64::<58>(_mm256_cvtepu8_epi64(_mm_set_epi64x(
            0,
            window_ptr.read() as i64,
        )));

        let pixels = _mm256_maskload_epi64(scanline_pixels_ptr as *const i64, modify_mask);
        let bot_matches_inv = _mm256_cmpeq_epi64(
            _mm256_and_si256(_mm256_srli_epi64::<58>(pixels), target_2_mask),
            zero,
        );

        let custom_alphas = _mm256_and_si256(_mm256_srli_epi64::<19>(pixels), alpha_mask);

        let coeffs_a_3d = _mm256_add_epi64(custom_alphas, alpha_incr_3d);
        let coeffs_b_3d = _mm256_sub_epi64(coeffs_max_3d, coeffs_a_3d);

        let have_custom_alpha_mask = _mm256_castsi256_pd(_mm256_slli_epi64::<38>(pixels));
        let coeffs_a_semitransparent = _mm256_castpd_si256(_mm256_blendv_pd(
            _mm256_castsi256_pd(coeff_a),
            _mm256_castsi256_pd(custom_alphas),
            have_custom_alpha_mask,
        ));
        let coeffs_b_semitransparent = _mm256_castpd_si256(_mm256_blendv_pd(
            _mm256_castsi256_pd(coeff_b),
            _mm256_castsi256_pd(_mm256_sub_epi64(
                coeffs_max_semitransparent,
                coeffs_a_semitransparent,
            )),
            have_custom_alpha_mask,
        ));

        let effect_pixels = if EFFECT == 0 {
            _mm256_castsi256_pd(pixels)
        } else {
            let top_matches_inv = _mm256_cmpeq_epi64(
                _mm256_and_si256(_mm256_srli_epi64::<26>(pixels), target_1_mask),
                zero,
            );
            if EFFECT == 1 {
                _mm256_blendv_pd(
                    _mm256_castsi256_pd(blend(pixels, coeff_a, coeff_b)),
                    _mm256_castsi256_pd(pixels),
                    _mm256_castsi256_pd(_mm256_or_si256(top_matches_inv, bot_matches_inv)),
                )
            } else {
                let value = if EFFECT == 2 {
                    _mm256_xor_si256(brightness_incr_max, pixels)
                } else {
                    pixels
                };
                let offset = _mm256_srli_epi64::<4>(_mm256_or_si256(
                    _mm256_and_si256(
                        _mm256_mullo_epi32(
                            _mm256_and_si256(value, brightness_in_rb_mask),
                            brightness_coeff,
                        ),
                        brightness_out_rb_mask,
                    ),
                    _mm256_and_si256(
                        _mm256_mullo_epi32(
                            _mm256_and_si256(value, brightness_in_g_mask),
                            brightness_coeff,
                        ),
                        brightness_out_g_mask,
                    ),
                ));
                _mm256_blendv_pd(
                    _mm256_castsi256_pd(if EFFECT == 2 {
                        _mm256_add_epi64(pixels, offset)
                    } else {
                        _mm256_sub_epi64(pixels, offset)
                    }),
                    _mm256_castsi256_pd(pixels),
                    _mm256_castsi256_pd(top_matches_inv),
                )
            }
        };
        let new_pixels = _mm256_castpd_si256(_mm256_blendv_pd(
            _mm256_blendv_pd(
                effect_pixels,
                _mm256_castsi256_pd(blend(
                    pixels,
                    coeffs_a_semitransparent,
                    coeffs_b_semitransparent,
                )),
                _mm256_castsi256_pd(_mm256_andnot_si256(
                    bot_matches_inv,
                    _mm256_slli_epi64::<39>(pixels),
                )),
            ),
            _mm256_castsi256_pd(blend_5bit_coeff(pixels, coeffs_a_3d, coeffs_b_3d)),
            _mm256_castsi256_pd(_mm256_andnot_si256(
                bot_matches_inv,
                _mm256_slli_epi64::<45>(pixels),
            )),
        ));
        _mm256_maskstore_epi64(scanline_pixels_ptr as *mut i64, modify_mask, new_pixels);
    }
}

static RGB6_TO_RGBA8_DATA: ([__m256i; 3], __m256i, __m256i) = unsafe {
    (
        [
            transmute(u32x8::from_array([0x3F; 8])),
            transmute(u32x8::from_array([0x3F00; 8])),
            transmute(u32x8::from_array([0x3F_0000; 8])),
        ],
        transmute(u32x8::from_array([0xFF00_0000; 8])),
        transmute(u32x8::from_array([0x0003_0303; 8])),
    )
};

#[inline]
#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
unsafe fn rgb6_to_rgba8(values: __m256i) -> __m256i {
    let rgb6_8 = _mm256_or_si256(
        _mm256_or_si256(
            _mm256_and_si256(values, RGB6_TO_RGBA8_DATA.0[0]),
            _mm256_and_si256(_mm256_slli_epi32::<2>(values), RGB6_TO_RGBA8_DATA.0[1]),
        ),
        _mm256_and_si256(_mm256_slli_epi32::<4>(values), RGB6_TO_RGBA8_DATA.0[2]),
    );
    _mm256_or_si256(
        RGB6_TO_RGBA8_DATA.1,
        _mm256_or_si256(
            _mm256_slli_epi32::<2>(rgb6_8),
            _mm256_and_si256(_mm256_srli_epi32::<4>(rgb6_8), RGB6_TO_RGBA8_DATA.2),
        ),
    )
}

#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn apply_brightness<R: Role>(
    scanline_buffer: &mut Scanline<u32>,
    data: &Data,
) {
    let mode = data.master_brightness_control().mode();
    if matches!(mode, 1 | 2) && data.master_brightness_factor() != 0 {
        let brightness_in_rb_mask = _mm256_set1_epi32(0x3_F03F);
        let brightness_in_g_mask = _mm256_set1_epi32(0xFC0);
        let brightness_out_rb_mask = _mm256_set1_epi32(0x3F_03F0);
        let brightness_out_g_mask = _mm256_set1_epi32(0xFC00);
        let brightness_coeff = _mm256_set1_epi32(data.master_brightness_factor() as i32);

        macro_rules! offset {
            ($value: expr) => {
                _mm256_srli_epi64::<4>(_mm256_or_si256(
                    _mm256_and_si256(
                        _mm256_mullo_epi32(
                            _mm256_and_si256($value, brightness_in_rb_mask),
                            brightness_coeff,
                        ),
                        brightness_out_rb_mask,
                    ),
                    _mm256_and_si256(
                        _mm256_mullo_epi32(
                            _mm256_and_si256($value, brightness_in_g_mask),
                            brightness_coeff,
                        ),
                        brightness_out_g_mask,
                    ),
                ))
            };
        }

        if mode == 1 {
            let brightness_incr_max = _mm256_set1_epi32(0x3_FFFF);
            for i in (0..SCREEN_WIDTH).step_by(8) {
                let scanline_pixels_ptr = scanline_buffer.0.as_mut_ptr().add(i) as *mut __m256i;
                let pixels = _mm256_load_si256(scanline_pixels_ptr);
                let offset = offset!(_mm256_xor_si256(brightness_incr_max, pixels));
                _mm256_store_si256(
                    scanline_pixels_ptr,
                    rgb6_to_rgba8(_mm256_add_epi32(pixels, offset)),
                );
            }
        } else {
            for i in (0..SCREEN_WIDTH).step_by(8) {
                let scanline_pixels_ptr = scanline_buffer.0.as_mut_ptr().add(i) as *mut __m256i;
                let pixels = _mm256_load_si256(scanline_pixels_ptr);
                let offset = offset!(pixels);
                _mm256_store_si256(
                    scanline_pixels_ptr,
                    rgb6_to_rgba8(_mm256_sub_epi32(pixels, offset)),
                );
            }
        }
    } else {
        for i in (0..SCREEN_WIDTH).step_by(8) {
            let scanline_pixels_ptr = scanline_buffer.0.as_mut_ptr().add(i) as *mut __m256i;
            _mm256_store_si256(
                scanline_pixels_ptr,
                rgb6_to_rgba8(_mm256_load_si256(scanline_pixels_ptr)),
            );
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
pub unsafe fn render_scanline_bg_text<R: Role>(
    renderer: &mut Renderer<R>,
    bg_index: BgIndex,
    line: u8,
    data: &Data,
    vram: &Vram,
) {
    let bg = &data.bgs()[bg_index.get() as usize];

    let x_start = bg.scroll[0] as u32;
    let y = bg.scroll[1] as u32 + line as u32;

    let tile_base = if R::IS_A {
        data.control().a_tile_base() + bg.control().tile_base()
    } else {
        bg.control().tile_base()
    };

    let mut tiles = TextTiles::new_uninit();
    let tiles = read_bg_text_tiles::<R>(&mut tiles, data.control(), bg.control(), y, vram);

    let zero = _mm256_setzero_si256();

    let bg_mask = 1 << bg_index.get();
    let pixel_attrs = _mm256_set1_epi64x(BgObjPixel(0).with_color_effects_mask(bg_mask).0 as i64);

    let tile_off_mask = tiles.len() - 1;
    let y_in_tile = y & 7;
    let mut x = x_start;
    let mut tile_i = x_start as usize >> 3 & tile_off_mask;

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
                let scanline_pixels_ptr = renderer.bg_obj_scanline.0.as_mut_ptr().add(screen_i);
                let window_ptr = renderer.window.0.as_ptr().add(screen_i) as *const u32;
                $(
                    let half_scanline_pixels_ptr = scanline_pixels_ptr.add($i * 4);
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
                            half_scanline_pixels_ptr as *const i64,
                            modify_mask,
                        )),
                    );
                    _mm256_maskstore_epi64(
                        half_scanline_pixels_ptr as *mut i64,
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

    if bg.control().use_256_colors() {
        let (palette, pal_base_mask) = if data.control().bg_ext_pal_enabled() {
            let slot = bg_index.get()
                | if bg_index.get() < 2 {
                    bg.control().bg01_ext_pal_slot() << 1
                } else {
                    0
                };
            (
                if R::IS_A {
                    vram.a_bg_ext_pal.as_ptr()
                } else {
                    vram.b_bg_ext_pal_ptr
                }
                .add((slot as usize) << 13) as *const u16,
                0xF,
            )
        } else {
            (
                vram.palette.as_ptr().add((!R::IS_A as usize) << 10) as *const u16,
                0,
            )
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
                let mut raw = if R::IS_A {
                    vram.read_a_bg::<u64>(tile_base)
                } else {
                    vram.read_b_bg::<u64>(tile_base)
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
        let colors_low_mask = _mm_set1_epi64x(0xF);

        render!(
            0,
            |tile| (vram.palette.as_ptr() as *const u16)
                .add((!R::IS_A as usize) << 9 | (tile >> 12 << 4) as usize),
            |tile_base, remaining| {
                let color_indices_mask = if remaining >= 8 {
                    u32::MAX
                } else {
                    (1_u32 << (remaining << 2)) - 1
                };
                let mut raw = if R::IS_A {
                    vram.read_a_bg::<u32>(tile_base)
                } else {
                    vram.read_b_bg::<u32>(tile_base)
                };
                if tile & 1 << 10 != 0 {
                    raw = raw.swap_bytes();
                    raw = (raw >> 4 & 0x0F0F_0F0F) | (raw << 4 & 0xF0F0_F0F0);
                }
                (raw & color_indices_mask) >> ((x & 7) << 2)
            },
            |half_color_indices| {
                let intermediate =
                    _mm_cvtepu8_epi64(_mm_set_epi64x(0, half_color_indices as u16 as i64));
                _mm256_cvtepu32_epi64(_mm_or_si128(
                    _mm_and_si128(intermediate, colors_low_mask),
                    _mm_slli_epi64::<32>(_mm_srli_epi64::<4>(intermediate)),
                ))
            }
        );
    }
}

#[allow(clippy::similar_names)]
#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn render_scanline_bg_affine<R: Role, const DISPLAY_AREA_OVERFLOW: bool>(
    renderer: &mut Renderer<R>,
    bg_index: AffineBgIndex,
    data: &mut Data,
    vram: &Vram,
) {
    let bg_control = data.bgs()[bg_index.get() as usize | 2].control();

    let zero = _mm256_setzero_si256();

    let bg_mask = 4 << bg_index.get();
    let pixel_attrs = _mm256_set1_epi64x(BgObjPixel(0).with_color_effects_mask(bg_mask).0 as i64);
    let addr_mask = _mm256_set1_epi64x(if R::IS_A { 0x7_FFFF } else { 0x1_FFFF });

    let affine = &data.affine_bg_data()[bg_index.get() as usize];
    let affine_params = [affine.params[0] as i64, affine.params[2] as i64];
    let mut x = _mm256_add_epi64(
        _mm256_set_epi64x(
            affine_params[0] * 3,
            affine_params[0] * 2,
            affine_params[0],
            0,
        ),
        _mm256_set1_epi64x(affine.pos()[0] as i64),
    );
    let x_incr = _mm256_set1_epi64x(affine_params[0] << 2);
    let mut y = _mm256_add_epi64(
        _mm256_set_epi64x(
            affine_params[1] * 3,
            affine_params[1] * 2,
            affine_params[1],
            0,
        ),
        _mm256_set1_epi64x(affine.pos()[1] as i64),
    );
    let y_incr = _mm256_set1_epi64x(affine_params[1] << 2);

    let map_base = _mm256_set1_epi64x(if R::IS_A {
        data.control().a_map_base() | bg_control.map_base()
    } else {
        bg_control.map_base()
    } as i64);
    let tile_base = _mm256_set1_epi64x(if R::IS_A {
        data.control().a_tile_base() + bg_control.tile_base()
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

    let palette = (vram.palette.as_ptr() as *const u16).add((!R::IS_A as usize) << 9);

    for i in (0..SCREEN_WIDTH).step_by(4) {
        let scanline_pixels_ptr = renderer.bg_obj_scanline.0.as_mut_ptr().add(i);
        let window_ptr = renderer.window.0.as_ptr().add(i) as *const u32;

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
            if R::IS_A {
                vram.a_bg.as_ptr()
            } else {
                vram.b_bg.as_ptr()
            } as *const i64,
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
                if R::IS_A {
                    vram.a_bg.as_ptr()
                } else {
                    vram.b_bg.as_ptr()
                } as *const i64,
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
                    palette as *const i64,
                    color_indices,
                    modify_mask,
                )),
                pixel_attrs,
            ),
            _mm256_slli_epi64::<32>(_mm256_maskload_epi64(
                scanline_pixels_ptr as *const i64,
                modify_mask,
            )),
        );
        _mm256_maskstore_epi64(scanline_pixels_ptr as *mut i64, modify_mask, new_pixels);

        x = _mm256_add_epi64(x, x_incr);
        y = _mm256_add_epi64(y, y_incr);
    }

    data.set_affine_bg_pos(
        bg_index,
        [
            affine.pos()[0].wrapping_add(affine.params[1] as i32),
            affine.pos()[1].wrapping_add(affine.params[3] as i32),
        ],
    );
}

#[allow(clippy::similar_names)]
#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn render_scanline_bg_large<R: Role, const DISPLAY_AREA_OVERFLOW: bool>(
    renderer: &mut Renderer<R>,
    data: &mut Data,
    vram: &Vram,
) {
    let bg_control = data.bgs()[2].control();

    let zero = _mm256_setzero_si256();

    let pixel_attrs = _mm256_set1_epi64x(BgObjPixel(0).with_color_effects_mask(1 << 2).0 as i64);
    let addr_mask = _mm256_set1_epi64x(if R::IS_A { 0x7_FFFF } else { 0x1_FFFF });

    let affine = &data.affine_bg_data()[0];
    let affine_params = [affine.params[0] as i64, affine.params[2] as i64];
    let mut x = _mm256_add_epi64(
        _mm256_set_epi64x(
            affine_params[0] * 3,
            affine_params[0] * 2,
            affine_params[0],
            0,
        ),
        _mm256_set1_epi64x(affine.pos()[0] as i64),
    );
    let x_incr = _mm256_set1_epi64x(affine_params[0] << 2);
    let mut y = _mm256_add_epi64(
        _mm256_set_epi64x(
            affine_params[1] * 3,
            affine_params[1] * 2,
            affine_params[1],
            0,
        ),
        _mm256_set1_epi64x(affine.pos()[1] as i64),
    );
    let y_incr = _mm256_set1_epi64x(affine_params[1] << 2);

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
    let palette = (vram.palette.as_ptr() as *const u16).add((!R::IS_A as usize) << 9);

    for i in (0..SCREEN_WIDTH).step_by(4) {
        let scanline_pixels_ptr = renderer.bg_obj_scanline.0.as_mut_ptr().add(i);
        let window_ptr = renderer.window.0.as_ptr().add(i) as *const u32;

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
                if R::IS_A {
                    vram.a_bg.as_ptr()
                } else {
                    vram.b_bg.as_ptr()
                } as *const i64,
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
                    color_indices,
                    modify_mask,
                )),
                pixel_attrs,
            ),
            _mm256_slli_epi64::<32>(_mm256_maskload_epi64(
                scanline_pixels_ptr as *const i64,
                modify_mask,
            )),
        );
        _mm256_maskstore_epi64(scanline_pixels_ptr as *mut i64, modify_mask, new_pixels);

        x = _mm256_add_epi64(x, x_incr);
        y = _mm256_add_epi64(y, y_incr);
    }

    data.set_affine_bg_pos(
        AffineBgIndex::new(0),
        [
            affine.pos()[0].wrapping_add(affine.params[1] as i32),
            affine.pos()[1].wrapping_add(affine.params[3] as i32),
        ],
    );
}

#[allow(clippy::similar_names)]
#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn render_scanline_bg_extended<R: Role, const DISPLAY_AREA_OVERFLOW: bool>(
    renderer: &mut Renderer<R>,
    bg_index: AffineBgIndex,
    data: &mut Data,
    vram: &Vram,
) {
    let bg_control = data.bgs()[bg_index.get() as usize | 2].control();

    let zero = _mm256_setzero_si256();

    let bg_mask = 4 << bg_index.get();
    let pixel_attrs = _mm256_set1_epi64x(BgObjPixel(0).with_color_effects_mask(bg_mask).0 as i64);
    let window_mask_shift = _mm_set_epi64x(0, (61 - bg_index.get()) as i64);
    let addr_mask = _mm256_set1_epi64x(if R::IS_A { 0x7_FFFF } else { 0x1_FFFF });

    let affine = &data.affine_bg_data()[bg_index.get() as usize];
    let affine_params = [affine.params[0] as i64, affine.params[2] as i64];
    let mut x = _mm256_add_epi64(
        _mm256_set_epi64x(
            affine_params[0] * 3,
            affine_params[0] * 2,
            affine_params[0],
            0,
        ),
        _mm256_set1_epi64x(affine.pos()[0] as i64),
    );
    let x_incr = _mm256_set1_epi64x(affine_params[0] << 2);
    let mut y = _mm256_add_epi64(
        _mm256_set_epi64x(
            affine_params[1] * 3,
            affine_params[1] * 2,
            affine_params[1],
            0,
        ),
        _mm256_set1_epi64x(affine.pos()[1] as i64),
    );
    let y_incr = _mm256_set1_epi64x(affine_params[1] << 2);

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
                let scanline_pixels_ptr = renderer.bg_obj_scanline.0.as_mut_ptr().add(i);
                let window_ptr = renderer.window.0.as_ptr().add(i) as *const u32;

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
                    if R::IS_A {
                        vram.a_bg.as_ptr()
                    } else {
                        vram.b_bg.as_ptr()
                    } as *const i64,
                    pixel_addrs,
                    modify_mask,
                );

                modify_mask = _mm256_and_si256(modify_mask, _mm256_slli_epi64(raw_colors, 32));

                let new_pixels = _mm256_or_si256(
                    _mm256_or_si256(rgb5_to_rgb6(raw_colors), pixel_attrs),
                    _mm256_slli_epi64::<32>(_mm256_maskload_epi64(
                        scanline_pixels_ptr as *const i64,
                        modify_mask,
                    )),
                );
                _mm256_maskstore_epi64(scanline_pixels_ptr as *mut i64, modify_mask, new_pixels);

                x = _mm256_add_epi64(x, x_incr);
                y = _mm256_add_epi64(y, y_incr);
            }
        } else {
            let color_indices_mask = _mm256_set1_epi64x(0xFF);
            let palette = (vram.palette.as_ptr() as *const u16).add((!R::IS_A as usize) << 9);
            for i in (0..SCREEN_WIDTH).step_by(4) {
                let scanline_pixels_ptr = renderer.bg_obj_scanline.0.as_mut_ptr().add(i);
                let window_ptr = renderer.window.0.as_ptr().add(i) as *const u32;

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
                        if R::IS_A {
                            vram.a_bg.as_ptr()
                        } else {
                            vram.b_bg.as_ptr()
                        } as *const i64,
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
                            palette as *const i64,
                            color_indices,
                            modify_mask,
                        )),
                        pixel_attrs,
                    ),
                    _mm256_slli_epi64::<32>(_mm256_maskload_epi64(
                        scanline_pixels_ptr as *const i64,
                        modify_mask,
                    )),
                );
                _mm256_maskstore_epi64(scanline_pixels_ptr as *mut i64, modify_mask, new_pixels);

                x = _mm256_add_epi64(x, x_incr);
                y = _mm256_add_epi64(y, y_incr);
            }
        }
    } else {
        let map_base = _mm256_set1_epi64x(if R::IS_A {
            data.control().a_map_base() | bg_control.map_base()
        } else {
            bg_control.map_base()
        } as i64);
        let tile_base = _mm256_set1_epi64x(if R::IS_A {
            data.control().a_tile_base() + bg_control.tile_base()
        } else {
            bg_control.tile_base()
        } as i64);

        let display_area_overflow_mask =
            _mm256_set1_epi64x(!((0x8000 << bg_control.size_key()) - 1));

        let map_row_shift = 4 + bg_control.size_key();
        let pos_map_mask = _mm256_set1_epi64x(((1 << map_row_shift) - 1) << 11);
        let pos_y_to_map_y_shift = _mm256_set1_epi64x((10 - map_row_shift) as i64);
        let x_offset_mask = _mm256_set1_epi64x(7);
        let y_offset_mask = _mm256_set1_epi64x(0x38);
        let color_indices_mask = _mm256_set1_epi64x(0xFF);
        let tile_number_mask = _mm256_set1_epi64x(0x3FF);

        let (palette, pal_base_mask) = if data.control().bg_ext_pal_enabled() {
            (
                if R::IS_A {
                    vram.a_bg_ext_pal.as_ptr()
                } else {
                    vram.b_bg_ext_pal_ptr
                }
                .add((bg_index.get() as usize | 2) << 13) as *const u16,
                0xF00,
            )
        } else {
            (
                vram.palette.as_ptr().add((!R::IS_A as usize) << 10) as *const u16,
                0,
            )
        };
        let pal_base_mask = _mm256_set1_epi64x(pal_base_mask);

        for i in (0..SCREEN_WIDTH).step_by(4) {
            let scanline_pixels_ptr = renderer.bg_obj_scanline.0.as_mut_ptr().add(i);
            let window_ptr = renderer.window.0.as_ptr().add(i) as *const u32;

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

            let tile_addrs = _mm256_add_epi64(
                map_base,
                _mm256_or_si256(
                    _mm256_srlv_epi64(_mm256_and_si256(y, pos_map_mask), pos_y_to_map_y_shift),
                    _mm256_srli_epi64::<10>(_mm256_and_si256(x, pos_map_mask)),
                ),
            );
            let tiles = _mm256_mask_i64gather_epi64::<1>(
                zero,
                if R::IS_A {
                    vram.a_bg.as_ptr()
                } else {
                    vram.b_bg.as_ptr()
                } as *const i64,
                tile_addrs,
                modify_mask,
            );

            let x_offsets = _mm256_and_si256(
                _mm256_srli_epi64::<8>(_mm256_castpd_si256(_mm256_blendv_pd(
                    _mm256_castsi256_pd(x),
                    _mm256_castsi256_pd(_mm256_xor_si256(x, x_offset_mask)),
                    _mm256_castsi256_pd(_mm256_slli_epi64::<53>(tiles)),
                ))),
                x_offset_mask,
            );
            let y_offsets = _mm256_and_si256(
                _mm256_srli_epi64::<5>(_mm256_castpd_si256(_mm256_blendv_pd(
                    _mm256_castsi256_pd(y),
                    _mm256_castsi256_pd(_mm256_xor_si256(y, y_offset_mask)),
                    _mm256_castsi256_pd(_mm256_slli_epi64::<52>(tiles)),
                ))),
                y_offset_mask,
            );

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
                    if R::IS_A {
                        vram.a_bg.as_ptr()
                    } else {
                        vram.b_bg.as_ptr()
                    } as *const i64,
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
                    scanline_pixels_ptr as *const i64,
                    modify_mask,
                )),
            );
            _mm256_maskstore_epi64(scanline_pixels_ptr as *mut i64, modify_mask, new_pixels);

            x = _mm256_add_epi64(x, x_incr);
            y = _mm256_add_epi64(y, y_incr);
        }
    }

    data.set_affine_bg_pos(
        bg_index,
        [
            affine.pos()[0].wrapping_add(affine.params[1] as i32),
            affine.pos()[1].wrapping_add(affine.params[3] as i32),
        ],
    );
}
