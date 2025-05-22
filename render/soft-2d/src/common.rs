pub use dust_soft_2d_base::*;

use core::marker::PhantomData;
use dust_core::gpu::{engine_2d::Role, Scanline};
use render::{bgs, effects};

#[allow(clippy::type_complexity)]
pub struct FnPtrs<
    R: Role,
    B: Buffers,
    D: RenderingData,
    V: Vram<R, BG_VRAM_LEN, OBJ_VRAM_LEN>,
    const BG_VRAM_LEN: usize,
    const OBJ_VRAM_LEN: usize,
> {
    pub apply_color_effects: [unsafe fn(&B, &D); 4],
    pub apply_brightness: unsafe fn(scanline_buffer: &mut Scanline<u32>, &D),
    pub render_scanline_bgs_and_objs:
        [unsafe fn(&B, vcount: u8, &mut D, &V, scanline_3d: Option<&Scanline<u32>>); 8],
    _marker: PhantomData<R>,
}

impl<
        R: Role,
        B: Buffers,
        D: RenderingData,
        V: Vram<R, BG_VRAM_LEN, OBJ_VRAM_LEN>,
        const BG_VRAM_LEN: usize,
        const OBJ_VRAM_LEN: usize,
    > FnPtrs<R, B, D, V, BG_VRAM_LEN, OBJ_VRAM_LEN>
{
    #[allow(unused_labels)]
    pub fn new() -> Self {
        FnPtrs {
            apply_color_effects: [
                render_fn_ptr!(effects::apply_color_effects::<B, D, 0>),
                render_fn_ptr!(effects::apply_color_effects::<B, D, 1>),
                render_fn_ptr!(effects::apply_color_effects::<B, D, 2>),
                render_fn_ptr!(effects::apply_color_effects::<B, D, 3>),
            ],
            apply_brightness: render_fn_ptr!(effects::apply_brightness::<D>),
            render_scanline_bgs_and_objs: [
                render_fn_ptr!(
                    bgs::render_scanline_bgs_and_objs::<R, B, D, V, BG_VRAM_LEN, OBJ_VRAM_LEN, 0>
                ),
                render_fn_ptr!(
                    bgs::render_scanline_bgs_and_objs::<R, B, D, V, BG_VRAM_LEN, OBJ_VRAM_LEN, 1>
                ),
                render_fn_ptr!(
                    bgs::render_scanline_bgs_and_objs::<R, B, D, V, BG_VRAM_LEN, OBJ_VRAM_LEN, 2>
                ),
                render_fn_ptr!(
                    bgs::render_scanline_bgs_and_objs::<R, B, D, V, BG_VRAM_LEN, OBJ_VRAM_LEN, 3>
                ),
                render_fn_ptr!(
                    bgs::render_scanline_bgs_and_objs::<R, B, D, V, BG_VRAM_LEN, OBJ_VRAM_LEN, 4>
                ),
                render_fn_ptr!(
                    bgs::render_scanline_bgs_and_objs::<R, B, D, V, BG_VRAM_LEN, OBJ_VRAM_LEN, 5>
                ),
                render_fn_ptr!(
                    bgs::render_scanline_bgs_and_objs::<R, B, D, V, BG_VRAM_LEN, OBJ_VRAM_LEN, 6>
                ),
                render_fn_ptr!(
                    bgs::render_scanline_bgs_and_objs::<R, B, D, V, BG_VRAM_LEN, OBJ_VRAM_LEN, 7>
                ),
            ],
            _marker: ::core::marker::PhantomData,
        }
    }
}
