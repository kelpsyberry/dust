use super::{Polygon, RenderingState, ScreenVertex};
use crate::{gpu::Scanline, utils::Bytes};

pub trait RendererTx {
    fn swap_buffers(
        &mut self,
        vert_ram: &[ScreenVertex],
        poly_ram: &[Polygon],
        state: &RenderingState,
    );
    fn repeat_last_frame(&mut self, state: &RenderingState);
    fn start_rendering(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        state: &RenderingState,
    );
}

pub trait RendererRx {
    fn start_frame(&mut self);
    fn read_scanline(&mut self) -> &Scanline<u32, 256>;
    fn skip_scanline(&mut self);
}
