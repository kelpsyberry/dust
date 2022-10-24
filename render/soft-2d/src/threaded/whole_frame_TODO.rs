mod impls;

use crate::common::{
    render::{objs::prerender_objs, FnPtrs},
    rgb5_to_rgb6, rgb5_to_rgb6_64, BgObjPixel, ObjPixel, Role, WindowPixel,
};
use core::{
    cell::UnsafeCell,
    hint, mem,
    sync::atomic::{fence, AtomicU8, Ordering},
};
use crossbeam_channel::{Receiver, Sender};
use dust_core::{
    gpu::{
        engine_2d::{
            self, BgControl, BrightnessControl, ColorEffectsControl, Control,
            Renderer as RendererTrait,
        },
        vram, Framebuffer, Scanline, SCREEN_HEIGHT, SCREEN_WIDTH,
    },
    utils::Bytes,
};
use std::{collections::VecDeque, sync::Arc, thread};

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

mod action {
    pub const NONE: u8 = 0;
    pub const START_FRAME: u8 = 1;
    pub const FINISH_FRAME: u8 = 2;
    pub const STOP: u8 = 3;
}

#[repr(C)]
struct SharedData<R: Role>
where
    [(); R::BG_VRAM_LEN]: Sized,
    [(); R::OBJ_VRAM_LEN]: Sized,
{
    cur_capture_scanline: AtomicU8,
    action: AtomicU8,
    vram: UnsafeCell<Vram<R>>,
    framebuffer: UnsafeCell<[(bool, Scanline<u32>); SCREEN_HEIGHT]>,
    capture_scanlines: Option<Box<UnsafeCell<[Scanline<u16>; SCREEN_HEIGHT]>>>,
}

unsafe impl<R: Role> Sync for SharedData<R>
where
    [(); R::BG_VRAM_LEN]: Sized,
    [(); R::OBJ_VRAM_LEN]: Sized,
{
}

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
    is_on_lower_screen: bool,
    control: Control,
    master_brightness_control: BrightnessControl,
    master_brightness_factor: u32,
    bgs: [Bg; 4],
    affine_bg_data: [AffineBgData; 2],
    color_effects_control: ColorEffectsControl,
    blend_coeffs: (u8, u8),
    brightness_coeff: u8,
}

