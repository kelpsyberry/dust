mod empty;
mod key1;
pub use empty::Empty;
pub mod header;
pub mod icon_title;
pub mod normal;

use super::RomOutputLen;
use crate::{
    utils::{mem_prelude::*, Savestate},
    Model,
};
use core::any::Any;

#[allow(clippy::len_without_is_empty)]
pub trait Contents: Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn len(&self) -> u64;

    fn game_code(&self) -> u32;

    fn secure_area_mut(&mut self) -> Option<&mut [u8]>;
    fn dldi_area_mut(&mut self, addr: u32, len: usize) -> Option<&mut [u8]>;

    fn read_header(&self, output: &mut Bytes<0x170>);
    fn read_slice(&self, addr: u32, output: &mut [u8]);

    fn read_slice_wrapping(&self, addr: u32, output: &mut [u8]) {
        let len = self.len();
        let addr = addr & (len - 1) as u32;
        let first_read_max_len = len - addr as u64;
        if output.len() as u64 <= first_read_max_len {
            self.read_slice(addr, output);
        } else {
            self.read_slice(addr, &mut output[..first_read_max_len as usize]);
            let mut i = first_read_max_len as usize;
            while i < output.len() {
                let end_i = (i + len as usize).min(output.len());
                self.read_slice(0, &mut output[i..end_i]);
                i += len as usize;
            }
        }
    }
}

impl Contents for BoxedByteSlice {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn len(&self) -> u64 {
        (**self).len() as u64
    }

    fn game_code(&self) -> u32 {
        self.read_le::<u32>(0xC)
    }

    fn secure_area_mut(&mut self) -> Option<&mut [u8]> {
        let arm9_rom_offset = self.read_le::<u32>(0x20) as usize;
        self.get_mut(arm9_rom_offset..arm9_rom_offset + 0x800)
    }

    fn dldi_area_mut(&mut self, addr: u32, len: usize) -> Option<&mut [u8]> {
        self.get_mut(addr as usize..addr as usize + len)
    }

    fn read_header(&self, output: &mut Bytes<0x170>) {
        output.copy_from_slice(&self[..0x170]);
    }

    fn read_slice(&self, addr: u32, output: &mut [u8]) {
        output.copy_from_slice(&self[addr as usize..addr as usize + output.len()]);
    }
}

trait RomDevice {
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

    pub fn contents(&self) -> Option<&dyn Contents> {
        match self {
            Rom::Normal(rom) => Some(rom.contents()),
            Rom::Empty(_) => None,
        }
    }

    pub fn contents_mut(&mut self) -> Option<&mut dyn Contents> {
        match self {
            Rom::Normal(rom) => Some(rom.contents_mut()),
            Rom::Empty(_) => None,
        }
    }
}

impl_from_variants!(Rom; Normal, Empty; normal::Normal, Empty);

pub fn min_size_for_model(model: Model) -> u64 {
    match model {
        Model::Ds | Model::Lite | Model::Ique | Model::IqueLite => 0x200,
        Model::Dsi => 0x1000,
    }
}

pub fn is_valid_size(len: u64, model: Model) -> bool {
    len.is_power_of_two() && len >= min_size_for_model(model)
}
