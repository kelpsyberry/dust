#![allow(clippy::unused_unit)]
#![feature(once_cell)]

mod audio;
#[cfg(feature = "log")]
mod console_log;
pub mod renderer_3d;

use dust_core::{
    cpu::{arm7, arm9, interpreter::Interpreter},
    ds_slot::{self, rom::Rom as DsSlotRom, spi::Spi as DsSlotSpi},
    emu::input::Keys,
    emu::Emu,
    flash::Flash,
    gpu::{SCREEN_HEIGHT, SCREEN_WIDTH},
    spi::firmware,
    utils::{zeroed_box, BoxedByteSlice, Bytes},
    Model, SaveContents,
};
use js_sys::{Function, Uint32Array, Uint8Array};
use wasm_bindgen::prelude::*;

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

impl SaveType {
    pub fn expected_len(self) -> Option<usize> {
        match self {
            SaveType::None => None,
            SaveType::Eeprom4k => Some(0x200),
            SaveType::EepromFram64k => Some(0x2000),
            SaveType::EepromFram512k => Some(0x1_0000),
            SaveType::EepromFram1m => Some(0x2_0000),
            SaveType::Flash2m => Some(0x4_0000),
            SaveType::Flash4m => Some(0x8_0000),
            SaveType::Flash8m => Some(0x10_0000),
            SaveType::Nand64m => Some(0x80_0000),
            SaveType::Nand128m => Some(0x100_0000),
            SaveType::Nand256m => Some(0x200_0000),
        }
    }

    pub fn from_save_len(len: usize) -> Option<Self> {
        match len {
            0x200 => Some(SaveType::Eeprom4k),
            0x2000 => Some(SaveType::EepromFram64k),
            0x1_0000 => Some(SaveType::EepromFram512k),
            0x2_0000 => Some(SaveType::EepromFram1m),
            0x4_0000 => Some(SaveType::Flash2m),
            0x8_0000 => Some(SaveType::Flash4m),
            0x10_0000 => Some(SaveType::Flash8m),
            0x80_0000 => Some(SaveType::Nand64m),
            0x100_0000 => Some(SaveType::Nand128m),
            0x200_0000 => Some(SaveType::Nand256m),
            _ => None,
        }
    }
}

#[wasm_bindgen]
pub enum WbgModel {
    Ds,
    Lite,
    Ique,
    IqueLite,
    Dsi,
}

impl From<WbgModel> for Model {
    fn from(other: WbgModel) -> Self {
        match other {
            WbgModel::Ds => Model::Ds,
            WbgModel::Lite => Model::Lite,
            WbgModel::Ique => Model::Ique,
            WbgModel::IqueLite => Model::IqueLite,
            WbgModel::Dsi => Model::Dsi,
        }
    }
}

#[wasm_bindgen]
pub struct EmuState {
    #[cfg(feature = "log")]
    logger: slog::Logger,
    model: Model,
    emu: Option<Emu<Interpreter>>,
    arm7_bios: Option<Box<Bytes<{ arm7::BIOS_SIZE }>>>,
    arm9_bios: Option<Box<Bytes<{ arm9::BIOS_SIZE }>>>,
}

#[wasm_bindgen]
impl EmuState {
    pub fn reset(&mut self) {
        let emu = self.emu.take().unwrap();

        let mut emu_builder = dust_core::emu::Builder::new(
            emu.spi.firmware.reset(),
            match emu.ds_slot.rom {
                DsSlotRom::Empty(device) => DsSlotRom::Empty(device.reset()),
                DsSlotRom::Normal(device) => DsSlotRom::Normal(device.reset()),
            },
            match emu.ds_slot.spi {
                DsSlotSpi::Empty(device) => DsSlotSpi::Empty(device.reset()),
                DsSlotSpi::Eeprom4k(device) => DsSlotSpi::Eeprom4k(device.reset()),
                DsSlotSpi::EepromFram(device) => DsSlotSpi::EepromFram(device.reset()),
                DsSlotSpi::Flash(device) => DsSlotSpi::Flash(device.reset()),
            },
            emu.audio.backend,
            emu.rtc.backend,
            emu.gpu.engine_3d.renderer,
            #[cfg(feature = "log")]
            self.logger.clone(),
        );

        emu_builder.arm7_bios = self.arm7_bios.clone();
        emu_builder.arm9_bios = self.arm9_bios.clone();

        emu_builder.model = self.model;
        emu_builder.direct_boot = true;

        self.emu = Some(emu_builder.build(Interpreter).unwrap());
    }

