pub mod bgs;
pub mod effects;
pub mod objs;

use super::{rgb5_to_rgb6, Buffers, RenderingData, Vram};
use core::marker::PhantomData;
use dust_core::gpu::{
    self,
    engine_2d::{Engine2d, Role},
    Scanline,
};

macro_rules! render_fn_ptr {
    ($group: ident::$ident: ident $($generics: tt)*) => {
        'get_fn_ptr: {
            #[cfg(target_arch = "x86_64")]
            if is_x86_feature_detected!("avx2") {
                break 'get_fn_ptr $group::avx2::$ident$($generics)*;
            }
            $group::all::$ident$($generics)*
        }
    };
}

#[allow(clippy::type_complexity)]
pub struct FnPtrs<R: Role, B: Buffers, D: RenderingData, V: Vram<R>> {
    pub apply_color_effects: [unsafe fn(&mut B, &D); 4],
    pub apply_brightness: unsafe fn(scanline_buffer: &mut Scanline<u32>, &D),
    pub render_scanline_bgs_and_objs:
        [unsafe fn(&mut B, vcount: u8, &mut D, &V, scanline_3d: Option<&Scanline<u32>>); 8],
    _marker: PhantomData<R>,
}

impl<R: Role, B: Buffers, D: RenderingData, V: Vram<R>> FnPtrs<R, B, D, V> {
    #[allow(unused_labels)]
    pub fn new() -> Self
    where
        [(); R::BG_VRAM_LEN]: Sized,
    {
        FnPtrs {
            apply_color_effects: [
                render_fn_ptr!(effects::apply_color_effects::<B, D, 0>),
                render_fn_ptr!(effects::apply_color_effects::<B, D, 1>),
                render_fn_ptr!(effects::apply_color_effects::<B, D, 2>),
                render_fn_ptr!(effects::apply_color_effects::<B, D, 3>),
            ],
            apply_brightness: render_fn_ptr!(effects::apply_brightness::<D>),
            render_scanline_bgs_and_objs: [
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 0>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 1>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 2>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 3>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 4>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 5>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 6>),
                render_fn_ptr!(bgs::render_scanline_bgs_and_objs::<R, B, D, V, 7>),
            ],
            _marker: ::core::marker::PhantomData,
        }
    }
}

pub fn render_scanline_vram_display<R: Role>(
    scanline_buffer: &mut Scanline<u32>,
    vcount: u8,
    engine: &Engine2d<R>,
    vram: &gpu::vram::Vram,
) {
    // The bank must be mapped as LCDC VRAM to be used
    let bank_index = engine.control().a_vram_bank();
    let bank_control = vram.bank_control()[bank_index as usize];
    if bank_control.enabled() && bank_control.mst() == 0 {
        let bank = match bank_index {
            0 => &vram.banks.a,
            1 => &vram.banks.b,
            2 => &vram.banks.c,
            _ => &vram.banks.d,
        };
        let line_base = (vcount as usize) << 9;
        for (i, pixel) in scanline_buffer.0.iter_mut().enumerate() {
            let src = unsafe { bank.read_le_aligned_unchecked::<u16>(line_base | i << 1) };
            *pixel = rgb5_to_rgb6(src);
        }
    } else {
        scanline_buffer.0.fill(0);
    }
}
