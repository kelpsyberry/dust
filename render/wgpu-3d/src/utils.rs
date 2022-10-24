use core::simd::f64x4;
use dust_core::gpu::engine_3d::Color;

#[inline]
pub fn round_up_to_alignment(size: usize, alignment: usize) -> usize {
    (size + alignment - 1) / alignment * alignment
}

#[inline]
pub fn expand_depth(depth: u16) -> u32 {
    let depth = depth as u32;
    depth << 9 | ((depth.wrapping_add(1) as i32) << 16 >> 31 & 0x1FF) as u32
}

#[inline]
pub fn decode_rgb5(color: u16, alpha: u8) -> u32 {
    let r = color & 0x1F;
    let g = (color >> 5) & 0x1F;
    let b = (color >> 10) & 0x1F;
    (alpha as u32) << 24 | (b as u32) << 16 | (g as u32) << 8 | r as u32
}

#[inline]
pub fn color_to_wgpu_f64(color: Color) -> wgpu::Color {
    let [r, g, b, a] = (color.cast::<f64>() * f64x4::splat(1.0 / 31.0)).to_array();
    wgpu::Color { r, g, b, a }
}
