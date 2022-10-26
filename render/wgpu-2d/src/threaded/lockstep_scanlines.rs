mod impls;

use crate::common::{
    self, capture,
    gfx::{self, GfxData, Renderer3dRx},
    render::{self, objs::prerender_objs},
    rgb5_to_rgb6_64, BgObjPixel, ObjPixel, ScanlineFlags, WindowPixel,
};
use core::{
    cell::UnsafeCell,
    hint,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};
use dust_core::{
    gpu::{
        engine_2d::{
            BgControl, BrightnessControl, CaptureControl, ColorEffectsControl, Control, Engine2d,
            EngineA, EngineB, Renderer as RendererTrait, Role, WindowControl, WindowsActive,
        },
        vram, Framebuffer, Scanline, SCREEN_HEIGHT, SCREEN_WIDTH,
    },
    utils::Bytes,
};
use std::{sync::Arc, thread};

#[repr(C)]
struct Vram<R: Role>
where
    [(); R::BG_VRAM_LEN]: Sized,
    [(); R::OBJ_VRAM_LEN]: Sized,
{
    bg: Bytes<{ R::BG_VRAM_LEN }>,
    obj: Bytes<{ R::OBJ_VRAM_LEN }>,
    palette: Bytes<0x406>,
    bg_ext_palette: Bytes<0x8006>,
    obj_ext_palette: Bytes<0x2006>,
    oam: Bytes<0x400>,
}

#[repr(C)]
#[allow(clippy::type_complexity)]
struct SharedData {
    stopped: AtomicBool,
    processing_line: AtomicBool,
    vcount: AtomicU8,

    vram: UnsafeCell<(Box<Vram<EngineA>>, Box<Vram<EngineB>>)>,
    rendering_data: UnsafeCell<[RenderingData; 2]>,
    capture_scanlines: UnsafeCell<(Scanline<BgObjPixel>, Scanline<u32>)>,

    framebuffer: UnsafeCell<Box<[[Scanline<BgObjPixel>; SCREEN_HEIGHT]; 2]>>,
}

unsafe impl Sync for SharedData {}

#[derive(Clone)]
struct Bg {
    control: BgControl,
    priority: u8,
    scroll: [u16; 2],
}

#[derive(Clone)]
struct AffineBgData {
    x_incr: [i16; 2],
    y_incr: [i16; 2],
    pos: [i32; 2],
}

#[derive(Clone)]
struct RenderingData {
    is_enabled: bool,
    engine_3d_enabled_in_frame: bool,
    is_on_lower_screen: bool,
    control: Control,
    master_brightness_control: BrightnessControl,
    master_brightness_factor: u32,
    bgs: [Bg; 4],
    affine_bg_data: [AffineBgData; 2],
    window_x_ranges: [(u8, u8); 2],
    window_control: [WindowControl; 4],
    windows_active: WindowsActive,
    color_effects_control: ColorEffectsControl,
    blend_coeffs: (u8, u8),
    brightness_coeff: u8,
    capture_control: CaptureControl,
    capture_enabled_in_frame: bool,
    is_capturing_3d_output: bool,
    capture_height: u8,
}

impl<R: Role> From<&Engine2d<R>> for RenderingData {
    fn from(other: &Engine2d<R>) -> Self {
        macro_rules! bgs {
            ($($i: literal),*) => {{
                [$(
                    {
                        let bg = &other.bgs[$i];
                        Bg {
                            control: bg.control(),
                            priority: bg.priority(),
                            scroll: bg.scroll,
                        }
                    }
                ),*]
            }}
        }
        macro_rules! affine_bgs {
            ($($i: literal),*) => {{
                [$(
                    {
                        let affine_bg = &other.affine_bg_data[$i];
                        AffineBgData {
                            x_incr: affine_bg.x_incr,
                            y_incr: affine_bg.y_incr,
                            pos: affine_bg.pos,
                        }
                    }
                ),*]
            }}
        }
        RenderingData {
            is_enabled: other.is_enabled(),
            engine_3d_enabled_in_frame: other.engine_3d_enabled_in_frame(),
            is_on_lower_screen: other.is_on_lower_screen(),
            control: other.control(),
            master_brightness_control: other.master_brightness_control(),
            master_brightness_factor: other.master_brightness_factor(),
            bgs: bgs!(0, 1, 2, 3),
            affine_bg_data: affine_bgs!(0, 1),
            window_x_ranges: *other.window_x_ranges(),
            window_control: *other.window_control(),
            windows_active: other.windows_active(),
            color_effects_control: other.color_effects_control(),
            blend_coeffs: other.blend_coeffs(),
            brightness_coeff: other.brightness_coeff(),
            capture_control: other.capture_control(),
            capture_enabled_in_frame: other.capture_enabled_in_frame(),
            is_capturing_3d_output: other.is_capturing_3d_output(),
            capture_height: other.capture_height(),
        }
    }
}

