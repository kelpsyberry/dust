use super::utils::expand_depth;
use dust_core::{
    gpu::engine_3d::{Color, Polygon, RenderingControl, RenderingState, ScreenVertex},
    utils::Bytes,
};

pub struct GxData {
    pub w_buffering: bool,

    pub vert_ram: [ScreenVertex; 6144],
    pub vert_ram_level: u16,

    pub poly_ram: [Polygon; 2048],
    pub poly_ram_level: u16,
}

impl GxData {
    pub fn prepare(
        &mut self,
        vert_ram: &[ScreenVertex],
        poly_ram: &[Polygon],
        state: &RenderingState,
    ) {
        self.w_buffering = state.w_buffering;

        self.vert_ram[..vert_ram.len()].copy_from_slice(vert_ram);
        self.vert_ram_level = vert_ram.len() as u16;
        self.poly_ram[..poly_ram.len()].copy_from_slice(poly_ram);
        self.poly_ram_level = poly_ram.len() as u16;
    }

    pub fn copy_from(&mut self, other: &Self) {
        self.w_buffering = other.w_buffering;

        self.vert_ram_level = other.vert_ram_level;
        self.vert_ram[..self.vert_ram_level as usize]
            .copy_from_slice(&other.vert_ram[..self.vert_ram_level as usize]);
        self.poly_ram_level = other.poly_ram_level;
        self.poly_ram[..self.poly_ram_level as usize]
            .copy_from_slice(&other.poly_ram[..self.poly_ram_level as usize]);
    }
}

#[repr(C)]
pub struct RenderingData {
    pub control: RenderingControl,

    pub alpha_test_ref: u8,

    pub clear_poly_id: u8,
    pub clear_image_offset: [u8; 2],
    pub clear_depth: u32,

    pub fog_offset: u16,
    pub fog_densities: [u8; 0x20],
    pub rear_plane_fog_enabled: bool,

    pub clear_color: Color,
    pub fog_color: Color,
    pub edge_colors: [Color; 8],
    pub toon_colors: [Color; 0x20],

    pub texture: Bytes<0x8_0000>,
    pub tex_pal: Bytes<0x2_0000>,

    pub texture_dirty: u8,
    pub tex_pal_dirty: u8,
}

impl RenderingData {
    #[inline]
    pub fn prepare(&mut self, state: &RenderingState) {
        self.control = state.control;

        self.alpha_test_ref = if state.control.alpha_test_enabled() {
            state.alpha_test_ref
        } else {
            0
        };

        self.clear_color = state.clear_color;
        self.clear_poly_id = state.clear_poly_id;
        self.clear_depth = expand_depth(state.clear_depth);
        self.clear_image_offset = state.clear_image_offset;

        self.toon_colors = state.toon_colors;
        self.edge_colors = state.edge_colors;

        self.fog_color = state.fog_color;
        self.fog_densities = state.fog_densities;
        self.fog_offset = state.fog_offset;
        self.rear_plane_fog_enabled = state.rear_plane_fog_enabled;
    }

    pub fn copy_vram(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        texture_dirty: u8,
        tex_pal_dirty: u8,
    ) {
        for i in 0..4 {
            if texture_dirty & 1 << i == 0 {
                continue;
            }
            let range = i << 17..(i + 1) << 17;
            self.texture[range.clone()].copy_from_slice(&texture[range]);
        }

        for i in 0..6 {
            if tex_pal_dirty & 1 << i == 0 {
                continue;
            }
            let range = i << 14..(i + 1) << 14;
            self.tex_pal[range.clone()].copy_from_slice(&tex_pal[range]);
        }

        self.texture_dirty = texture_dirty;
        self.tex_pal_dirty = tex_pal_dirty;
    }
}

#[repr(C)]
pub struct FrameData {
    pub rendering: RenderingData,
    pub gx: GxData,
}
