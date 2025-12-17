#![feature(step_trait, slice_ptr_get, rustc_attrs)]
#![warn(clippy::all)]
#![allow(internal_features)]

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
