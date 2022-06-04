use super::{Polygon, RenderingState, ScreenVertex};
use crate::{gpu::Scanline, utils::Bytes};

pub trait Renderer {
    fn swap_buffers(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        vert_ram: &[ScreenVertex],
        poly_ram: &[Polygon],
        state: &RenderingState,
        w_buffering: bool,
    );
    fn repeat_last_frame(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        state: &RenderingState,
    );

    fn start_frame(&mut self);

    fn read_scanline(&mut self) -> &Scanline<u32, 512>;
    fn skip_scanline(&mut self);
}
