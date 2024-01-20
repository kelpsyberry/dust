#![feature(
    step_trait,
    new_uninit,
    slice_ptr_get,
    slice_ptr_len,
    array_chunks,
    portable_simd,
    associated_type_defaults,
    const_trait_impl,
    const_mut_refs,
    slice_as_chunks,
    duration_constants,
    lazy_cell,
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
mod ds_slot_rom;
use ds_slot_rom::DsSlotRom;
mod frame_data;
use frame_data::FrameData;
mod game_db;
mod input;

mod emu;
mod ui;

fn main() {
    #[cfg(all(target_os = "macos", app_bundle))]
    {
        use cocoa::{
            base::{id, nil},
            foundation::{NSBundle, NSString},
        };
        use std::{env::set_current_dir, ffi::CStr};
        let path = (|| unsafe {
            let main_bundle = id::mainBundle();
            if main_bundle == nil {
                return None;
            }
            let resource_path = main_bundle.resourcePath();
            if resource_path == nil {
                return None;
            }
            let result = CStr::from_ptr(resource_path.UTF8String())
                .to_str()
                .ok()
                .map(str::to_string);
            let _: () = msg_send![resource_path, release];
            result
        })()
        .expect("Couldn't get bundle resource path");
        set_current_dir(path).expect("Couldn't change working directory to bundle resource path");
    }

    ui::main();
}