pub struct FrontendChannels {
    common_shared_data: Arc<gfx::SharedData>,
    common: gfx::FrontendChannels,
}

impl FrontendChannels {
    pub fn new_color_output_view(&self) -> Option<wgpu::TextureView> {
        self.common.new_color_output_view()
    }

    pub fn set_renderer_3d_rx(&self, renderer_3d_rx: Renderer3dRx) {
        self.common.set_renderer_3d_rx(renderer_3d_rx);
    }

    pub fn set_resolution_scale_shift(&self, value: u8) {
        self.common_shared_data.set_resolution_scale_shift(value);
    }
}

pub struct Renderer {
    affine_bg_pos: [[[i32; 2]; 2]; 2],
    shared_data: Arc<SharedData>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Renderer {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        resolution_scale_shift: u8,
        renderer_3d_rx: Renderer3dRx,
    ) -> (Self, wgpu::TextureView, FrontendChannels) {
        const BG: Bg = Bg {
            control: BgControl(0),
            scroll: [0; 2],
            priority: 4,
        };
        const AFFINE_BG_DATA: AffineBgData = AffineBgData {
            x_incr: [0; 2],
            y_incr: [0; 2],
            pos: [0; 2],
        };

        let rendering_data_b = RenderingData {
            is_enabled: false,
            engine_3d_enabled_in_frame: false,
            is_on_lower_screen: false,
            control: Control(0),
            master_brightness_control: BrightnessControl(0),
            master_brightness_factor: 0,
            bgs: [BG; 4],
            affine_bg_data: [AFFINE_BG_DATA; 2],
            window_x_ranges: [(0, 0); 2],
            window_control: [WindowControl(0); 4],
            windows_active: WindowsActive(0),
            color_effects_control: ColorEffectsControl(0),
            blend_coeffs: (0, 0),
            brightness_coeff: 0,
            capture_control: CaptureControl(0),
            capture_enabled_in_frame: false,
            is_capturing_3d_output: false,
            capture_height: 128,
        };

        let common_shared_data = Arc::new(gfx::SharedData::new(resolution_scale_shift));

        let shared_data = Arc::new(unsafe {
            SharedData {
                stopped: AtomicBool::new(false),
                processing_line: AtomicBool::new(false),
                vcount: AtomicU8::new(0),

                vram: UnsafeCell::new((
                    Box::new_zeroed().assume_init(),
                    Box::new_zeroed().assume_init(),
                )),
                rendering_data: UnsafeCell::new([
                    RenderingData {
                        is_on_lower_screen: true,
                        ..rendering_data_b.clone()
                    },
                    rendering_data_b,
                ]),
                capture_scanlines: UnsafeCell::new((
                    Scanline([BgObjPixel(0); SCREEN_WIDTH]),
                    Scanline([0; SCREEN_WIDTH]),
                )),

                framebuffer: UnsafeCell::new(Box::new_zeroed().assume_init()),
            }
        });

        let (color_output_view_tx, color_output_view_rx) = crossbeam_channel::unbounded();

        let (renderer_3d_rx_tx, renderer_3d_rx_rx) = crossbeam_channel::unbounded();

        let (thread_data, color_output_view) = ThreadData::new(
            Arc::clone(&device),
            Arc::clone(&queue),
            color_output_view_tx,
            renderer_3d_rx_rx,
            Arc::clone(&common_shared_data),
            Arc::clone(&shared_data),
            resolution_scale_shift,
            renderer_3d_rx,
        );

        (
            Renderer {
                affine_bg_pos: [[[0; 2]; 2]; 2],

                shared_data: Arc::clone(&shared_data),

                thread: Some(
                    thread::Builder::new()
                        .name("2D rendering".to_string())
                        .spawn(move || {
                            thread_data.run();
                        })
                        .expect("couldn't spawn 2D rendering thread"),
                ),
            },
            color_output_view,
            FrontendChannels {
                common_shared_data,
                common: gfx::FrontendChannels::new(color_output_view_rx, renderer_3d_rx_tx),
            },
        )
    }

