use crate::{
    utils::{bitfield_debug, ByteSlice, BoxedByteSlice},
    SaveContents,
};

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct SavedStatus(pub u8) {
        pub write_protect: u8 @ 2..=3,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Status(pub u8) {
        pub write_in_progress: bool @ 0,
        pub write_enabled: bool @ 1,
        pub write_protect: u8 @ 2..=3,
    }
}

#[derive(Clone)]
pub struct Eeprom4K {
    #[cfg(feature = "log")]
    logger: slog::Logger,

    contents: BoxedByteSlice,
    contents_dirty: bool,

    status: Status,
    write_protect_start: u16,

    cur_command: u8,
    cur_command_pos: u8,
    cur_addr: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationError {
    IncorrectSize,
}

impl Eeprom4K {
    pub fn new(
        contents: SaveContents,
        saved_status: Option<SavedStatus>,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Result<Self, CreationError> {
        if contents.len() != 512 {
            return Err(CreationError::IncorrectSize);
        }
        let mut result = Eeprom4K {
            #[cfg(feature = "log")]
            logger,

            contents: contents.get_or_create(|_| {
                let mut contents = BoxedByteSlice::new_zeroed(512);
                contents.fill(0xFF);
                contents
            }),
            contents_dirty: false,

            status: Status(0xF0),
            write_protect_start: 0x200,

            cur_command: 0,
            cur_command_pos: 0,
            cur_addr: 0,
        };
        result.set_status(Status(0xF0 | saved_status.unwrap_or(SavedStatus(0)).0));
        Ok(result)
    }

    pub fn reset(self) -> Self {
        Eeprom4K {
            status: Status(0xF0 | self.saved_status().0),

            cur_command: 0,
            cur_command_pos: 0,
            cur_addr: 0,

            ..self
        }
    }

    #[inline]
    pub fn status(&self) -> Status {
        self.status
    }

    #[inline]
    pub fn saved_status(&self) -> SavedStatus {
        SavedStatus(self.status.0 & 0x0C)
    }

    #[inline]
    pub fn set_status(&mut self, value: Status) {
        self.status.0 = (self.status.0 & 0xF3) | (value.0 & 0x0C);
        self.write_protect_start = match value.write_protect() {
            0 => 0x200,
            1 => 0x180,
            2 => 0x100,
            _ => 0,
        };
    }

    #[inline]
    pub fn cur_command(&self) -> u8 {
        self.cur_command
    }

    #[inline]
    pub fn cur_addr(&self) -> u16 {
        self.cur_addr
    }

    #[inline]
    pub fn cur_command_pos(&self) -> u8 {
        self.cur_command_pos
    }
}

impl super::SpiDevice for Eeprom4K {
    fn contents(&self) -> ByteSlice {
        self.contents.as_byte_slice()
    }

    fn contents_dirty(&self) -> bool {
        self.contents_dirty
    }

    fn mark_contents_flushed(&mut self) {
        self.contents_dirty = false;
    }

    fn write_data(&mut self, value: u8, first: bool, last: bool) -> u8 {
        // Implemented based on official docs for the ST M95040-W, found in DS cartridges.
        // TODO:
        // - What happens when writes are disabled and a write command is issued?
        // - What's the actual value for high-z responses? Since no other device should be
        //   connected, does it float to 0 or 0xFF?
        if first {
            self.cur_command = value;
            self.cur_command_pos = 0;
        }
        match self.cur_command {
            0x06 | 0x0E => {
                // Write enable
                self.status.set_write_enabled(true);
                0xFF // High-Z
            }

            0x04 | 0x0C => {
                // Write disable
                self.status.set_write_enabled(false);
                0xFF // High-Z
            }

            0x05 | 0x0D => {
                // Read status register
                if self.cur_command_pos == 0 {
                    self.cur_command_pos += 1;
                    0xFF // High-Z
                } else {
                    self.status.0
                }
            }

            0x01 | 0x09 => {
                // Write status register
                if self.status.write_enabled() {
                    self.set_status(Status(value));
                    if last {
                        self.status.set_write_enabled(false);
                    }
                }
                0xFF // High-Z
            }

            0x03 | 0x0B => {
                // Read
                match self.cur_command_pos {
                    0 => {
                        self.cur_addr = (value as u16 & 8) << 5;
                        self.cur_command_pos += 1;
                        0xFF // High-Z
                    }
                    1 => {
                        self.cur_addr |= value as u16;
                        self.cur_command_pos += 1;
                        0xFF // High-Z
                    }
                    _ => {
                        let result =
                            unsafe { self.contents.read_unchecked(self.cur_addr as usize) };
                        self.cur_addr = (self.cur_addr + 1) & 0x1FF;
                        result
                    }
                }
            }

            0x02 | 0x0A => {
                // Write
                if self.status.write_enabled() {
                    match self.cur_command_pos {
                        0 => {
                            self.cur_addr = (value as u16 & 8) << 5;
                            self.cur_command_pos += 1;
                        }
                        1 => {
                            self.cur_addr |= value as u16;
                            self.cur_command_pos += 1;
                        }
                        _ => {
                            unsafe {
                                self.contents.write_unchecked(self.cur_addr as usize, value);
                            }
                            self.cur_addr = (self.cur_addr & 0x1F0) | ((self.cur_addr + 1) & 0xF);
                        }
                    }
                    if last {
                        self.status.set_write_enabled(false);
                        self.contents_dirty = true;
                    }
                }
                0xFF // High-Z
            }

            _ => {
                if first {
                    #[cfg(feature = "log")]
                    slog::warn!(self.logger, "Unrecognized command: {:#X}", self.cur_command);
                }
                0xFF // High-Z
            }
        }
    }
}