impl From<&engine_2d::Data> for RenderingData {
    fn from(other: &engine_2d::Data) -> Self {
        macro_rules! bgs {
            ($($i: literal),*) => {{
                let bgs = other.bgs();
                [$(
                    {
                        let bg = &bgs[$i];
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
                let affine_bgs = other.affine_bg_data();
                [$(
                    {
                        let affine_bg = &affine_bgs[$i];
                        AffineBgData {
                            x_incr: affine_bg.x_incr,
                            y_incr: affine_bg.y_incr,
                            pos: affine_bg.pos(),
                        }
                    }
                ),*]
            }}
        }
        RenderingData {
            is_on_lower_screen: other.is_on_lower_screen(),
            control: other.control(),
            master_brightness_control: other.master_brightness_control(),
            master_brightness_factor: other.master_brightness_factor(),
            bgs: bgs!(0, 1, 2, 3),
            affine_bg_data: affine_bgs!(0, 1),
            color_effects_control: other.color_effects_control(),
            blend_coeffs: other.blend_coeffs(),
            brightness_coeff: other.brightness_coeff(),
        }
    }
}

struct VramUpdates {
    bg: Box<[u8]>,
    
}

struct Updates {
    line: i16,
    vram: VramUpdates,
    data: RenderingData,
}

pub struct Renderer<R: Role>
where
    [(); R::BG_VRAM_LEN]: Sized,
    [(); R::OBJ_VRAM_LEN]: Sized,
{
    tx: Sender<Updates>,
    shared_data: Arc<SharedData<R>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl<R: Role> RendererTrait for Renderer<R>
where
    [(); R::BG_VRAM_LEN]: Sized,
    [(); R::OBJ_VRAM_LEN]: Sized,
{
    fn uses_vram_tracking(&self) -> bool {
        true
    }

    fn post_load(&mut self, _data: &engine_2d::Data) {}
    fn update_color_effects_control(&mut self, _value: ColorEffectsControl) {}

    fn start_prerendering_objs(&mut self, data: &engine_2d::Data, vram: &mut vram::Vram) {
        let vram_updates = self.apply_vram_updates(vram);
        let _ = self.tx.send(Updates {
            line: -1,
            vram: vram_updates,
            data: data.into(),
        });
    }

    fn start_scanline(&mut self, _line: u8, data: &engine_2d::Data, vram: &mut vram::Vram) {
        let vram_updates = self.apply_vram_updates(vram);
        let _ = self.tx.send(Updates {
            line: -1,
            vram: vram_updates,
            data: data.into(),
        });
    }

    fn finish_scanline(
        &mut self,
        _scanline: u8,
        line: u8,
        _framebuffer: &mut [Framebuffer; 2],
        data: &mut engine_2d::Data,
        vram: &mut vram::Vram,
    ) {
        if R::IS_A && data.capture_enabled_in_frame() && line < data.capture_height() {}
    }

    fn finish_frame(&mut self, _framebuffer: &mut [Framebuffer; 2]) {}
}

impl<R: Role> Renderer<R>
where
    [(); R::BG_VRAM_LEN]: Sized,
    [(); R::OBJ_VRAM_LEN]: Sized,
{
    pub fn new(renderer_3d_rx: R::Renderer3dRx) -> Self {
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

        let (tx, rx) = crossbeam_channel::unbounded();
        let shared_data = Arc::new(SharedData {
            cur_capture_scanline: AtomicU8::new(0),
            action: AtomicU8::new(action::NONE),
            vram: UnsafeCell::new(Vram {
                bg: Bytes::new([0; R::BG_VRAM_LEN]),
                obj: Bytes::new([0; R::OBJ_VRAM_LEN]),
                palette: Bytes::new([0; 0x406]),
                bg_ext_palette: Bytes::new([0; 0x8006]),
                obj_ext_palette: Bytes::new([0; 0x2006]),
                oam: Bytes::new([0; 0x400]),
            }),
            framebuffer: UnsafeCell::new([(false, Scanline([0; SCREEN_WIDTH])); SCREEN_HEIGHT]),
            capture_scanlines: R::IS_A.then(|| unsafe { Box::new_zeroed().assume_init() }),
        });

        let thread_data = ThreadData {
            rx,
            queued_updates: VecDeque::new(),
            updates: None,

            shared_data: Arc::clone(&shared_data),
            vram: unsafe { Box::new_zeroed().assume_init() },
            rendering_data: RenderingData {
                is_on_lower_screen: R::IS_A,
                control: Control(0),
                master_brightness_control: BrightnessControl(0),
                master_brightness_factor: 0,
                bgs: [BG; 4],
                affine_bg_data: [AFFINE_BG_DATA; 2],
                color_effects_control: ColorEffectsControl(0),
                blend_coeffs: (0, 0),
                brightness_coeff: 0,
            },
            fns: FnPtrs::new(),
            renderer_3d_rx,
            buffers: Buffers {
                cur_scanline: -1,
                obj_window: UnsafeCell::new([0; SCREEN_WIDTH / 8]),
                bg_obj_scanline: UnsafeCell::new(Scanline([BgObjPixel(0); SCREEN_WIDTH])),
                obj_scanlines: UnsafeCell::new(unsafe { Box::new_zeroed().assume_init() }),
                window: UnsafeCell::new(Scanline([WindowPixel(0); SCREEN_WIDTH + 7])),
            },
        };

        Renderer {
            tx,
            shared_data,
            thread: Some(
                thread::Builder::new()
                    .name("2D rendering".to_string())
                    .spawn(move || {
                        thread_data.run();
                    })
                    .expect("couldn't spawn 2D rendering thread"),
            ),
        }
    }
}

impl<R: Role> Drop for Renderer<R>
where
    [(); R::BG_VRAM_LEN]: Sized,
    [(); R::OBJ_VRAM_LEN]: Sized,
{
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.shared_data
                .action
                .store(action::STOP, Ordering::Relaxed);
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

struct Buffers {
    cur_scanline: i16,
    obj_window: UnsafeCell<[u8; SCREEN_WIDTH / 8]>,
    bg_obj_scanline: UnsafeCell<Scanline<BgObjPixel>>,
    obj_scanlines: UnsafeCell<Box<[Scanline<ObjPixel>; SCREEN_HEIGHT]>>,
    // Allow for slightly out-of-bounds SIMD accesses
    window: UnsafeCell<Scanline<WindowPixel, { SCREEN_WIDTH + 7 }>>,
}

struct ThreadData<R: Role>
where
    [(); R::BG_VRAM_LEN]: Sized,
    [(); R::OBJ_VRAM_LEN]: Sized,
{
    rx: Receiver<Updates>,
    queued_updates: VecDeque<Updates>,
    updates: Option<Updates>,

    shared_data: Arc<SharedData<R>>,
    vram: Box<Vram<R>>,
    rendering_data: RenderingData,

    fns: FnPtrs<R, Buffers, RenderingData, Vram<R>>,
    renderer_3d_rx: R::Renderer3dRx,
    buffers: Buffers,
}

impl<R: Role> ThreadData<R>
where
    [(); R::BG_VRAM_LEN]: Sized,
    [(); R::OBJ_VRAM_LEN]: Sized,
{
    fn run(mut self) {
        loop {
            // Wait for updates or a new frame request
            loop {
                thread::park();
                match self.shared_data.action.load(Ordering::Relaxed) {
                    action::START_FRAME => {
                        self.shared_data.action.store(0, Ordering::Relaxed);
                        self.shared_data
                            .action
                            .store(action::NONE, Ordering::Relaxed);
                        break;
                    }
                    action::STOP => {
                        return;
                    }
                    _ => {}
                }
            }

            // Render a frame
            loop {
                while self.buffers.cur_scanline < SCREEN_HEIGHT as i16 {
                    if self.buffers.cur_scanline >= 0 {
                        let scanline_base = (scanline as usize) * SCREEN_WIDTH;
                        let scanline_buffer = unsafe {
                            let scanline = &mut *((&mut *self.shared_data.framebuffer.get())
                                [data.is_on_lower_screen() as usize]
                                .0
                                .as_mut_ptr()
                                .add(scanline_base)
                                as *mut Scanline<u32>);
                        };
                    }
                    if self.buffers.cur_scanline < SCREEN_HEIGHT as i16 - 1 {}

                    self.buffers.cur_scanline += 1;
                    if let Some(updates) = &self.queued_updates.front() {
                        let line = updates.line;
                        if line <= self.buffers.cur_scanline {
                            self.updates = self.queued_updates.pop_front();
                            if line < self.buffers.cur_scanline {
                                self.buffers.cur_scanline = line;
                                continue;
                            }
                        }
                    }
                    for updates in self.rx.try_iter() {
                        let line = updates.line;
                        if line <= self.buffers.cur_scanline {
                            self.updates = Some(updates);
                            if line < self.buffers.cur_scanline {
                                self.buffers.cur_scanline = line;
                                continue;
                            }
                        } else {
                            self.queued_updates.push_back(updates);
                        }
                    }
                }
                fence(Ordering::Release);
                loop {
                    if let Ok(updates) = self.rx.try_recv() {
                        self.buffers.cur_scanline = updates.line;
                        self.updates = Some(updates);
                        break;
                    }

                    if self.shared_data.action.load(Ordering::Relaxed) == action::FINISH_FRAME {
                        self.shared_data
                            .action
                            .store(action::NONE, Ordering::Relaxed);
                    }

                    hint::spin_loop();
                }
            }
        }
    }
}
