use crate::{Buffers, RenderingData};
use core::{
    arch::x86_64::*,
    mem::transmute,
    simd::{u32x8, u64x4},
};
use dust_core::gpu::{Scanline, SCREEN_WIDTH};

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
pub unsafe fn apply_color_effects<B: Buffers, D: RenderingData, const EFFECT: u8>(
    buffers: &mut B,
    data: &D,
) {
    let zero = _mm256_setzero_si256();

    let color_effects_control = data.color_effects_control();
    let blend_coeffs = data.blend_coeffs();
    let target_1_mask = _mm256_set1_epi64x(color_effects_control.target_1_mask() as i64);
    let target_2_mask = _mm256_set1_epi64x(color_effects_control.target_2_mask() as i64);
    let coeff_a = _mm256_set1_epi64x(blend_coeffs.0 as i64);
    let coeff_b = _mm256_set1_epi64x(blend_coeffs.1 as i64);
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

    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

    for i in (0..SCREEN_WIDTH).step_by(4) {
        let scanline_ptr = scanline.0.as_mut_ptr().add(i);
        let window_ptr = window.0.as_ptr().add(i) as *const u32;

        let modify_mask = _mm256_slli_epi64::<58>(_mm256_cvtepu8_epi64(_mm_set_epi64x(
            0,
            window_ptr.read() as i64,
        )));

        let pixels = _mm256_maskload_epi64(scanline_ptr as *const i64, modify_mask);
        let bot_matches_inv = _mm256_cmpeq_epi64(
            _mm256_and_si256(_mm256_srli_epi64::<58>(pixels), target_2_mask),
            zero,
        );

        let custom_alphas = _mm256_and_si256(_mm256_srli_epi64::<18>(pixels), alpha_mask);

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
                _mm256_slli_epi64::<40>(pixels),
            )),
        ));
        _mm256_maskstore_epi64(scanline_ptr as *mut i64, modify_mask, new_pixels);
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
pub unsafe fn apply_brightness<D: RenderingData>(scanline_buffer: &mut Scanline<u32>, data: &D) {
    let mode = data.master_brightness_control().mode();
    let brightness_factor = data.master_brightness_factor();

    if matches!(mode, 1 | 2) && brightness_factor != 0 {
        let brightness_in_rb_mask = _mm256_set1_epi32(0x3_F03F);
        let brightness_in_g_mask = _mm256_set1_epi32(0xFC0);
        let brightness_out_rb_mask = _mm256_set1_epi32(0x3F_03F0);
        let brightness_out_g_mask = _mm256_set1_epi32(0xFC00);
        let brightness_coeff = _mm256_set1_epi32(brightness_factor as i32);

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
                let scanline_ptr = scanline_buffer.0.as_mut_ptr().add(i) as *mut __m256i;
                let pixels = _mm256_load_si256(scanline_ptr);
                let offset = offset!(_mm256_xor_si256(brightness_incr_max, pixels));
                _mm256_store_si256(
                    scanline_ptr,
                    rgb6_to_rgba8(_mm256_add_epi32(pixels, offset)),
                );
            }
        } else {
            for i in (0..SCREEN_WIDTH).step_by(8) {
                let scanline_ptr = scanline_buffer.0.as_mut_ptr().add(i) as *mut __m256i;
                let pixels = _mm256_load_si256(scanline_ptr);
                let offset = offset!(pixels);
                _mm256_store_si256(
                    scanline_ptr,
                    rgb6_to_rgba8(_mm256_sub_epi32(pixels, offset)),
                );
            }
        }
    } else {
        for i in (0..SCREEN_WIDTH).step_by(8) {
            let scanline_ptr = scanline_buffer.0.as_mut_ptr().add(i) as *mut __m256i;
            _mm256_store_si256(scanline_ptr, rgb6_to_rgba8(_mm256_load_si256(scanline_ptr)));
        }
    }
}
