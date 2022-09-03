use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SaveType {
    None,
    #[serde(rename = "eeprom-4k")]
    Eeprom4k,
    #[serde(rename = "eeprom-fram-64k")]
    EepromFram64k,
    #[serde(rename = "eeprom-fram-512k")]
    EepromFram512k,
    #[serde(rename = "eeprom-fram-1m")]
    EepromFram1m,
    #[serde(rename = "flash-2m")]
    Flash2m,
    #[serde(rename = "flash-4m")]
    Flash4m,
    #[serde(rename = "flash-8m")]
    Flash8m,
    #[serde(rename = "nand-64m")]
    Nand64m,
    #[serde(rename = "nand-128m")]
    Nand128m,
    #[serde(rename = "nand-256m")]
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

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Entry {
    pub code: u32,
    pub rom_size: u32,
    pub save_type: SaveType,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Database(Vec<Entry>);

pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
}

impl Database {
    pub fn read_from_file(path: &Path) -> Result<Self, Error> {
        let content = fs::read_to_string(path).map_err(Error::Io)?;
        serde_json::from_str(&content).map_err(Error::Json)
    }

    pub fn lookup(&self, game_code: u32) -> Option<Entry> {
        self.0
            .binary_search_by_key(&game_code, |entry| entry.code)
            .ok()
            .map(|i| self.0[i])
    }
}
