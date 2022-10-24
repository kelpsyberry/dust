mod impls;

use crate::common::{
    self, capture,
    render::{self, objs::prerender_objs},
    rgb5_to_rgb6_64, BgObjPixel, ObjPixel, WindowPixel,
};
use core::cell::UnsafeCell;
use dust_core::gpu::{
    engine_2d::{Engine2d, EngineA, EngineB, Renderer as RendererTrait, Role},
    engine_3d,
    vram::Vram,
    Framebuffer, Scanline, SCREEN_HEIGHT, SCREEN_WIDTH,
};

struct Buffers {
    obj_window: UnsafeCell<[u8; SCREEN_WIDTH / 8]>,
    obj_scanline: UnsafeCell<Scanline<ObjPixel>>,
    // Allow for slightly out-of-bounds SIMD accesses
    window: UnsafeCell<Scanline<WindowPixel, { SCREEN_WIDTH + 7 }>>,
    bg_obj_scanline: UnsafeCell<Scanline<BgObjPixel>>,
}

type FnPtrs<R> = common::FnPtrs<R, Buffers, Engine2d<R>, Vram>;

pub struct Renderer {
    fns: (FnPtrs<EngineA>, FnPtrs<EngineB>),
    renderer_3d_rx: Box<dyn engine_3d::SoftRendererRx>,
    buffers: [Buffers; 2],
    framebuffer: Box<[[Scanline<u32>; SCREEN_HEIGHT]; 2]>,
}

unsafe impl Send for Renderer {}

impl Renderer {
    pub fn new(renderer_3d_rx: Box<dyn engine_3d::SoftRendererRx>) -> Self {
        macro_rules! buffers {
            () => {
                Buffers {
                    obj_window: UnsafeCell::new([0; SCREEN_WIDTH / 8]),
                    obj_scanline: UnsafeCell::new(Scanline([ObjPixel(0); SCREEN_WIDTH])),
                    window: UnsafeCell::new(Scanline([WindowPixel(0); SCREEN_WIDTH + 7])),
                    bg_obj_scanline: UnsafeCell::new(Scanline([BgObjPixel(0); SCREEN_WIDTH])),
                }
            };
        }

        Renderer {
            fns: (FnPtrs::new(), FnPtrs::new()),
            renderer_3d_rx,
            buffers: [buffers!(), buffers!()],
            framebuffer: unsafe { Box::new_zeroed().assume_init() },
        }
    }

