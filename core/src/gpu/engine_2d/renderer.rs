use super::{
    super::{vram::Vram, Framebuffer},
    Engine2d, EngineA, EngineB,
};

pub trait Renderer {
    fn uses_bg_obj_vram_tracking(&self) -> bool;
    fn uses_lcdc_vram_tracking(&self) -> bool;

    fn framebuffer(&self) -> &Framebuffer;

    fn start_prerendering_objs(
        &mut self,
        engines: (&mut Engine2d<EngineA>, &mut Engine2d<EngineB>),
        vram: &mut Vram,
    );
    fn start_scanline(
        &mut self,
        line: u8,
        vcount: u8,
        engines: (&mut Engine2d<EngineA>, &mut Engine2d<EngineB>),
        vram: &mut Vram,
    );
    fn finish_scanline(
        &mut self,
        line: u8,
        vcount: u8,
        engines: (&mut Engine2d<EngineA>, &mut Engine2d<EngineB>),
        vram: &mut Vram,
    );
}
