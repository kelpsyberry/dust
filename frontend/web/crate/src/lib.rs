#[cfg(feature = "log")]
mod console_log;

use core::str;
use dust_core::{emu::input::Keys, emu::Emu, utils::BoxedByteSlice, Model};
use js_sys::{Uint32Array, Uint8Array};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct EmuState {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[wasm_bindgen]
pub enum SaveType {
    None,
    Eeprom4k,
    EepromFram64k,
    EepromFram512k,
    EepromFram1m,
    Flash2m,
    Flash4m,
    Flash8m,
    Nand64m,
    Nand128m,
    Nand256m,
}

#[wasm_bindgen]
impl EmuState {
    pub fn reset(&mut self) {}

    pub fn load_save(&mut self, ram_arr: Uint8Array) {}

    pub fn export_save(&self) -> Uint8Array {
        Uint8Array::from(&[][..])
    }

    pub fn update_input(&mut self, pressed: u32, released: u32) {}

    pub fn update_touch(&mut self, x: Option<u16>, y: Option<u16>) {}

    pub fn run_frame(&mut self) -> Uint32Array {
        Uint32Array::from(&[][..])
    }
}

// Wasm-bindgen creates invalid output using a constructor, for some reason
#[wasm_bindgen]
pub fn create_emu_state(
    rom_arr: Uint8Array,
    bios7_arr: Uint8Array,
    bios9_arr: Uint8Array,
    firmware_arr: Uint8Array,
    save_type: SaveType,
) -> EmuState {
    console_error_panic_hook::set_once();

    let mut rom = BoxedByteSlice::new_zeroed(rom_arr.length() as usize);
    rom_arr.copy_to(&mut rom[..]);

    EmuState {}
}