    fn render_scanline<R: Role>(
        &mut self,
        line: u8,
        vcount: u8,
        engine: &mut Engine2d<R>,
        vram: &mut Vram,
    ) where
        [(); R::OBJ_VRAM_LEN]: Sized,
    {
        let fns = unsafe {
            &*(if R::IS_A {
                &self.fns.0 as *const _ as *const ()
            } else {
                &self.fns.1 as *const _ as *const ()
            } as *const FnPtrs<R>)
        };
        let buffers = &mut self.buffers[!R::IS_A as usize];

        let scanline_buffer = unsafe {
            self.framebuffer[engine.is_on_lower_screen() as usize].get_unchecked_mut(line as usize)
        };

        // According to melonDS, if vcount falls outside the drawing range or 2D engine B is
        // disabled, the scanline is filled with pure white.
        if vcount >= SCREEN_HEIGHT as u8 || (!R::IS_A && !engine.is_enabled()) {
            if R::IS_A && engine.engine_3d_enabled_in_frame() {
                self.renderer_3d_rx.skip_scanline();
            }
            // TODO: Display capture interaction?

            scanline_buffer.0.fill(0xFFFF_FFFF);
            return;
        }

        let display_mode = if R::IS_A {
            engine.control().display_mode_a()
        } else {
            engine.control().display_mode_b()
        };

        let render_bg_obj_line = display_mode == 1
            || (R::IS_A
                && engine.capture_enabled_in_frame()
                && !engine.capture_control().src_a_3d_only());

        let scanline_3d = if R::IS_A && engine.engine_3d_enabled_in_frame() {
            let enabled_in_bg_obj = engine.bgs[0].priority() != 4 && engine.control().bg0_3d();
            if (engine.capture_enabled_in_frame()
                && (engine.capture_control().src_a_3d_only() || enabled_in_bg_obj))
                || (display_mode == 1 && enabled_in_bg_obj)
            {
                Some(self.renderer_3d_rx.read_scanline())
            } else {
                self.renderer_3d_rx.skip_scanline();
                None
            }
        } else {
            None
        };

        if render_bg_obj_line {
            let window = buffers.window.get_mut();

            window.0[..SCREEN_WIDTH].fill(WindowPixel(if engine.control().wins_enabled() == 0 {
                0x3F
            } else {
                engine.window_control()[2].0
            }));

            if engine.control().obj_win_enabled() {
                let obj_window_pixel = WindowPixel(engine.window_control()[3].0);
                for (i, window_pixel) in window.0[..SCREEN_WIDTH].iter_mut().enumerate() {
                    if buffers.obj_window.get_mut()[i >> 3] & 1 << (i & 7) != 0 {
                        *window_pixel = obj_window_pixel;
                    }
                }
            }

            for i in (0..2).rev() {
                if !engine.windows_active().0 & 1 << i != 0 {
                    continue;
                }

                let x_range = &engine.window_x_ranges()[i];
                let x_start = x_range.0 as usize;
                let mut x_end = x_range.1 as usize;
                if x_end < x_start {
                    x_end = 256;
                }
                window.0[x_start..x_end].fill(WindowPixel(engine.window_control()[i].0));
            }

            let backdrop = BgObjPixel(rgb5_to_rgb6_64(
                vram.palette.read_le::<u16>((!R::IS_A as usize) << 10),
            ))
            .with_color_effects_mask(1 << 5)
            .0;
            buffers
                .bg_obj_scanline
                .get_mut()
                .0
                .fill(BgObjPixel(backdrop | backdrop << 32));

            unsafe {
                fns.render_scanline_bgs_and_objs[engine.control().bg_mode() as usize](
                    buffers,
                    vcount,
                    engine,
                    vram,
                    scanline_3d,
                );
                fns.apply_color_effects[engine.color_effects_control().color_effect() as usize](
                    buffers, engine,
                );
            }
        }

        #[allow(clippy::match_same_arms)]
        match display_mode {
            0 => {
                scanline_buffer.0.fill(0xFFFF_FFFF);
            }

            1 => {
                for (dst, src) in scanline_buffer
                    .0
                    .iter_mut()
                    .zip(buffers.bg_obj_scanline.get_mut().0.iter())
                {
                    *dst = src.0 as u32;
                }
            }

            2 => {
                render::render_scanline_vram_display(scanline_buffer, vcount, engine, vram);
            }

            _ => {
                // TODO: Main memory display mode
            }
        }

        unsafe {
            (fns.apply_brightness)(scanline_buffer, engine);
        }

        if render_bg_obj_line && line < (SCREEN_HEIGHT - 1) as u8 {
            prerender_objs::<R, _, _, _>(buffers, line + 1, engine, vram);
        }

        if R::IS_A && engine.capture_enabled_in_frame() && line < engine.capture_height() {
            capture::run(
                line,
                engine.control(),
                engine.capture_control(),
                buffers.bg_obj_scanline.get_mut(),
                scanline_3d,
                vram,
            )
        }
    }
}

impl RendererTrait for Renderer {
    fn uses_bg_obj_vram_tracking(&self) -> bool {
        false
    }

    fn uses_lcdc_vram_tracking(&self) -> bool {
        false
    }

    fn framebuffer(&self) -> &Framebuffer {
        unsafe { &*(self.framebuffer.as_ptr() as *const () as *const Framebuffer) }
    }

    fn start_prerendering_objs(
        &mut self,
        engines: (&mut Engine2d<EngineA>, &mut Engine2d<EngineB>),
        vram: &mut Vram,
    ) {
        prerender_objs::<EngineA, _, _, _>(&mut self.buffers[0], 0, engines.0, vram);
        prerender_objs::<EngineB, _, _, _>(&mut self.buffers[1], 0, engines.1, vram);
    }

    fn start_scanline(
        &mut self,
        line: u8,
        _vcount: u8,
        engines: (&mut Engine2d<EngineA>, &mut Engine2d<EngineB>),
        _vram: &mut Vram,
    ) {
        if line == 0 && engines.0.engine_3d_enabled_in_frame() {
            self.renderer_3d_rx.start_frame();
        }
    }

    fn finish_scanline(
        &mut self,
        line: u8,
        vcount: u8,
        engines: (&mut Engine2d<EngineA>, &mut Engine2d<EngineB>),
        vram: &mut Vram,
    ) {
        self.render_scanline(line, vcount, engines.0, vram);
        self.render_scanline(line, vcount, engines.1, vram);
    }
}
