#![feature(
    step_trait,
    new_zeroed_alloc,
    slice_ptr_get,
    array_chunks,
    slice_as_chunks,
    hash_extract_if
)]
#![warn(clippy::all)]

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

#[macro_use]
mod utils;

#[macro_use]
mod config;

mod audio;
#[cfg(feature = "debug-views")]
mod debug_views;
mod frame_data;
use frame_data::FrameData;
mod game_db;
mod input;

mod emu;
mod ui;

fn main() {
    emu_utils::app::setup_current_dir();
    ui::main();
}
