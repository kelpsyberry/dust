use super::{Buffers, RenderingData, Vram};
use crate::common::{
    BgObjPixel, Buffers as BuffersTrait, ObjPixel, RenderingData as RenderingDataTrait,
    Vram as VramTrait, WindowPixel,
};
use dust_core::{
    gpu::{
        engine_2d::{
            AffineBgIndex, BgControl, BgIndex, BrightnessControl, ColorEffectsControl, Control,
            Role,
        },
        Scanline, SCREEN_WIDTH,
    },
    utils::Bytes,
};

impl BuffersTrait for Buffers {
    unsafe fn obj_window(&self) -> &mut [u8; SCREEN_WIDTH / 8] {
        &mut *self.obj_window.get()
    }

    unsafe fn obj_scanline(&self) -> &mut Scanline<ObjPixel> {
        &mut *self.obj_scanline.get()
    }

    unsafe fn bg_obj_scanline(&self) -> &mut Scanline<BgObjPixel> {
        &mut *self.bg_obj_scanline.get()
    }

    unsafe fn window(&self) -> &mut Scanline<WindowPixel, { SCREEN_WIDTH + 7 }> {
        &mut *self.window.get()
    }
}

impl RenderingDataTrait for RenderingData {
    fn control(&self) -> Control {
        self.control
    }

    fn master_brightness_control(&self) -> BrightnessControl {
        self.master_brightness_control
    }

    fn master_brightness_factor(&self) -> u32 {
        self.master_brightness_factor
    }

    fn bg_control(&self, i: BgIndex) -> BgControl {
        self.bgs[i.get() as usize].control
    }

    fn bg_priority(&self, i: BgIndex) -> u8 {
        self.bgs[i.get() as usize].priority
    }

    fn bg_scroll(&self, i: BgIndex) -> [u16; 2] {
        self.bgs[i.get() as usize].scroll
    }

    fn affine_bg_x_incr(&self, i: AffineBgIndex) -> [i16; 2] {
        self.affine_bg_data[i.get() as usize].x_incr
    }

    fn affine_bg_pos(&self, i: AffineBgIndex) -> [i32; 2] {
        self.affine_bg_data[i.get() as usize].pos
    }

    fn increase_affine_bg_pos(&mut self, i: AffineBgIndex) {
        let affine = &mut self.affine_bg_data[i.get() as usize];
        affine.pos = [
            affine.pos[0].wrapping_add(affine.y_incr[0] as i32),
            affine.pos[1].wrapping_add(affine.y_incr[1] as i32),
        ];
    }

    fn color_effects_control(&self) -> ColorEffectsControl {
        self.color_effects_control
    }

    fn blend_coeffs(&self) -> (u8, u8) {
        self.blend_coeffs
    }

    fn brightness_coeff(&self) -> u8 {
        self.brightness_coeff
    }
}

impl<R: Role> VramTrait<R> for Vram<R>
where
    [(); R::BG_VRAM_LEN]: Sized,
    [(); R::OBJ_VRAM_LEN]: Sized,
{
    fn bg(&self) -> &Bytes<{ R::BG_VRAM_LEN }> {
        &self.bg
    }

    fn obj(&self) -> &Bytes<{ R::OBJ_VRAM_LEN }> {
        &self.obj
    }

    fn bg_palette(&self) -> &Bytes<0x206> {
        unsafe { &*(self.palette.as_ptr() as *const Bytes<0x206>) }
    }

    fn obj_palette(&self) -> &Bytes<0x206> {
        unsafe { &*(self.palette.as_ptr().add(0x200) as *const Bytes<0x206>) }
    }

    fn bg_ext_palette(&self) -> &Bytes<0x8006> {
        &self.bg_ext_palette
    }

    fn obj_ext_palette(&self) -> &Bytes<0x2006> {
        &self.obj_ext_palette
    }

    fn oam(&self) -> &Bytes<0x400> {
        &self.oam
    }
}
