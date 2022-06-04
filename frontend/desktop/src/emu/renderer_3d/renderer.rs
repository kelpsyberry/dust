use super::{RenderingData, RenderingState, SharedData};
use dust_core::{
    gpu::{
        engine_3d::{
            Polygon, Renderer as RendererTrair, RenderingState as CoreRenderingState, ScreenVertex,
        },
        Scanline, SCREEN_HEIGHT,
    },
    utils::{zeroed_box, Bytes},
};
use std::{
    hint,
    mem::transmute,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    thread,
};

impl RenderingData {
    fn copy_texture_data(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        state: &CoreRenderingState,
    ) {
        for i in 0..4 {
            if state.texture_dirty & 1 << i == 0 {
                continue;
            }
            let range = i << 17..(i + 1) << 17;
            self.texture[range.clone()].copy_from_slice(&texture[range]);
        }
        for i in 0..6 {
            if state.tex_pal_dirty & 1 << i == 0 {
                continue;
            }
            let range = i << 14..(i + 1) << 14;
            self.tex_pal[range.clone()].copy_from_slice(&tex_pal[range]);
        }
    }
}

pub struct Renderer {
    next_scanline: u8,
    shared_data: Arc<SharedData>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Renderer {
    fn wait_for_line(&self, line: u8) {
        while {
            let processing_scanline = self.shared_data.processing_scanline.load(Ordering::Acquire);
            processing_scanline == u8::MAX || processing_scanline <= line
        } {
            hint::spin_loop();
        }
    }
}

impl RendererTrair for Renderer {
    fn swap_buffers(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        vert_ram: &[ScreenVertex],
        poly_ram: &[Polygon],
        state: &CoreRenderingState,
        w_buffering: bool,
    ) {
        self.wait_for_line(SCREEN_HEIGHT as u8 - 1);

        let rendering_data = unsafe { &mut *self.shared_data.rendering_data.get() };
        rendering_data.copy_texture_data(texture, tex_pal, state);
        rendering_data.vert_ram[..vert_ram.len()].copy_from_slice(vert_ram);
        rendering_data.poly_ram[..poly_ram.len()].copy_from_slice(poly_ram);
        rendering_data.poly_ram_level = poly_ram.len() as u16;
        rendering_data.w_buffering = w_buffering;

        self.shared_data
            .processing_scanline
            .store(u8::MAX, Ordering::Release);
        self.thread.as_ref().unwrap().thread().unpark();
    }

    fn repeat_last_frame(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        state: &CoreRenderingState,
    ) {
        self.wait_for_line(SCREEN_HEIGHT as u8 - 1);

        let rendering_data = unsafe { &mut *self.shared_data.rendering_data.get() };
        rendering_data.copy_texture_data(texture, tex_pal, state);

        self.shared_data
            .processing_scanline
            .store(u8::MAX, Ordering::Release);
        self.thread.as_ref().unwrap().thread().unpark();
    }

    fn start_frame(&mut self) {
        self.next_scanline = 0;
    }

    fn read_scanline(&mut self) -> &Scanline<u32, 512> {
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

impl Drop for Renderer {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.shared_data.stopped.store(true, Ordering::Relaxed);
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

impl Renderer {
    pub fn new() -> Self {
        let shared_data = Arc::new(unsafe {
            SharedData {
                rendering_data: transmute(zeroed_box::<RenderingData>()),
                scanline_buffer: transmute(zeroed_box::<[Scanline<u32, 512>; SCREEN_HEIGHT]>()),
                processing_scanline: AtomicU8::new(SCREEN_HEIGHT as u8),
                stopped: AtomicBool::new(false),
            }
        });
        Renderer {
            next_scanline: 0,
            shared_data: shared_data.clone(),
            thread: Some(
                thread::Builder::new()
                    .name("3D rendering".to_string())
                    .spawn(move || {
                        let mut state = RenderingState::new(shared_data);
                        loop {
                            loop {
                                if state.shared_data.stopped.load(Ordering::Relaxed) {
                                    return;
                                }
                                if state
                                    .shared_data
                                    .processing_scanline
                                    .compare_exchange(
                                        u8::MAX,
                                        0,
                                        Ordering::Acquire,
                                        Ordering::Acquire,
                                    )
                                    .is_ok()
                                {
                                    break;
                                } else {
                                    thread::park();
                                }
                            }
                            state.run_frame();
                        }
                    })
                    .expect("Couldn't spawn 3D rendering thread"),
            ),
        }
    }
}
