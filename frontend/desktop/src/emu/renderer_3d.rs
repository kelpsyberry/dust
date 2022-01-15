use dust_core::{
    gpu::{
        engine_3d::{
            PolyVertIndex, PolyVertsLen, Polygon, Renderer as RendererTrair, ScreenVertex,
        },
        Scanline, SCREEN_HEIGHT,
    },
    utils::{zeroed_box, Bytes, Zero},
};
use std::{
    cell::UnsafeCell,
    hint,
    mem::transmute,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    thread,
};

struct RenderingData {
    texture: Bytes<0x8_0000>,
    tex_pal: Bytes<0x1_8000>,
    vert_ram: [ScreenVertex; 6144],
    poly_ram: [Polygon; 2048],
    vert_ram_level: u16,
    poly_ram_level: u16,
}

unsafe impl Zero for RenderingData {}

impl RenderingData {
    fn copy_texture_data(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        state: &dust_core::gpu::engine_3d::RenderingState,
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

struct SharedData {
    rendering_data: Box<UnsafeCell<RenderingData>>,
    scanline_buffer: Box<UnsafeCell<[Scanline<u32, 512>; SCREEN_HEIGHT]>>,
    processing_scanline: AtomicU8,
    stopped: AtomicBool,
}

unsafe impl Sync for SharedData {}

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
        state: &dust_core::gpu::engine_3d::RenderingState,
    ) {
        self.wait_for_line(SCREEN_HEIGHT as u8 - 1);

        let rendering_data = unsafe { &mut *self.shared_data.rendering_data.get() };
        rendering_data.copy_texture_data(texture, tex_pal, state);
        rendering_data.vert_ram[..vert_ram.len()].copy_from_slice(vert_ram);
        rendering_data.poly_ram[..poly_ram.len()].copy_from_slice(poly_ram);
        rendering_data.vert_ram_level = vert_ram.len() as u16;
        rendering_data.poly_ram_level = poly_ram.len() as u16;

        self.shared_data
            .processing_scanline
            .store(u8::MAX, Ordering::Release);
        self.thread.as_ref().unwrap().thread().unpark();
    }

