mod empty;
pub use empty::Empty;
pub mod eeprom_4k;
pub mod eeprom_fram;
pub mod flash;

use crate::utils::ByteSlice;

trait SpiDevice {
    fn contents(&self) -> ByteSlice;
    fn contents_dirty(&self) -> bool;
    fn mark_contents_flushed(&mut self);
    fn write_data(&mut self, data: u8, first: bool, last: bool) -> u8;
}

#[derive(Clone)]
pub enum Spi {
    Eeprom4K(eeprom_4k::Eeprom4K),
    EepromFram(eeprom_fram::EepromFram),
    Flash(flash::Flash),
    Empty(Empty),
}

impl Spi {
    pub fn contents(&self) -> ByteSlice {
        handle_variants!(
            Spi;
            Eeprom4K, EepromFram, Flash, Empty;
            self, contents()
        )
    }

    pub fn contents_dirty(&self) -> bool {
        handle_variants!(
            Spi;
            Eeprom4K, EepromFram, Flash, Empty;
            self, contents_dirty()
        )
    }

    pub fn mark_contents_flushed(&mut self) {
        handle_variants!(
            Spi;
            Eeprom4K, EepromFram, Flash, Empty;
            self, mark_contents_flushed()
        );
    }

    pub fn write_data(&mut self, data: u8, first: bool, last: bool) -> u8 {
        handle_variants!(
            Spi;
            Eeprom4K, EepromFram, Flash, Empty;
            self, write_data(data, first, last)
        )
    }
}

impl_from_variants!(
    Spi;
    Eeprom4K, EepromFram, Flash, Empty;
    eeprom_4k::Eeprom4K, eeprom_fram::EepromFram, flash::Flash, Empty
);
