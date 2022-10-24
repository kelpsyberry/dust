pub mod bgs;

pub use dust_soft_2d_base::render::*;

use super::BgObjPixel;
use dust_core::gpu::{
    self,
    engine_2d::{Engine2d, Role},
    Scanline,
};
use dust_soft_2d_base::rgb5_to_rgb6_64;

pub fn render_scanline_vram_display<R: Role>(
    scanline_buffer: &mut Scanline<BgObjPixel>,
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
            *pixel = BgObjPixel(rgb5_to_rgb6_64(src));
        }
    } else {
        scanline_buffer.0.fill(BgObjPixel(0));
    }
}