    fn flush_vram_updates<R: Role>(&mut self, vram: &mut vram::Vram) {
        let shared_vram = unsafe { &mut *self.shared_data.vram.get() };
        let updates = unsafe { vram.bg_obj_updates.as_mut().unwrap_unchecked() }.get_mut();

        macro_rules! update {
            (
                $i: literal, $shared_vram: expr,
                (
                    $(
                        $region: ident, $src_region: ident,
                        $subregions: literal, $subregion_shift: literal
                    ),*;
                    $(
                        $bool_region: ident, $bool_src_region: ident,
                        $bool_src_region_offset: literal, $bool_region_len: literal
                    ),*
                )
            ) => {
                $(
                    for i in 0..$subregions {
                        if updates[$i].$region & 1 << i != 0 {
                            let base = i << $subregion_shift;
                            $shared_vram.$region.as_mut_ptr().add(base).copy_from(
                                vram.$src_region.as_ptr().add(base),
                                1 << $subregion_shift,
                            );
                        }
                    }
                    updates[$i].$region = 0;
                )*
                $(
                    if updates[$i].$bool_region {
                        $shared_vram.$bool_region.as_mut_ptr().copy_from(
                            vram.$bool_src_region.as_ptr().add($bool_src_region_offset),
                            $bool_region_len,
                        );
                    }
                    updates[$i].$bool_region = false;
                )*
            }
        }

        unsafe {
            if R::IS_A {
                update!(
                    0, shared_vram.0, (
                        bg, a_bg, 32, 14,
                        obj, a_obj, 16, 14,
                        bg_ext_palette, a_bg_ext_pal, 2, 14;
                        obj_ext_palette, a_obj_ext_pal, 0, 0x2000,
                        palette, palette, 0, 0x400,
                        oam, oam, 0, 0x400
                    )
                );
            } else {
                update!(
                    1, shared_vram.1, (
                        bg, b_bg, 8, 14,
                        obj, b_obj, 8, 14;
                        palette, palette, 0x400, 0x400,
                        oam, oam, 0x400, 0x400
                    )
                );

                for i in 0..2 {
                    if updates[1].bg_ext_palette & 1 << i != 0 {
                        let base = i << 14;
                        shared_vram
                            .1
                            .bg_ext_palette
                            .as_mut_ptr()
                            .add(base)
                            .copy_from_nonoverlapping(vram.b_bg_ext_pal_ptr.add(base), 0x4000);
                    }
                }
                updates[1].bg_ext_palette = 0;

                if updates[1].obj_ext_palette {
                    shared_vram
                        .1
                        .obj_ext_palette
                        .as_mut_ptr()
                        .copy_from_nonoverlapping(vram.b_obj_ext_pal_ptr, 0x2000);
                }
                updates[1].obj_ext_palette = false;
            }
        }
    }

    fn flush_rendering_data(&mut self, engines: (&mut Engine2d<EngineA>, &mut Engine2d<EngineB>)) {
        let rendering_data = unsafe { &mut *self.shared_data.rendering_data.get() };

        let prev_affine_bg_pos = [
            [
                rendering_data[0].affine_bg_data[0].pos,
                rendering_data[0].affine_bg_data[1].pos,
            ],
            [
                rendering_data[1].affine_bg_data[0].pos,
                rendering_data[1].affine_bg_data[1].pos,
            ],
        ];

        rendering_data[0] = RenderingData::from(&*engines.0);
        rendering_data[1] = RenderingData::from(&*engines.1);

        rendering_data[0].affine_bg_data[0].pos = prev_affine_bg_pos[0][0];
        rendering_data[0].affine_bg_data[1].pos = prev_affine_bg_pos[0][1];
        rendering_data[1].affine_bg_data[0].pos = prev_affine_bg_pos[1][0];
        rendering_data[1].affine_bg_data[1].pos = prev_affine_bg_pos[1][1];
    }

    fn start_scanline(&mut self) {
        self.shared_data
            .processing_line
            .store(true, Ordering::Release);
    }

    fn wait_for_scanline_finish(&self) {
        while self.shared_data.processing_line.load(Ordering::Acquire) {
            hint::spin_loop();
        }
    }
}

impl RendererTrait for Renderer {
    fn uses_bg_obj_vram_tracking(&self) -> bool {
        true
    }

    fn uses_lcdc_vram_tracking(&self) -> bool {
        false
    }

    fn framebuffer(&self) -> &Framebuffer {
        unimplemented!();
    }

