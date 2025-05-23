// TODO: Possibly migrate to core::simd when masked loads/stores are supported

use dust_soft_2d_base::render::bgs::avx2::*;

use core::arch::x86_64::*;
use dust_core::{
    gpu::{
        engine_2d::{AffineBgIndex, BgIndex, Role},
        SCREEN_WIDTH,
    },
    utils::mem_prelude::*,
};
use dust_soft_2d_base::{BgObjPixel, Buffers, RenderingData, Vram};

#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn render_scanline_bgs_and_objs<
    R: Role,
    B: Buffers,
    D: RenderingData,
    V: Vram<R, BG_VRAM_LEN, OBJ_VRAM_LEN>,
    const BG_VRAM_LEN: usize,
    const OBJ_VRAM_LEN: usize,
    const BG_MODE: u8,
>(
    buffers: &B,
    vount: u8,
    data: &mut D,
    vram: &V,
) {
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
        render_scanline_bg_affine::<_, _, _, _, BG_VRAM_LEN, OBJ_VRAM_LEN, false>,
        render_scanline_bg_affine::<_, _, _, _, BG_VRAM_LEN, OBJ_VRAM_LEN, true>,
    ];

    let render_scanline_bg_extended = [
        render_scanline_bg_extended::<_, _, _, _, BG_VRAM_LEN, OBJ_VRAM_LEN, false>,
        render_scanline_bg_extended::<_, _, _, _, BG_VRAM_LEN, OBJ_VRAM_LEN, true>,
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
                        render_scanline_bg_large::<_, _, _, _, BG_VRAM_LEN, OBJ_VRAM_LEN, false>,
                        render_scanline_bg_large::<_, _, _, _, BG_VRAM_LEN, OBJ_VRAM_LEN, true>,
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
                render_scanline_bg_3d(buffers);
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
                    crate::common::rgb5_to_rgb6_64(if obj_pixel.use_raw_color() {
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

#[target_feature(enable = "sse4.1,sse4.2,avx,avx2")]
pub unsafe fn render_scanline_bg_3d<B: Buffers>(buffers: &B) {
    // TODO: 3D layer scrolling

    let pixel_attrs =
        _mm256_set1_epi64x(BgObjPixel(0).with_color_effects_mask(1).with_is_3d(true).0 as i64);

    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

    for i in (0..SCREEN_WIDTH).step_by(4) {
        let scanline_ptr = scanline.0.as_mut_ptr().add(i);
        let window_ptr = window.0.as_ptr().add(i) as *const u32;

        let modify_mask = _mm256_slli_epi64::<63>(_mm256_cvtepu8_epi64(_mm_set_epi64x(
            0,
            window_ptr.read() as i64,
        )));

        let new_pixels = _mm256_or_si256(
            pixel_attrs,
            _mm256_slli_epi64::<32>(_mm256_maskload_epi64(
                scanline_ptr as *const i64,
                modify_mask,
            )),
        );
        _mm256_maskstore_epi64(scanline_ptr as *mut i64, modify_mask, new_pixels);
    }
}
