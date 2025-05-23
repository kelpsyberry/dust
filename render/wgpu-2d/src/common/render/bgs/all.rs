use dust_soft_2d_base::render::bgs::all::*;

use dust_core::{
    gpu::{
        engine_2d::{AffineBgIndex, BgIndex, Role},
        SCREEN_WIDTH,
    },
    utils::mem_prelude::*,
};
use dust_soft_2d_base::{rgb5_to_rgb6_64, BgObjPixel, Buffers, RenderingData, Vram};

pub fn render_scanline_bgs_and_objs<
    R: Role,
    B: Buffers,
    D: RenderingData,
    V: Vram<R, BG_VRAM_LEN, OBJ_VRAM_LEN>,
    const BG_VRAM_LEN: usize,
    const OBJ_VRAM_LEN: usize,
    const BG_MODE: u8,
>(
    buffers: &B,
    vcount: u8,
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
            render_scanline_bg_text(buffers, BgIndex::new(1), vcount, data, vram);
        }

        if data.bg_priority(BgIndex::new(0)) == priority {
            if R::IS_A && data.control().bg0_3d() {
                if data.engine_3d_enabled_in_frame() {
                    render_scanline_bg_3d(buffers);
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

pub fn render_scanline_bg_3d<B: Buffers>(buffers: &B) {
    // TODO: 3D layer scrolling

    let pixel_attrs = BgObjPixel(0).with_color_effects_mask(1).with_is_3d(true);

    let scanline = unsafe { buffers.bg_obj_scanline() };
    let window = unsafe { buffers.window() };

    for i in 0..SCREEN_WIDTH {
        if window.0[i].0 & 1 != 0 {
            scanline.0[i].0 = scanline.0[i].0 << 32 | pixel_attrs.0;
        }
    }
}
