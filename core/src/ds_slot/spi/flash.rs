use crate::{flash, SaveContents};

pub type Status = flash::Status;

#[derive(Clone)]
pub struct Flash {
    #[cfg(feature = "log")]
    logger: slog::Logger,
    pub contents: flash::Flash,
    has_ir: bool,
    first: bool,
    accessing_flash: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationError {
    IncorrectSize,
}

pub enum CreationContents {}

impl Flash {
    pub fn new(
        contents: SaveContents,
        id: [u8; 20],
        has_ir: bool,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Result<Self, CreationError> {
        if !matches!(contents.len(), 0x4_0000 | 0x8_0000 | 0x10_0000) {
            return Err(CreationError::IncorrectSize);
        }
        Ok(Flash {
            contents: flash::Flash::new(
                contents,
                id,
                #[cfg(feature = "log")]
                logger.new(slog::o!("contents" => "")),
            )
            .unwrap(),
            has_ir,
            first: false,
            accessing_flash: false,
            #[cfg(feature = "log")]
            logger,
        })
    }

    #[must_use]
    pub fn reset(self) -> Self {
        Flash {
            contents: self.contents.reset(),
            first: false,
            accessing_flash: false,
            ..self
        }
    }

    #[inline]
    pub fn has_ir(&self) -> bool {
        self.has_ir
    }

    #[inline]
    pub fn id(&self) -> &[u8; 20] {
        self.contents.id()
    }

    #[inline]
    pub fn status(&self) -> Status {
        self.contents.status()
    }

    #[inline]
    pub fn powered_down(&self) -> bool {
        self.contents.powered_down()
    }

    #[inline]
    pub fn cur_command(&self) -> u8 {
        self.contents.cur_command()
    }

    #[inline]
    pub fn cur_addr(&self) -> u32 {
        self.contents.cur_addr()
    }

    #[inline]
    pub fn cur_command_pos(&self) -> u8 {
        self.contents.cur_command_pos()
    }
}

impl super::SpiDevice for Flash {
    fn contents(&self) -> emu_utils::ByteSlice {
        self.contents.contents()
    }

    fn contents_mut(&mut self) -> emu_utils::ByteMutSlice {
        self.contents.contents_mut()
    }

    fn contents_dirty(&self) -> bool {
        self.contents.contents_dirty()
    }

    fn mark_contents_dirty(&mut self) {
        self.contents.mark_contents_dirty();
    }

    fn mark_contents_flushed(&mut self) {
        self.contents.mark_contents_flushed();
    }

    fn write_data(&mut self, value: u8, first: bool, last: bool) -> u8 {
        if self.has_ir {
            if first {
                self.accessing_flash = value == 0x00;
                self.first = true;
                return 0xFF;
            }
            let first = self.first;
            self.first = false;
            if self.accessing_flash {
                self.contents.handle_byte(value, first, last)
            } else {
                #[cfg(feature = "log")]
                slog::info!(
                    self.logger,
                    "IR: {:#04X}{}",
                    value,
                    match (first, last) {
                        (false, false) => "",
                        (true, false) => " (first)",
                        (false, true) => " (last)",
                        (true, true) => " (first, last)",
                    }
                );
                0xFF
            }
        } else {
            self.contents.handle_byte(value, first, last)
        }
    }
}
