mod renderer;
pub use renderer::Renderer;
mod render;
use render::RenderingState;

use dust_core::{
    gpu::{
        engine_3d::{Polygon, RenderingControl, ScreenVertex},
        Scanline, SCREEN_HEIGHT,
    },
    utils::{Bytes, Zero},
};
use std::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU8},
};

struct RenderingData {
    control: RenderingControl,
    texture: Bytes<0x8_0000>,
    tex_pal: Bytes<0x1_8000>,
    vert_ram: [ScreenVertex; 6144],
    poly_ram: [Polygon; 2048],
    poly_ram_level: u16,
    w_buffering: bool,
}

unsafe impl Zero for RenderingData {}

struct SharedData {
    rendering_data: Box<UnsafeCell<RenderingData>>,
    scanline_buffer: Box<UnsafeCell<[Scanline<u32, 256>; SCREEN_HEIGHT]>>,
    processing_scanline: AtomicU8,
    stopped: AtomicBool,
}

unsafe impl Sync for SharedData {}