    fn repeat_last_frame(
        &mut self,
        texture: &Bytes<0x8_0000>,
        tex_pal: &Bytes<0x1_8000>,
        state: &dust_core::gpu::engine_3d::RenderingState,
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

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct Edge {
    a: ScreenVertex,
    b: ScreenVertex,
    x_ref: i32,
    x_incr: i32,
    x_limit: i32,
    y_start: u8,
    y_end: u8,
    is_x_major: bool,
    is_negative: bool,
}

impl Edge {
    fn new(a: ScreenVertex, b: ScreenVertex) -> Self {
        // Slope calculation based on https://github.com/StrikerX3/nds-interp

        let a_x = a.coords.extract(0) as i32;
        let b_x = b.coords.extract(0) as i32;
        let a_y = a.coords.extract(1) as u8;
        let mut b_y = b.coords.extract(1) as u8;
        let mut x_diff = b_x - a_x;
        let y_diff = b_y as i32 - a_y as i32;
        if y_diff == 0 {
            b_y += 1;
        }

        let mut x_ref = a_x << 18;

        let is_negative = x_diff < 0;
        if is_negative {
            x_ref -= 1;
            x_diff = -x_diff;
        }

        let is_x_major = x_diff > y_diff;
        if x_diff >= y_diff {
            if is_negative {
                x_ref -= 1 << 17;
            } else {
                x_ref += 1 << 17;
            }
        }

        let x_incr = if y_diff == 0 {
            x_diff << 18
        } else {
            x_diff * ((1 << 18) / y_diff)
        };

        Edge {
            a,
            b,
            x_ref,
            x_incr,
            x_limit: if is_negative { b_x } else { b_x + 1 },
            y_start: a_y,
            y_end: b_y,
            is_x_major,
            is_negative,
        }
    }

    fn compute_line(&mut self, y: u8) -> (u16, u16) {
        let line_x_disp = self.x_incr * (y - self.y_start) as i32;
        let start_x = if self.is_negative {
            self.x_ref - line_x_disp
        } else {
            self.x_ref + line_x_disp
        };
        if self.is_x_major {
            if self.is_negative {
                (
                    (((start_x + (0x1FF - (start_x & 0x1FF)) - self.x_incr) >> 18) + 1)
                        .clamp(self.x_limit, 512) as u16,
                    ((start_x >> 18) + 1).clamp(0, 512) as u16,
                )
            } else {
                (
                    (start_x >> 18).clamp(0, 512) as u16,
                    ((((start_x & !0x1FF) + self.x_incr) >> 18).clamp(0, self.x_limit) as u16),
                )
            }
        } else {
            (
                (start_x >> 18).clamp(0, 512) as u16,
                ((start_x >> 18) + 1).clamp(0, 512) as u16,
            )
        }
    }
}

fn inc_poly_vert_index(i: PolyVertIndex, verts: PolyVertsLen) -> PolyVertIndex {
    let new = i.get() + 1;
    if new == verts.get() {
        PolyVertIndex::new(0)
    } else {
        PolyVertIndex::new(new)
    }
}

fn dec_poly_vert_index(i: PolyVertIndex, verts: PolyVertsLen) -> PolyVertIndex {
    if i.get() == 0 {
        PolyVertIndex::new(verts.get() - 1)
    } else {
        PolyVertIndex::new(i.get() - 1)
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct RenderingPolygon {
    poly: Polygon,
    height_minus_1: u8,
    left_edge_decreasing: bool,
    left_edge: Edge,
    right_edge: Edge,
    left_edge_enabled: bool,
    right_edge_enabled: bool,
    left_i: PolyVertIndex,
    right_i: PolyVertIndex,
}

unsafe impl Zero for RenderingPolygon {}

struct RenderingState {
    shared_data: Arc<SharedData>,
    polys: Box<[RenderingPolygon; 2048]>,
}

impl RenderingState {
    fn new(shared_data: Arc<SharedData>) -> Self {
        RenderingState {
            shared_data,
            polys: zeroed_box(),
        }
    }

    fn run_frame(&mut self) {
        let rendering_data = unsafe { &*self.shared_data.rendering_data.get() };
        {
            let len = rendering_data.poly_ram_level as usize;
            for (dst, src) in self.polys[..len]
                .iter_mut()
                .zip(&rendering_data.poly_ram[..len])
            {
                if src.vertices_len.get() < 3 {
                    continue;
                }

                let top_y = src.top_y as u16;
                let (top_i, top_vert) = unsafe {
                    src.vertices[..src.vertices_len.get() as usize]
                        .iter()
                        .enumerate()
                        .filter_map(|(i, vert_addr)| {
                            let vert = &rendering_data.vert_ram[vert_addr.get() as usize];
                            if vert.coords.extract(1) == top_y {
                                Some((i, vert))
                            } else {
                                None
                            }
                        })
                        .last()
                        .unwrap_unchecked()
                };
                let top_i = PolyVertIndex::new(top_i as u8);

                let mut other_verts = [
                    dec_poly_vert_index(top_i, src.vertices_len),
                    inc_poly_vert_index(top_i, src.vertices_len),
                ]
                .map(|i| {
                    (
                        i,
                        &rendering_data.vert_ram[src.vertices[i.get() as usize].get() as usize],
                        src.depth_values[i.get() as usize],
                    )
                });

                let left_edge_decreasing = other_verts[0]
                    .1
                    .coords
                    .le(other_verts[1].1.coords)
                    .extract(0);

                if !left_edge_decreasing {
                    other_verts.swap(0, 1);
                }

                *dst = RenderingPolygon {
                    poly: *src,
                    height_minus_1: src.bot_y - src.top_y,
                    left_edge_decreasing,
                    left_edge: Edge::new(
                        *top_vert,
                        *other_verts[0].1,
                        // src.depth_values[top_i as usize],
                        // other_verts[0].2,
                    ),
                    right_edge: Edge::new(
                        *top_vert,
                        *other_verts[1].1,
                        // src.depth_values[top_i as usize],
                        // other_verts[1].2,
                    ),
                    left_edge_enabled: true,
                    right_edge_enabled: true,
                    left_i: other_verts[0].0,
                    right_i: other_verts[1].0,
                };
            }
        }

        for y in 0..SCREEN_HEIGHT as u8 {
            let scanline = &mut unsafe { &mut *self.shared_data.scanline_buffer.get() }[y as usize];
            scanline.0.fill(0);
            for poly in self.polys[..rendering_data.poly_ram_level as usize].iter_mut() {
                if y.wrapping_sub(poly.poly.top_y) <= poly.height_minus_1 {
                    if poly.left_edge_enabled {
                        if y >= poly.left_edge.y_end {
                            let mut i = poly.left_i;
                            let mut start_vert = &poly.left_edge.b;
                            let mut y_start = poly.left_edge.y_end;
                            loop {
                                if i == poly.right_i {
                                    poly.left_edge_enabled = false;
                                    poly.left_i = i;
                                    break;
                                }
                                i = if poly.left_edge_decreasing {
                                    dec_poly_vert_index(i, poly.poly.vertices_len)
                                } else {
                                    inc_poly_vert_index(i, poly.poly.vertices_len)
                                };
                                let new_end_vert = &rendering_data.vert_ram
                                    [poly.poly.vertices[i.get() as usize].get() as usize];
                                let new_y_end = new_end_vert.coords.extract(1) as u8;
                                if new_y_end >= y_start {
                                    poly.left_edge = Edge::new(*start_vert, *new_end_vert);
                                    poly.left_i = i;
                                    break;
                                } else {
                                    start_vert = new_end_vert;
                                    y_start = new_y_end.max(y);
                                }
                            }
                        }
                        if poly.left_edge_enabled {
                            let left_range = poly.left_edge.compute_line(y);
                            for x in left_range.0..left_range.1 {
                                scanline.0[x as usize] = 0xFFFF;
                            }
                        }
                    }

                    if poly.right_edge_enabled {
                        if y >= poly.right_edge.y_end {
                            let mut i = poly.right_i;
                            let mut start_vert = &poly.right_edge.b;
                            let mut y_start = poly.right_edge.y_end;
                            loop {
                                if i == poly.left_i {
                                    poly.right_edge_enabled = false;
                                    poly.right_i = i;
                                    break;
                                }
                                i = if poly.left_edge_decreasing {
                                    inc_poly_vert_index(i, poly.poly.vertices_len)
                                } else {
                                    dec_poly_vert_index(i, poly.poly.vertices_len)
                                };
                                let new_end_vert = &rendering_data.vert_ram
                                    [poly.poly.vertices[i.get() as usize].get() as usize];
                                let new_y_end = new_end_vert.coords.extract(1) as u8;
                                if new_y_end >= y_start {
                                    poly.right_edge = Edge::new(*start_vert, *new_end_vert);
                                    poly.right_i = i;
                                    break;
                                } else {
                                    start_vert = new_end_vert;
                                    y_start = new_y_end.max(y);
                                }
                            }
                        }
                        if poly.right_edge_enabled {
                            let right_range = poly.right_edge.compute_line(y);
                            for x in right_range.0..right_range.1 {
                                scanline.0[x as usize] = 0xFFFF;
                            }
                        }
                    }
                }
            }

            if self
                .shared_data
                .processing_scanline
                .compare_exchange(y, y + 1, Ordering::Release, Ordering::Relaxed)
                .is_err()
            {
                return;
            }
        }
    }
}
