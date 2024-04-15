use dust_core::{
    gpu::{
        engine_3d::{
            Polygon, RendererTx, RenderingState as CoreRenderingState, ScreenVertex, SoftRendererRx,
        },
        Scanline, SCREEN_HEIGHT,
    },
    utils::Bytes,
};
use dust_soft_3d::{Renderer, RenderingData};
use std::{
    cell::UnsafeCell,
    hint,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    thread,
};

struct SharedData {
    rendering_data: Box<UnsafeCell<RenderingData>>,
    scanline_buffer: Box<UnsafeCell<[Scanline<u32>; SCREEN_HEIGHT]>>,
    processing_scanline: AtomicU8,
    stopped: AtomicBool,
}

unsafe impl Sync for SharedData {}

pub struct Tx {
    shared_data: Arc<SharedData>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Tx {
    fn wait_for_frame_end(&self) {
        while {
            let processing_scanline = self.shared_data.processing_scanline.load(Ordering::Acquire);
            processing_scanline == u8::MAX || processing_scanline < SCREEN_HEIGHT as u8
        } {
            hint::spin_loop();
        }
    }
}

impl RendererTx for Tx {
    fn set_capture_enabled(&mut self, _capture_enabled: bool) {}

    fn swap_buffers(
        &mut self,
        vert_ram: &[ScreenVertex],
        poly_ram: &[Polygon],
        state: &CoreRenderingState,
    ) {
        self.wait_for_frame_end();
        unsafe { &mut *self.shared_data.rendering_data.get() }.prepare(vert_ram, poly_ram, state);
    }

    fn repeat_last_frame(&mut self, state: &CoreRenderingState) {
        self.wait_for_frame_end();
        unsafe { &mut *self.shared_data.rendering_data.get() }.repeat_last_frame(state);
    }

    fn start_rendering(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        state: &CoreRenderingState,
    ) {
        unsafe { &mut *self.shared_data.rendering_data.get() }.copy_vram(texture, tex_pal, state);

        self.shared_data
            .processing_scanline
            .store(u8::MAX, Ordering::Release);
        self.thread.as_ref().unwrap().thread().unpark();
    }

    fn skip_rendering(&mut self) {}
}

impl Drop for Tx {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.shared_data.stopped.store(true, Ordering::Relaxed);
            thread.thread().unpark();
            let _ = thread.join();
            self.shared_data
                .processing_scanline
                .store(SCREEN_HEIGHT as u8, Ordering::Relaxed);
        }
    }
}

#[derive(Clone)]
pub struct Rx {
    next_scanline: u8,
    shared_data: Arc<SharedData>,
}

impl Rx {
    fn wait_for_line(&self, line: u8) {
        while {
            let processing_scanline = self.shared_data.processing_scanline.load(Ordering::Acquire);
            processing_scanline == u8::MAX || processing_scanline <= line
        } {
            hint::spin_loop();
        }
    }
}

impl SoftRendererRx for Rx {
    fn start_frame(&mut self) {
        self.next_scanline = 0;
    }

    fn read_scanline(&mut self) -> &Scanline<u32> {
        self.wait_for_line(self.next_scanline);
        let result =
            unsafe { &(&*self.shared_data.scanline_buffer.get())[self.next_scanline as usize] };
        self.next_scanline += 1;
        result
    }

    fn skip_scanline(&mut self) {
        self.next_scanline += 1;
    }
}

pub fn init() -> (Tx, Rx) {
    let shared_data = Arc::new(unsafe {
        SharedData {
            rendering_data: Box::new_zeroed().assume_init(),
            scanline_buffer: Box::new_zeroed().assume_init(),
            processing_scanline: AtomicU8::new(SCREEN_HEIGHT as u8),
            stopped: AtomicBool::new(false),
        }
    });
    let rx = Rx {
        next_scanline: 0,
        shared_data: Arc::clone(&shared_data),
    };
    (
        Tx {
            shared_data: Arc::clone(&shared_data),
            thread: Some(
                thread::Builder::new()
                    .name("3D rendering".to_owned())
                    .spawn(move || {
                        let mut raw_renderer = Renderer::new();
                        loop {
                            if shared_data.stopped.load(Ordering::Relaxed) {
                                return;
                            }
                            if shared_data
                                .processing_scanline
                                .compare_exchange(u8::MAX, 0, Ordering::Acquire, Ordering::Acquire)
                                .is_ok()
                            {
                                let rendering_data = unsafe { &*shared_data.rendering_data.get() };
                                raw_renderer.start_frame(rendering_data);
                                raw_renderer.render_line(0, rendering_data);
                                for y in 0..192 {
                                    let scanline =
                                        &mut unsafe { &mut *shared_data.scanline_buffer.get() }
                                            [y as usize];
                                    if y < 191 {
                                        raw_renderer.render_line(y + 1, rendering_data);
                                    }
                                    raw_renderer.postprocess_line(y, scanline, rendering_data);
                                    let _ = shared_data.processing_scanline.compare_exchange(
                                        y,
                                        y + 1,
                                        Ordering::Release,
                                        Ordering::Relaxed,
                                    );
                                }
                            } else {
                                thread::park();
                            }
                        }
                    })
                    .expect("couldn't spawn 3D rendering thread"),
            ),
        },
        rx,
    )
}
