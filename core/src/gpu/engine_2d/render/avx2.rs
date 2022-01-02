use super::all::{read_bg_text_tiles, TextTiles};
use super::*;
use core::arch::x86_64::*;

unsafe fn rgb_15_to_18_data() -> [__m256i; 3] {
    [
        _mm256_set1_epi64x(0x3E),
        _mm256_set1_epi64x(0xF80),
        _mm256_set1_epi64x(0x3_E000),
    ]
}

unsafe fn rgb_15_to_18(values: __m256i, data: [__m256i; 3]) -> __m256i {
    _mm256_or_si256(
        _mm256_or_si256(
            _mm256_and_si256(_mm256_slli_epi64::<1>(values), data[0]),
            _mm256_and_si256(_mm256_slli_epi64::<2>(values), data[1]),
        ),
        _mm256_and_si256(_mm256_slli_epi64::<3>(values), data[2]),
    )
}

pub fn render_scanline_bg_text<R: Role>(
    engine: &mut Engine2d<R>,
    bg_index: BgIndex,
    line: u16,
    vram: &Vram,
) {
    let bg = &engine.bgs[bg_index.get() as usize];

    let x_start = bg.scroll[0] as u32;
    let y = bg.scroll[1] as u32 + line as u32;

    let tile_base = if R::IS_A {
        engine.control.a_tile_base() + bg.control.tile_base()
    } else {
        bg.control.tile_base()
    };

    let mut tiles = TextTiles::new_uninit();
    let tiles = read_bg_text_tiles(engine, &mut tiles, bg.control, y, vram);

    let bg_mask = 1 << bg_index.get();
    let pixel_attrs = BgObjPixel(0).with_color_effects_mask(bg_mask);

    let tile_off_mask = tiles.len() - 1;
    let y_in_tile = y & 7;
    let mut x = x_start;
    let mut tile_i = x_start as usize >> 3 & tile_off_mask;

    let zero = unsafe { _mm256_setzero_si256() };
    let ones = unsafe { _mm256_set1_epi64x(-1) };
    let pixel_attrs = unsafe { _mm256_set1_epi64x(pixel_attrs.0 as i64) };
    let conv_data = unsafe { rgb_15_to_18_data() };
    let bg_mask = unsafe { _mm256_set1_epi64x(bg_mask as i64) };

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
                let $tile_ident = unsafe { *tiles.get_unchecked(tile_i) };
                tile_i = (tile_i + 1) & tile_off_mask;

                let y_in_tile = if $tile_ident & 1 << 11 == 0 {
                    y_in_tile
                } else {
                    7 ^ y_in_tile
                };
                let $tile_base_ident = tile_base + (
                    ($tile_ident as u32 & 0x3FF) << (5 + $i_shift) | y_in_tile << (2 + $i_shift)
                );
                unsafe {
                    let palette = $palette;
                    let $remaining_ident = SCREEN_WIDTH - screen_i;
                    let color_indices = $color_indices;
                    let scanline_pixels_ptr = engine.bg_obj_scanline.0.as_mut_ptr().add(screen_i);
                    let window_ptr = engine.window.0.as_ptr().add(screen_i) as *const u32;
                    $(
                        let half_scanline_pixels_ptr = scanline_pixels_ptr.add($i * 4);
                        let half_window_ptr = window_ptr.add($i);
                        let $half_color_indices_ident = color_indices >> ($i << (4 + $i_shift));
                        let half_color_indices = $half_color_indices;
                        let modify_mask = _mm256_xor_si256(
                            _mm256_or_si256(
                                _mm256_cmpeq_epi64(
                                    _mm256_and_si256(
                                        _mm256_cvtepu8_epi64(_mm_set_epi64x(
                                            0,
                                            half_window_ptr.read_unaligned() as i64,
                                        )),
                                        bg_mask,
                                    ),
                                    zero,
                                ),
                                _mm256_cmpeq_epi64(half_color_indices, zero),
                            ),
                            ones,
                        );
                        let new_colors = _mm256_or_si256(
                            _mm256_or_si256(
                                rgb_15_to_18(
                                    _mm256_mask_i64gather_epi64::<2>(
                                        zero,
                                        palette as *const i64,
                                        half_color_indices,
                                        ones,
                                    ),
                                    conv_data,
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
                            new_colors,
                        );
                    )*
                }

                let new_x = (x & !7) + 8;
                screen_i += (new_x - x) as usize;
                x = new_x;
            }
        };
    }

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
        let colors_low_mask = unsafe { _mm_set1_epi64x(0xF) };

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
