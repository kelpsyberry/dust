#![feature(
    step_trait,
    once_cell,
    hash_drain_filter,
    new_uninit,
    slice_ptr_get,
    int_log,
    try_blocks,
    slice_ptr_len
)]

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

#[macro_use]
mod utils;

mod audio;
mod config;
#[cfg(feature = "debug-views")]
mod debug_views;
mod game_db;
pub mod input;
mod triple_buffer;

mod emu;
mod ui;

use dust_core::{gpu::Framebuffer, utils::zeroed_box};
use std::panic;

#[repr(C)]
struct FrameData {
    fb: Box<Framebuffer>,
    fps: f32,
    #[cfg(feature = "debug-views")]
    debug: debug_views::FrameData,
}

impl Default for FrameData {
    fn default() -> Self {
        FrameData {
            fb: zeroed_box(),
            fps: 0.0,
            #[cfg(feature = "debug-views")]
            debug: debug_views::FrameData::new(),
        }
    }
}

fn main() {
    let panic_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        error!(
            "Unexpected panic",
            "Encountered unexpected panic: {}\n\nThe emulator will now quit.", info
        );
        panic_hook(info);
    }));

    ui::main();
}
