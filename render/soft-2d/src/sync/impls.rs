use super::Buffers;
use crate::common::{BgObjPixel, Buffers as BuffersTrait, ObjPixel, WindowPixel};
use dust_core::gpu::{Scanline, SCREEN_WIDTH};

impl BuffersTrait for Buffers {
    unsafe fn obj_window(&self) -> &mut [u8; SCREEN_WIDTH / 8] {
        &mut *self.obj_window.get()
    }

    unsafe fn obj_scanline(&self) -> &mut Scanline<ObjPixel> {
        &mut *self.obj_scanline.get()
    }

    unsafe fn window(&self) -> &mut Scanline<WindowPixel, { SCREEN_WIDTH + 7 }> {
        &mut *self.window.get()
    }

    unsafe fn bg_obj_scanline(&self) -> &mut Scanline<BgObjPixel> {
        &mut *self.bg_obj_scanline.get()
    }
}
