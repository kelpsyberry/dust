use dust_core::{
    gpu::{
        engine_3d::{Polygon, Renderer as RendererTrair, ScreenVertex},
        Scanline,
    },
    utils::{zeroed_box, Bytes},
};

pub struct Renderer {
    scanline: Box<Scanline<u32, 512>>,
}

impl RendererTrair for Renderer {
    fn swap_buffers(
        &mut self,
        _texture: &Bytes<0x8_0000>,
        _tex_pal: &Bytes<0x1_8000>,
        _vert_ram: &[ScreenVertex],
        _poly_ram: &[Polygon],
        _state: &dust_core::gpu::engine_3d::RenderingState,
    ) {
    }

    fn repeat_last_frame(
        &mut self,
        _texture: &Bytes<0x8_0000>,
        _tex_pal: &Bytes<0x1_8000>,
        _state: &dust_core::gpu::engine_3d::RenderingState,
    ) {
    }

    fn start_frame(&mut self) {}

    fn read_scanline(&mut self) -> &Scanline<u32, 512> {
        &self.scanline
    }

    fn skip_scanline(&mut self) {}
}

impl Renderer {
    pub fn new() -> Self {
        Renderer {
            scanline: zeroed_box(),
        }
    }
}
