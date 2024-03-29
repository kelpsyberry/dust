mod empty;
pub use empty::Empty;
pub mod eeprom_4k;
pub mod eeprom_fram;
pub mod flash;

use crate::{utils::Savestate, SaveReloadContents};

trait SpiDevice {
    fn contents(&self) -> &[u8];
    fn contents_mut(&mut self) -> &mut [u8];
    fn reload_contents(&mut self, contents: SaveReloadContents);
    fn contents_dirty(&self) -> bool;
    fn mark_contents_dirty(&mut self);
    fn mark_contents_flushed(&mut self);
    fn write_data(&mut self, data: u8, first: bool, last: bool) -> u8;
}

#[derive(Clone, Savestate)]
#[load(in_place_only)]
pub enum Spi {
    Eeprom4k(eeprom_4k::Eeprom4k),
    EepromFram(eeprom_fram::EepromFram),
    Flash(flash::Flash),
    Empty(Empty),
}

impl Spi {
    #[must_use]
    pub fn reset(self) -> Self {
        match self {
            Spi::Empty(device) => Spi::Empty(device.reset()),
            Spi::Eeprom4k(device) => Spi::Eeprom4k(device.reset()),
            Spi::EepromFram(device) => Spi::EepromFram(device.reset()),
            Spi::Flash(device) => Spi::Flash(device.reset()),
        }
    }

    pub fn contents(&self) -> &[u8] {
        forward_to_variants!(
            Spi;
            Eeprom4k, EepromFram, Flash, Empty;
            self, contents()
        )
    }

    pub fn contents_mut(&mut self) -> &mut [u8] {
        forward_to_variants!(
            Spi;
            Eeprom4k, EepromFram, Flash, Empty;
            self, contents_mut()
        )
    }

    pub fn reload_contents(&mut self, contents: SaveReloadContents) {
        forward_to_variants!(
            Spi;
            Eeprom4k, EepromFram, Flash, Empty;
            self, reload_contents(contents)
        );
    }

    pub fn contents_dirty(&self) -> bool {
        forward_to_variants!(
            Spi;
            Eeprom4k, EepromFram, Flash, Empty;
            self, contents_dirty()
        )
    }

    pub fn mark_contents_dirty(&mut self) {
        forward_to_variants!(
            Spi;
            Eeprom4k, EepromFram, Flash, Empty;
            self, mark_contents_dirty()
        );
    }

    pub fn mark_contents_flushed(&mut self) {
        forward_to_variants!(
            Spi;
            Eeprom4k, EepromFram, Flash, Empty;
            self, mark_contents_flushed()
        );
    }

    pub fn write_data(&mut self, data: u8, first: bool, last: bool) -> u8 {
        forward_to_variants!(
            Spi;
            Eeprom4k, EepromFram, Flash, Empty;
            self, write_data(data, first, last)
        )
    }
}

impl_from_variants!(
    Spi;
    Eeprom4k, EepromFram, Flash, Empty;
    eeprom_4k::Eeprom4k, eeprom_fram::EepromFram, flash::Flash, Empty
);