    fn start_prerendering_objs(
        &mut self,
        engines: (&mut Engine2d<EngineA>, &mut Engine2d<EngineB>),
        vram: &mut vram::Vram,
    ) {
        self.thread.as_ref().unwrap().thread().unpark();

        self.wait_for_scanline_finish();

        self.flush_vram_updates::<EngineA>(vram);
        self.flush_vram_updates::<EngineB>(vram);

        self.flush_rendering_data(engines);

        self.start_scanline();
    }

    fn start_scanline(
        &mut self,
        line: u8,
        vcount: u8,
        engines: (&mut Engine2d<EngineA>, &mut Engine2d<EngineB>),
        vram: &mut vram::Vram,
    ) {
        if line == 0 {
            self.thread.as_ref().unwrap().thread().unpark();
        } else if !engines.0.capture_enabled_in_frame() || line > engines.0.capture_height() {
            self.wait_for_scanline_finish();
        }

        {
            let display_mode = engines.0.control().display_mode_a();
            if display_mode == 1
                || (engines.0.capture_enabled_in_frame()
                    && !engines.0.capture_control().src_a_3d_only())
            {
                self.flush_vram_updates::<EngineA>(vram);
            }
            #[allow(clippy::match_same_arms)]
            match display_mode {
                2 => render::render_scanline_vram_display(
                    unsafe {
                        (&mut *self.shared_data.framebuffer.get())
                            [engines.0.is_on_lower_screen() as usize]
                            .get_unchecked_mut(line as usize)
                    },
                    vcount,
                    engines.0,
                    vram,
                ),
                3 => {
                    // TODO: Main memory display mode
                }
                _ => {}
            }
        }
        if engines.1.control().display_mode_b() == 1 {
            self.flush_vram_updates::<EngineB>(vram);
        }

        if line == 0 {
            self.wait_for_scanline_finish();
        }

        self.flush_rendering_data((engines.0, engines.1));

        macro_rules! update_affine_bgs {
            ($($engine_i: literal, $engine: expr, ($($i: literal),*));*) => {{
                let shared_data = unsafe { &mut *self.shared_data.rendering_data.get() };
                $($(
                    let new_pos = $engine.affine_bg_data[$i].pos;
                    let saved_pos = &mut self.affine_bg_pos[$engine_i][$i];
                    if new_pos != *saved_pos || line == 0 {
                        *saved_pos = new_pos;
                        shared_data[$engine_i].affine_bg_data[$i].pos = new_pos;
                    }
                )*)*
            }}
        }
        update_affine_bgs!(0, engines.0, (0, 1); 1, engines.1, (0, 1));

        self.shared_data.vcount.store(vcount, Ordering::Relaxed);
        self.start_scanline();
    }