    pub fn load_save(&mut self, ram_arr: Uint8Array) {
        ram_arr.copy_to(&mut self.emu.as_mut().unwrap().ds_slot.spi.contents_mut()[..])
    }

    pub fn export_save(&self) -> Uint8Array {
        Uint8Array::from(&self.emu.as_ref().unwrap().ds_slot.spi.contents()[..])
    }

    pub fn update_input(&mut self, pressed: u32, released: u32) {
        let emu = self.emu.as_mut().unwrap();
        emu.press_keys(Keys::from_bits_truncate(pressed));
        emu.release_keys(Keys::from_bits_truncate(released));
    }

    pub fn update_touch(&mut self, x: Option<u16>, y: Option<u16>) {
        let emu = self.emu.as_mut().unwrap();
        if let Some((x, y)) = x.zip(y) {
            emu.set_touch_pos([x, y]);
        } else {
            emu.end_touch();
        }
    }

    pub fn run_frame(&mut self) -> Uint32Array {
        // TODO: Handle an eventual shutdown
        let emu = self.emu.as_mut().unwrap();
        emu.run();
        Uint32Array::from(unsafe {
            core::slice::from_raw_parts(
                emu.gpu.framebuffer.0.as_ptr() as *const u32,
                SCREEN_WIDTH * SCREEN_HEIGHT * 2,
            )
        })
    }
}

