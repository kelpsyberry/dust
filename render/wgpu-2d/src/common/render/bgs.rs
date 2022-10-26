pub mod all;
#[cfg(target_arch = "x86_64")]
pub mod avx2;

use crate::common::BgObjPixel;
use dust_core::gpu::Scanline;

pub fn patch_scanline_bg_3d(
    bg_obj_scanline: &mut Scanline<BgObjPixel>,
    scanline_3d: &Scanline<u32>,
) {
    for (i, pixel) in bg_obj_scanline.0.iter_mut().enumerate() {
        let new_pixel = scanline_3d.0[i];
        if pixel.is_3d() {
            if new_pixel >> 18 == 0 {
                pixel.0 >>= 32;
            } else {
                pixel.0 |= new_pixel as u64;
            }
        } else if pixel.bot_is_3d() {
            if new_pixel >> 18 == 0 {
                pixel.0 &= 0xFFFF_FFFF;
            } else {
                pixel.0 |= (new_pixel as u64) << 32;
            }
        }
    }
}
