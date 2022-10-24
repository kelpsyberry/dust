pub mod bgs;
pub mod effects;
pub mod objs;

use super::rgb5_to_rgb6;
use dust_core::gpu::{
    self,
    engine_2d::{Engine2d, Role},
    Scanline,
};

#[macro_export]
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