// Wasm-bindgen creates invalid output using a constructor, for some reason
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn create_emu_state(
    arm7_bios_arr: Option<Uint8Array>,
    arm9_bios_arr: Option<Uint8Array>,
    firmware_arr: Option<Uint8Array>,
    rom_arr: Uint8Array,
    save_contents_arr: Option<Uint8Array>,
    save_type: Option<SaveType>,
    has_ir: bool,
    model: WbgModel,
    audio_callback: Function,
) -> EmuState {
    console_error_panic_hook::set_once();

    #[cfg(feature = "log")]
    let logger = slog::Logger::root(console_log::Console::new(), slog::o!());

    let arm7_bios = arm7_bios_arr.map(|arr| {
        let mut buf = zeroed_box::<Bytes<{ arm7::BIOS_SIZE }>>();
        arr.copy_to(&mut buf[..]);
        buf
    });

    let arm9_bios = arm9_bios_arr.map(|arr| {
        let mut buf = zeroed_box::<Bytes<{ arm9::BIOS_SIZE }>>();
        arr.copy_to(&mut buf[..]);
        buf
    });

    let model = Model::from(model);

    let firmware = firmware_arr
        .map(|arr| {
            let mut buf = BoxedByteSlice::new_zeroed(arr.length() as usize);
            arr.copy_to(&mut buf[..]);
            buf
        })
        .unwrap_or_else(|| firmware::default(model));

    let mut rom = BoxedByteSlice::new_zeroed(rom_arr.length().next_power_of_two() as usize);
    rom_arr.copy_to(&mut rom[..rom_arr.length() as usize]);

    let save_contents = save_contents_arr.map(|save_contents_arr| {
        let mut save_contents = BoxedByteSlice::new_zeroed(save_contents_arr.length() as usize);
        save_contents_arr.copy_to(&mut save_contents[..]);
        save_contents
    });

    let (ds_slot_rom, ds_slot_spi) = {
        let rom = ds_slot::rom::normal::Normal::new(
            rom,
            arm7_bios.as_deref(),
            #[cfg(feature = "log")]
            logger.new(slog::o!("ds_rom" => "normal")),
        )
        .unwrap()
        .into();

        let save_type = if let Some(save_contents) = &save_contents {
            if let Some(save_type) = save_type {
                let expected_len = save_type.expected_len();
                if expected_len != Some(save_contents.len()) {
                    let (chosen_save_type, _message) = if let Some(detected_save_type) =
                        SaveType::from_save_len(save_contents.len())
                    {
                        (detected_save_type, "existing save file")
                    } else {
                        (save_type, "database entry")
                    };
                    #[cfg(feature = "log")]
                    slog::error!(
                        logger,
                        "Unexpected save file size: expected {}, got {} B; respecting {}.",
                        if let Some(expected_len) = expected_len {
                            format!("{} B", expected_len)
                        } else {
                            "no file".to_string()
                        },
                        save_contents.len(),
                        _message,
                    );
                    chosen_save_type
                } else {
                    save_type
                }
            } else {
                #[allow(clippy::unnecessary_lazy_evaluations)]
                SaveType::from_save_len(save_contents.len()).unwrap_or_else(|| {
                    #[cfg(feature = "log")]
                    slog::error!(
                        logger,
                        concat!(
                            "Unrecognized save file size ({} B) and no database entry found, ",
                            "defaulting to an empty save.",
                        ),
                        save_contents.len()
                    );
                    SaveType::None
                })
            }
        } else {
            #[allow(clippy::unnecessary_lazy_evaluations)]
            save_type.unwrap_or_else(|| {
                #[cfg(feature = "log")]
                slog::error!(
                    logger,
                    concat!(
                        "No existing save file present and no database entry found, defaulting to ",
                        "an empty save.",
                    )
                );
                SaveType::None
            })
        };

        let spi = if save_type == SaveType::None {
            ds_slot::spi::Empty::new(
                #[cfg(feature = "log")]
                logger.new(slog::o!("ds_spi" => "empty")),
            )
            .into()
        } else {
            let expected_len = save_type.expected_len().unwrap();
            let save_contents = match save_contents {
                Some(save_contents) => {
                    SaveContents::Existing(if save_contents.len() == expected_len {
                        let mut new_contents = BoxedByteSlice::new_zeroed(expected_len);
                        new_contents[..save_contents.len()].copy_from_slice(&save_contents);
                        drop(save_contents);
                        new_contents
                    } else {
                        save_contents
                    })
                }
                None => SaveContents::New(expected_len),
            };
            match save_type {
                SaveType::None => unreachable!(),
                SaveType::Eeprom4k => ds_slot::spi::eeprom_4k::Eeprom4k::new(
                    save_contents,
                    None,
                    #[cfg(feature = "log")]
                    logger.new(slog::o!("ds_spi" => "eeprom_4k")),
                )
                .expect("Couldn't create 4 Kib EEPROM DS slot SPI device")
                .into(),
                SaveType::EepromFram64k | SaveType::EepromFram512k | SaveType::EepromFram1m => {
                    ds_slot::spi::eeprom_fram::EepromFram::new(
                        save_contents,
                        None,
                        #[cfg(feature = "log")]
                        logger.new(slog::o!("ds_spi" => "eeprom_fram")),
                    )
                    .expect("Couldn't create EEPROM/FRAM DS slot SPI device")
                    .into()
                }
                SaveType::Flash2m | SaveType::Flash4m | SaveType::Flash8m => {
                    ds_slot::spi::flash::Flash::new(
                        save_contents,
                        [0; 20],
                        has_ir,
                        #[cfg(feature = "log")]
                        logger.new(slog::o!("ds_spi" => if has_ir { "flash" } else { "flash_ir" })),
                    )
                    .expect("Couldn't create FLASH DS slot SPI device")
                    .into()
                }
                SaveType::Nand64m | SaveType::Nand128m | SaveType::Nand256m => {
                    #[cfg(feature = "log")]
                    slog::error!(logger, "TODO: NAND saves");
                    ds_slot::spi::Empty::new(
                        #[cfg(feature = "log")]
                        logger.new(slog::o!("ds_spi" => "nand_todo")),
                    )
                    .into()
                }
            }
        };

        (rom, spi)
    };

    let mut emu_builder = dust_core::emu::Builder::new(
        Flash::new(
            SaveContents::Existing(firmware),
            firmware::id_for_model(model),
            #[cfg(feature = "log")]
            logger.new(slog::o!("fw" => "")),
        )
        .expect("Couldn't build firmware"),
        ds_slot_rom,
        ds_slot_spi,
        Box::new(audio::Backend::new(audio_callback)),
        Box::new(dust_core::rtc::DummyBackend),
        Box::new(renderer_3d::EmuState::new()),
        #[cfg(feature = "log")]
        logger.clone(),
    );

    emu_builder.arm7_bios = arm7_bios.clone();
    emu_builder.arm9_bios = arm9_bios.clone();

    emu_builder.model = model;
    emu_builder.direct_boot = true;

    let emu = emu_builder.build(Interpreter).unwrap();

    EmuState {
        #[cfg(feature = "log")]
        logger,
        model,
        emu: Some(emu),
        arm7_bios,
        arm9_bios,
    }
}

#[wasm_bindgen]
pub fn internal_get_module() -> wasm_bindgen::JsValue {
    wasm_bindgen::module()
}

#[wasm_bindgen]
pub fn internal_get_memory() -> wasm_bindgen::JsValue {
    wasm_bindgen::memory()
}
