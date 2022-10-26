use crate::{GxData, Renderer};
use dust_core::{
    gpu::{
        engine_3d::{
            AccelRendererRx, Polygon, RendererTx, RenderingState as CoreRenderingState,
            ScreenVertex,
        },
        Scanline, SCREEN_HEIGHT,
    },
    utils::Bytes,
};
use dust_soft_3d as soft;
use emu_utils::triple_buffer;
use parking_lot::RwLock;
use std::{
    cell::UnsafeCell,
    hint,
    mem::{self, MaybeUninit},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        Arc,
    },
    thread,
};

struct SharedData {
    stopped: AtomicBool,
    resolution_scale_shift: AtomicU8,

    capture_rendering_data: Box<UnsafeCell<soft::RenderingData>>,
    capture_scanline_buffer: Box<UnsafeCell<[Scanline<u32>; SCREEN_HEIGHT]>>,
    capture_processing_scanline: AtomicU8,
}

unsafe impl Sync for SharedData {}

struct FrameData {
    rendering_data: crate::FrameData,
    frame_index: u64,
    render: bool,
}

pub struct Tx {
    shared_data: Arc<SharedData>,
    frame_tx: triple_buffer::Sender<FrameData>,
    last_gx_data: GxData,
    texture_dirty: [u8; 3],
    tex_pal_dirty: [u8; 3],
    cur_frame_index: u64,
    capture_enabled: bool,

    thread: Option<thread::JoinHandle<()>>,
}

impl Tx {
    fn wait_for_capture_frame_end(&self) {
        while {
            let processing_scanline = self
                .shared_data
                .capture_processing_scanline
                .load(Ordering::Acquire);
            processing_scanline == u8::MAX || processing_scanline < SCREEN_HEIGHT as u8
        } {
            hint::spin_loop();
        }
    }

    fn finish_frame(&mut self) {
        let frame = self.frame_tx.current();
        frame.frame_index = self.cur_frame_index;
        self.cur_frame_index += 1;
        frame.render = true;
        self.frame_tx.finish();
        self.thread.as_ref().unwrap().thread().unpark();
    }
}

impl RendererTx for Tx {
    fn set_capture_enabled(&mut self, capture_enabled: bool) {
        if capture_enabled == self.capture_enabled {
            return;
        }
        self.capture_enabled = capture_enabled;
        if capture_enabled {
            self.shared_data
                .capture_processing_scanline
                .store(u8::MAX, Ordering::Release);
        }
    }

    fn swap_buffers(
        &mut self,
        vert_ram: &[ScreenVertex],
        poly_ram: &[Polygon],
        state: &CoreRenderingState,
    ) {
        self.wait_for_capture_frame_end();
        unsafe { &mut *self.shared_data.capture_rendering_data.get() }
            .prepare(vert_ram, poly_ram, state);

        self.last_gx_data.prepare(vert_ram, poly_ram, state);
        let frame = self.frame_tx.current();
        frame.rendering_data.gx.copy_from(&self.last_gx_data);
        frame.rendering_data.rendering.prepare(state);
    }

    fn repeat_last_frame(&mut self, state: &CoreRenderingState) {
        self.wait_for_capture_frame_end();
        unsafe { &mut *self.shared_data.capture_rendering_data.get() }.repeat_last_frame(state);

        let frame = self.frame_tx.current();
        frame.rendering_data.gx.copy_from(&self.last_gx_data);
        frame.rendering_data.rendering.prepare(state);
    }

    fn start_rendering(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        state: &CoreRenderingState,
    ) {
        unsafe { &mut *self.shared_data.capture_rendering_data.get() }
            .copy_vram(texture, tex_pal, state);

        if self.capture_enabled {
            self.shared_data
                .capture_processing_scanline
                .store(u8::MAX, Ordering::Release);
        }

        for elem in &mut self.texture_dirty {
            *elem |= state.texture_dirty;
        }
        for elem in &mut self.tex_pal_dirty {
            *elem |= state.tex_pal_dirty;
        }

        let i = self.frame_tx.current_i() as usize;
        self.frame_tx.current().rendering_data.rendering.copy_vram(
            texture,
            tex_pal,
            mem::replace(&mut self.texture_dirty[i], 0),
            mem::replace(&mut self.tex_pal_dirty[i], 0),
        );
        self.finish_frame();
    }

    fn skip_rendering(&mut self) {
        self.finish_frame();
    }
}

impl Drop for Tx {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.shared_data.stopped.store(true, Ordering::Relaxed);
            thread.thread().unpark();
            let _ = thread.join();
            self.shared_data.capture_processing_scanline.store(SCREEN_HEIGHT as u8, Ordering::Relaxed);
        }
    }
}

#[derive(Clone)]
pub struct Rx {
    next_capture_scanline: u8,
    shared_data: Arc<SharedData>,
}

impl Rx {
    fn wait_for_capture_line(&self, line: u8) {
        while {
            let processing_scanline = self
                .shared_data
                .capture_processing_scanline
                .load(Ordering::Acquire);
            processing_scanline == u8::MAX || processing_scanline <= line
        } {
            hint::spin_loop();
        }
    }
}

