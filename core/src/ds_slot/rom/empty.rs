use super::super::RomOutputLen;
use crate::utils::{ByteMutSlice, Bytes, Savestate};

#[derive(Clone, Savestate)]
pub struct Empty {
    #[cfg(feature = "log")]
    #[savestate(skip)]
    logger: slog::Logger,
}

#[allow(clippy::new_without_default)]
impl Empty {
    pub fn new(#[cfg(feature = "log")] logger: slog::Logger) -> Self {
        Empty {
            #[cfg(feature = "log")]
            logger,
        }
    }

    #[inline]
    #[must_use]
    pub fn reset(self) -> Self {
        self
    }
}

impl super::RomDevice for Empty {
    fn read(&self, _addr: u32, mut output: ByteMutSlice) {
        output.fill(0xFF);
    }

    fn chip_id(&self) -> u32 {
        0
    }

    fn setup(&mut self, _direct_boot: bool) {}

    #[allow(clippy::needless_pass_by_value)]
    fn handle_rom_command(
        &mut self,
        _cmd: Bytes<8>,
        output: &mut Bytes<0x4000>,
        output_len: RomOutputLen,
    ) {
        #[cfg(feature = "log")]
        slog::trace!(self.logger, "{:016X}", _cmd.read_be::<u64>(0));
        // TODO: Since there is no card inserted, does the last transferred command byte linger on
        // the data bus or does it get filled with 0xFF? GBATEK seems to imply the latter.
        output[..output_len.get() as usize].fill(0xFF);
    }
}
