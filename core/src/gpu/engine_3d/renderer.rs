use super::{Polygon, RenderingState, ScreenVertex};
use crate::{gpu::Scanline, utils::Bytes};

pub trait RendererTx {
    fn set_capture_enabled(&mut self, capture_enabled: bool);
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
    fn skip_rendering(&mut self);
}

// TODO: Use the SCREEN_WIDTH/SCREEN_HEIGHT constants, can't right now due to a compiler bug making
//       the trait stop being object-safe in that case.

pub trait SoftRendererRx {
    fn start_frame(&mut self);
    fn read_scanline(&mut self) -> &Scanline<u32>;
    fn skip_scanline(&mut self);
}

pub trait AccelRendererRx {
    fn start_frame(&mut self, capture_enabled: bool);
    fn read_capture_scanline(&mut self) -> &Scanline<u32>;
}