    fn finish_scanline(
        &mut self,
        line: u8,
        _vcount: u8,
        engines: (&mut Engine2d<EngineA>, &mut Engine2d<EngineB>),
        vram: &mut vram::Vram,
    ) {
        if engines.0.capture_enabled_in_frame() && line < engines.0.capture_height() {
            self.wait_for_scanline_finish();
            let (bg_obj_scanline, scanline_3d) =
                unsafe { &*self.shared_data.capture_scanlines.get() };
            capture::run(
                line,
                engines.0.control(),
                engines.0.capture_control(),
                bg_obj_scanline,
                engines
                    .0
                    .engine_3d_enabled_in_frame()
                    .then_some(scanline_3d),
                vram,
            )
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.shared_data.stopped.store(true, Ordering::Relaxed);
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

struct Buffers {
    obj_window: UnsafeCell<[u8; SCREEN_WIDTH / 8]>,
    obj_scanline: UnsafeCell<Scanline<ObjPixel>>,
    // Allow for slightly out-of-bounds SIMD accesses
    window: UnsafeCell<Scanline<WindowPixel, { SCREEN_WIDTH + 7 }>>,
    bg_obj_scanline: UnsafeCell<Scanline<BgObjPixel>>,
}

type FnPtrs<R> = common::FnPtrs<R, Buffers, RenderingData, Vram<R>>;

struct ThreadData {
    cur_scanline: i16,
    shared_data: Arc<SharedData>,

    fns: (FnPtrs<EngineA>, FnPtrs<EngineB>),
    buffers: [Buffers; 2],
    fb_scanline_flags: [[ScanlineFlags; SCREEN_HEIGHT]; 2],
    gfx_data: GfxData,
}

impl ThreadData {
    #[allow(clippy::too_many_arguments)]
    fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        color_output_view_tx: crossbeam_channel::Sender<wgpu::TextureView>,
        renderer_3d_rx_rx: crossbeam_channel::Receiver<Renderer3dRx>,
        common_shared_data: Arc<gfx::SharedData>,
        shared_data: Arc<SharedData>,
        resolution_scale_shift: u8,
        renderer_3d_rx: Renderer3dRx,
    ) -> (Self, wgpu::TextureView) {
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

        let (gfx_data, color_output_view) = GfxData::new(
            device,
            queue,
            Arc::clone(&common_shared_data),
            color_output_view_tx,
            renderer_3d_rx_rx,
            resolution_scale_shift,
            renderer_3d_rx,
        );

        (
            ThreadData {
                cur_scanline: 0,
                shared_data,

                fns: (FnPtrs::new(), FnPtrs::new()),
                buffers: [buffers!(), buffers!()],
                fb_scanline_flags: [[ScanlineFlags::default(); SCREEN_HEIGHT]; 2],
                gfx_data,
            },
            color_output_view,
        )
    }

    fn render_scanline<R: Role>(&mut self, vcount: u8, vram: &Vram<R>)
    where
        [(); R::BG_VRAM_LEN]: Sized,
        [(); R::OBJ_VRAM_LEN]: Sized,
    {
        let data = &mut unsafe { &mut *self.shared_data.rendering_data.get() }[!R::IS_A as usize];

        let fns = unsafe {
            &*(if R::IS_A {
                &self.fns.0 as *const _ as *const ()
            } else {
                &self.fns.1 as *const _ as *const ()
            } as *const FnPtrs<R>)
        };
        let buffers = &mut self.buffers[!R::IS_A as usize];

        let render_obj_line = if self.cur_scanline >= 0 {
            let (scanline_buffer, scanline_flags) = unsafe {
                (
                    (&mut *self.shared_data.framebuffer.get())[data.is_on_lower_screen as usize]
                        .get_unchecked_mut(self.cur_scanline as usize),
                    self.fb_scanline_flags[data.is_on_lower_screen as usize]
                        .get_unchecked_mut(self.cur_scanline as usize),
                )
            };

            let display_mode = if R::IS_A {
                data.control.display_mode_a()
            } else {
                data.control.display_mode_b()
            };

            let render_bg_obj_line = display_mode == 1
                || (R::IS_A
                    && data.capture_enabled_in_frame
                    && !data.capture_control.src_a_3d_only());

            // According to melonDS, if vcount falls outside the drawing range or 2D engine B is
            // disabled, the scanline is filled with pure white.
            if vcount >= SCREEN_HEIGHT as u8 || (!R::IS_A && !data.is_enabled) {
                if R::IS_A && data.engine_3d_enabled_in_frame {
                    self.gfx_data.skip_3d_scanline();
                }
                scanline_buffer.0.fill(BgObjPixel(0x3_FFFF));
                *scanline_flags = ScanlineFlags::default();
            } else {
                if R::IS_A && data.engine_3d_enabled_in_frame {
                    let enabled_in_bg_obj = data.bgs[0].priority != 4 && data.control.bg0_3d();
                    if (data.capture_enabled_in_frame
                        && (data.capture_control.src_a_3d_only() || enabled_in_bg_obj))
                        || (display_mode == 1 && enabled_in_bg_obj)
                    {
                        self.gfx_data
                            .process_3d_scanline(self.cur_scanline as usize);
                    } else {
                        self.gfx_data.skip_3d_scanline();
                    }
                }

                if render_bg_obj_line {
                    let window = buffers.window.get_mut();

                    window.0[..SCREEN_WIDTH].fill(WindowPixel(
                        if data.control.wins_enabled() == 0 {
                            0x3F
                        } else {
                            data.window_control[2].0
                        },
                    ));

                    if data.control.obj_win_enabled() {
                        let obj_window_pixel = WindowPixel(data.window_control[3].0);
                        for (i, window_pixel) in window.0[..SCREEN_WIDTH].iter_mut().enumerate() {
                            if buffers.obj_window.get_mut()[i >> 3] & 1 << (i & 7) != 0 {
                                *window_pixel = obj_window_pixel;
                            }
                        }
                    }

                    for i in (0..2).rev() {
                        if !data.windows_active.0 & 1 << i != 0 {
                            continue;
                        }

                        let x_range = &data.window_x_ranges[i];
                        let x_start = x_range.0 as usize;
                        let mut x_end = x_range.1 as usize;
                        if x_end < x_start {
                            x_end = 256;
                        }
                        window.0[x_start..x_end].fill(WindowPixel(data.window_control[i].0));
                    }

                    let backdrop = BgObjPixel(rgb5_to_rgb6_64(vram.palette.read_le::<u16>(0)))
                        .with_color_effects_mask(1 << 5)
                        .0;
                    buffers
                        .bg_obj_scanline
                        .get_mut()
                        .0
                        .fill(BgObjPixel(backdrop | backdrop << 32));

                    unsafe {
                        fns.render_scanline_bgs_and_objs[data.control.bg_mode() as usize](
                            buffers, vcount, data, vram,
                        );
                    }
                }

                match display_mode {
                    0 => {
                        scanline_buffer.0.fill(BgObjPixel(0x3_FFFF));
                        *scanline_flags =
                            ScanlineFlags::master_brightness_only(data.master_brightness_control);
                    }

                    1 => {
                        scanline_buffer
                            .0
                            .copy_from_slice(&buffers.bg_obj_scanline.get_mut().0);
                        *scanline_flags = ScanlineFlags::new(
                            data.master_brightness_control,
                            data.color_effects_control,
                            data.blend_coeffs,
                            data.brightness_coeff,
                        );
                        for i in 0..SCREEN_WIDTH {
                            if !buffers.window.get_mut().0[i].color_effects_enabled() {
                                scanline_buffer.0[i].0 &= !0xFC00_0000_FC00_0000;
                            }
                        }
                    }

                    2 => {
                        *scanline_flags =
                            ScanlineFlags::master_brightness_only(data.master_brightness_control);
                    }

                    _ => {}
                }

                if R::IS_A
                    && data.capture_enabled_in_frame
                    && self.cur_scanline < data.capture_height as i16
                {
                    let (capture_bg_obj_scanline, capture_scanline_3d) =
                        unsafe { &mut *self.shared_data.capture_scanlines.get() };
                    if data.is_capturing_3d_output {
                        let scanline_3d = self
                            .gfx_data
                            .capture_3d_scanline(self.cur_scanline as usize);
                        if data.capture_control.src_a_3d_only() {
                            capture_scanline_3d.0.copy_from_slice(&scanline_3d.0);
                        } else {
                            render::bgs::patch_scanline_bg_3d(
                                buffers.bg_obj_scanline.get_mut(),
                                scanline_3d,
                            );
                        }
                    }
                    if !data.capture_control.src_a_3d_only() {
                        unsafe {
                            fns.apply_color_effects
                                [data.color_effects_control.color_effect() as usize](
                                buffers, data
                            );
                        }
                        capture_bg_obj_scanline
                            .0
                            .copy_from_slice(&buffers.bg_obj_scanline.get_mut().0);
                    }
                }
            }

            render_bg_obj_line && self.cur_scanline < (SCREEN_HEIGHT - 1) as i16
        } else {
            true
        };

        if render_obj_line {
            prerender_objs::<R, _, _, _>(
                &mut self.buffers[!R::IS_A as usize],
                (self.cur_scanline + 1) as u8,
                data,
                vram,
            );
        }
    }

    fn run(mut self) {
        thread::park();
        loop {
            if self.shared_data.stopped.load(Ordering::Relaxed) {
                return;
            }

            if !self.shared_data.processing_line.load(Ordering::Relaxed) {
                hint::spin_loop();
                continue;
            }

            if self.cur_scanline == 0 {
                let data = &unsafe { &*self.shared_data.rendering_data.get() }[0];
                self.gfx_data
                    .start_frame(data.engine_3d_enabled_in_frame, data.is_capturing_3d_output);
            }

            let vcount = self.shared_data.vcount.load(Ordering::Acquire);
            let vram = unsafe { &*self.shared_data.vram.get() };

            self.render_scanline::<EngineA>(vcount, &vram.0);
            self.render_scanline::<EngineB>(vcount, &vram.1);

            self.shared_data
                .processing_line
                .store(false, Ordering::Release);

            self.cur_scanline += 1;
            if self.cur_scanline == SCREEN_HEIGHT as i16 {
                self.cur_scanline = -1;

                unsafe {
                    self.gfx_data.finish_frame(
                        &*self.shared_data.framebuffer.get(),
                        &self.fb_scanline_flags,
                        (*self.shared_data.rendering_data.get())[0].engine_3d_enabled_in_frame,
                    )
                }

                thread::park();
            }
        }
    }
}