impl AccelRendererRx for Rx {
    fn start_frame(&mut self, capture_enabled: bool) {
        if capture_enabled {
            self.next_capture_scanline = 0;
        }
    }

    fn read_capture_scanline(&mut self) -> &Scanline<u32> {
        self.wait_for_capture_line(self.next_capture_scanline);
        let result = unsafe {
            &(&*self.shared_data.capture_scanline_buffer.get())[self.next_capture_scanline as usize]
        };
        self.next_capture_scanline += 1;
        result
    }
}

pub struct FrontendChannels {
    shared_data: Arc<SharedData>,
}

impl FrontendChannels {
    pub fn set_resolution_scale_shift(&self, value: u8) {
        self.shared_data
            .resolution_scale_shift
            .store(value, Ordering::Relaxed);
    }
}

pub struct Rx2dData {
    pub color_output_view: wgpu::TextureView,
    pub color_output_view_rx: crossbeam_channel::Receiver<wgpu::TextureView>,
    pub last_submitted_frame: Arc<(AtomicU64, RwLock<Option<thread::Thread>>)>,
}

pub fn init(
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    resolution_scale_shift: u8,
) -> (Tx, Rx, FrontendChannels, Rx2dData) {
    let shared_data = Arc::new(unsafe {
        SharedData {
            stopped: AtomicBool::new(false),
            resolution_scale_shift: AtomicU8::new(resolution_scale_shift),

            capture_rendering_data: Box::new_zeroed().assume_init(),
            capture_scanline_buffer: Box::new_zeroed().assume_init(),
            capture_processing_scanline: AtomicU8::new(SCREEN_HEIGHT as u8),
        }
    });
    let shared_data_ = Arc::clone(&shared_data);

    let (frame_tx, mut frame_rx) = unsafe { triple_buffer::init_zeroed() };

    let mut renderer = Renderer::new(device, queue, resolution_scale_shift);

    let color_output_view = renderer.create_output_view();
    let (color_output_view_tx, color_output_view_rx) = crossbeam_channel::unbounded();
    let last_submitted_frame: Arc<(AtomicU64, RwLock<Option<thread::Thread>>)> =
        Arc::new((AtomicU64::new(0), RwLock::new(None)));
    let last_submitted_frame_ = Arc::clone(&last_submitted_frame);

    (
        Tx {
            shared_data: Arc::clone(&shared_data),
            frame_tx,
            last_gx_data: unsafe { MaybeUninit::zeroed().assume_init() },
            texture_dirty: [0; 3],
            tex_pal_dirty: [0; 3],
            cur_frame_index: 0,
            capture_enabled: false,

            thread: Some(
                thread::Builder::new()
                    .name("3D rendering".to_string())
                    .spawn(move || {
                        let mut raw_soft_renderer = soft::Renderer::new();
                        loop {
                            if shared_data.stopped.load(Ordering::Relaxed) {
                                break;
                            }
                            if shared_data
                                .capture_processing_scanline
                                .compare_exchange(u8::MAX, 0, Ordering::Acquire, Ordering::Acquire)
                                .is_ok()
                            {
                                let rendering_data =
                                    unsafe { &*shared_data.capture_rendering_data.get() };
                                raw_soft_renderer.start_frame(rendering_data);
                                for y in 0..192 {
                                    let scanline = &mut unsafe {
                                        &mut *shared_data.capture_scanline_buffer.get()
                                    }[y as usize];
                                    raw_soft_renderer.render_line(y, scanline, rendering_data);
                                    let _ =
                                        shared_data.capture_processing_scanline.compare_exchange(
                                            y,
                                            y + 1,
                                            Ordering::Release,
                                            Ordering::Relaxed,
                                        );
                                }
                            }
                            if let Ok(frame) = frame_rx.get() {
                                if frame.render {
                                    let resolution_scale_shift =
                                        shared_data.resolution_scale_shift.load(Ordering::Relaxed);
                                    if resolution_scale_shift != renderer.resolution_scale_shift() {
                                        renderer.set_resolution_scale_shift(resolution_scale_shift);
                                        color_output_view_tx
                                            .send(renderer.create_output_view())
                                            .expect(
                                                "couldn't send 3D output texture view to UI thread",
                                            );
                                    }

                                    let command_buffer =
                                        renderer.render_frame(&frame.rendering_data);
                                    renderer.queue().submit([command_buffer]);
                                }
                                last_submitted_frame
                                    .0
                                    .store(frame.frame_index, Ordering::Relaxed);
                                if let Some(thread) = &*last_submitted_frame.1.read() {
                                    thread.unpark();
                                }
                            } else {
                                thread::park();
                            }
                        }
                    })
                    .expect("couldn't spawn 3D rendering thread"),
            ),
        },
        Rx {
            next_capture_scanline: 0,
            shared_data: Arc::clone(&shared_data_),
        },
        FrontendChannels {
            shared_data: shared_data_,
        },
        Rx2dData {
            color_output_view,
            color_output_view_rx,
            last_submitted_frame: last_submitted_frame_,
        },
    )
}
