mod empty;
mod key1;
pub use empty::Empty;
pub mod header;
pub mod icon;
pub mod normal;

use super::RomOutputLen;
use crate::{
    utils::{mem_prelude::*, Savestate},
    Model,
};

#[allow(clippy::len_without_is_empty)]
pub trait Contents {
    fn len(&self) -> usize;

    fn game_code(&self) -> u32;

    fn secure_area_mut(&mut self) -> Option<&mut [u8]>;
    fn dldi_area_mut(&mut self, addr: usize, len: usize) -> Option<&mut [u8]>;

    fn read_header(&mut self, buf: &mut Bytes<0x170>);
    fn read_slice(&mut self, addr: usize, output: &mut [u8]);
}

impl Contents for BoxedByteSlice {
    fn len(&self) -> usize {
        (**self).len()
    }

    fn game_code(&self) -> u32 {
        self.read_le::<u32>(0xC)
    }

    fn secure_area_mut(&mut self) -> Option<&mut [u8]> {
        let arm9_rom_offset = self.read_le::<u32>(0x20) as usize;
        self.get_mut(arm9_rom_offset..arm9_rom_offset + 0x800)
    }

    fn dldi_area_mut(&mut self, addr: usize, len: usize) -> Option<&mut [u8]> {
        self.get_mut(addr..addr + len)
    }

    fn read_header(&mut self, buf: &mut Bytes<0x170>) {
        buf.copy_from_slice(&self[..0x170]);
    }

    fn read_slice(&mut self, addr: usize, output: &mut [u8]) {
        let end_addr = addr + output.len();
        output.copy_from_slice(&self[addr..end_addr]);
    }
}

trait RomDevice {
    fn read(&mut self, addr: u32, output: &mut [u8]);
    fn read_header(&mut self, buf: &mut Bytes<0x170>);
    fn chip_id(&self) -> u32;
    fn setup(&mut self, direct_boot: bool) -> Result<(), ()>;
    fn handle_rom_command(
        &mut self,
        cmd: Bytes<8>,
        output: &mut Bytes<0x4000>,
        output_len: RomOutputLen,
    );
}

#[derive(Savestate)]
#[load(in_place_only)]
pub enum Rom {
    Normal(normal::Normal),
    Empty(Empty),
}

impl Rom {
    pub fn read(&mut self, addr: u32, output: &mut [u8]) {
        forward_to_variants!(Rom; Normal, Empty; self, read(addr, output));
    }

    pub fn read_header(&mut self, buf: &mut Bytes<0x170>) {
        forward_to_variants!(Rom; Normal, Empty; self, read_header(buf));
    }

    pub fn chip_id(&self) -> u32 {
        forward_to_variants!(Rom; Normal, Empty; self, chip_id())
    }

    pub(crate) fn setup(&mut self, direct_boot: bool) -> Result<(), ()> {
        forward_to_variants!(Rom; Normal, Empty; self, setup(direct_boot))
    }

    pub fn handle_rom_command(
        &mut self,
        cmd: Bytes<8>,
        output: &mut Bytes<0x4000>,
        output_len: RomOutputLen,
    ) {
        forward_to_variants!(Rom; Normal, Empty; self, handle_rom_command(cmd, output, output_len));
    }

    pub fn into_contents(self) -> Option<Box<dyn Contents>> {
        match self {
            Rom::Normal(rom) => Some(rom.into_contents()),
            Rom::Empty(_) => None,
        }
    }

    pub fn contents(&mut self) -> Option<&mut dyn Contents> {
        match self {
            Rom::Normal(rom) => Some(rom.contents()),
            Rom::Empty(_) => None,
        }
    }
}

impl_from_variants!(Rom; Normal, Empty; normal::Normal, Empty);

pub fn min_size_for_model(model: Model) -> usize {
    match model {
        Model::Ds | Model::Lite | Model::Ique | Model::IqueLite => 0x200,
        Model::Dsi => 0x1000,
    }
}

pub fn is_valid_size(len: usize, model: Model) -> bool {
    len.is_power_of_two() && len >= min_size_for_model(model)
}
