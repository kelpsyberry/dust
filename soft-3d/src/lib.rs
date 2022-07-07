#![feature(portable_simd, const_mut_refs, const_trait_impl)]

mod render;
pub use render::Renderer as RawRenderer;

use dust_core::{
    gpu::{
        engine_3d::RenderingState,
        engine_3d::{Polygon, RenderingControl, ScreenVertex},
    },
    utils::{Bytes, Zero},
};

pub struct RenderingData {
    pub control: RenderingControl,
    pub texture: Bytes<0x8_0000>,
    // TODO: How is the texture palette region mirrored?
    pub tex_pal: Bytes<0x2_0000>,
    pub vert_ram: [ScreenVertex; 6144],
    pub poly_ram: [Polygon; 2048],
    pub poly_ram_level: u16,
    pub w_buffering: bool,
}

unsafe impl Zero for RenderingData {}

impl RenderingData {
    #[inline]
    pub fn copy_texture_data(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        state: &RenderingState,
    ) {
        for i in 0..4 {
            if state.texture_dirty & 1 << i == 0 {
                continue;
            }
            let range = i << 17..(i + 1) << 17;
            self.texture[range.clone()].copy_from_slice(&texture[range]);
        }
        for i in 0..6 {
            if state.tex_pal_dirty & 1 << i == 0 {
                continue;
            }
            let range = i << 14..(i + 1) << 14;
            self.tex_pal[range.clone()].copy_from_slice(&tex_pal[range]);
        }
    }
}
