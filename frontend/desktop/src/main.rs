#![feature(
    step_trait,
    once_cell,
    hash_drain_filter,
    new_uninit,
    slice_ptr_get,
    int_log,
    try_blocks,
    slice_ptr_len,
    array_chunks,
    portable_simd,
    associated_type_defaults,
    const_trait_impl,
    const_mut_refs,
    slice_as_chunks,
    duration_constants
)]

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
mod ds_slot_rom;
use ds_slot_rom::DsSlotRom;
mod frame_data;
use frame_data::FrameData;
mod game_db;
mod input;

mod emu;
mod ui;

fn main() {
    ui::main();
}
