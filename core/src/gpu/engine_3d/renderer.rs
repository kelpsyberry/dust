use super::{Polygon, RenderingState, Vertex};
use crate::{gpu::Scanline, utils::Bytes};

pub trait Renderer {
    fn swap_buffers(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        vert_ram: &[Vertex],
        poly_ram: &[Polygon],
        state: &RenderingState,
    );

    fn start_frame(&mut self);

    fn read_scanline(&mut self) -> &Scanline<u32, 512>;
    fn skip_scanline(&mut self);
}
