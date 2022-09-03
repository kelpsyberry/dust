use crate::{flash, utils::Savestate, SaveContents, SaveReloadContents};

pub type Status = flash::Status;

#[derive(Clone, Savestate)]
#[load(in_place_only)]
pub struct Flash {
    #[cfg(feature = "log")]
    #[savestate(skip)]
    logger: slog::Logger,
    pub contents: flash::Flash,
    #[savestate(skip)]
    has_ir: bool,
    ir_cmd: u8,
    first_ir_data_byte: bool,
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
            ir_cmd: 0,
            first_ir_data_byte: false,
            #[cfg(feature = "log")]
            logger,
        })
    }

    #[must_use]
    pub fn reset(self) -> Self {
        Flash {
            contents: self.contents.reset(),
            ir_cmd: 0,
            first_ir_data_byte: false,
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

    fn reload_contents(&mut self, contents: SaveReloadContents) {
        match contents {
            SaveReloadContents::Existing(contents) => {
                let mut contents_ = self.contents.contents_mut();
                contents_[..contents.len()].copy_from_slice(&contents[..]);
                contents_[contents.len()..].fill(0);
            }
            SaveReloadContents::New => self.contents.contents_mut().fill(0xFF),
        }
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
                self.ir_cmd = value;
                self.first_ir_data_byte = true;
                0
            } else {
                let first = self.first_ir_data_byte;
                self.first_ir_data_byte = false;
                match self.ir_cmd {
                    0x00 => {
                        // Pass-through to FLASH chip
                        self.contents.handle_byte(value, first, last)
                    }

                    0x08 => {
                        // Read ID
                        0xAA
                    }

                    _command => {
                        #[cfg(feature = "log")]
                        slog::warn!(
                            self.logger,
                            "Unknown IR byte (command {:#04X}): {:#04X}{}",
                            _command,
                            value,
                            match (first, last) {
                                (false, false) => "",
                                (true, false) => " (first)",
                                (false, true) => " (last)",
                                (true, true) => " (first, last)",
                            }
                        );
                        0
                    }
                }
            }
        } else {
            self.contents.handle_byte(value, first, last)
        }
    }
}
