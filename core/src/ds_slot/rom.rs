mod empty;
mod key1;
pub use empty::Empty;
pub mod header;
pub mod icon;
pub mod normal;

use super::RomOutputLen;
use crate::utils::{ByteMutSlice, Bytes};

trait RomDevice {
    fn read(&self, addr: u32, output: ByteMutSlice);
    fn chip_id(&self) -> u32;
    fn setup(&mut self, direct_boot: bool);
    fn handle_rom_command(
        &mut self,
        cmd: Bytes<8>,
        output: &mut Bytes<0x4000>,
        output_len: RomOutputLen,
    );
}

#[derive(Clone)]
pub enum Rom {
    Normal(normal::Normal),
    Empty(Empty),
}

impl Rom {
    pub fn read(&self, addr: u32, output: ByteMutSlice) {
        handle_variants!(Rom; Normal, Empty; self, read(addr, output));
    }

    pub fn chip_id(&self) -> u32 {
        handle_variants!(Rom; Normal, Empty; self, chip_id())
    }

    pub(crate) fn setup(&mut self, direct_boot: bool) {
        handle_variants!(Rom; Normal, Empty; self, setup(direct_boot));
    }

    pub fn handle_rom_command(
        &mut self,
        cmd: Bytes<8>,
        output: &mut Bytes<0x4000>,
        output_len: RomOutputLen,
    ) {
        handle_variants!(Rom; Normal, Empty; self, handle_rom_command(cmd, output, output_len));
    }
}

impl_from_variants!(Rom; Normal, Empty; normal::Normal, Empty);
