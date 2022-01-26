use crate::{
    utils::{bitfield_debug, BoxedByteSlice, ByteMutSlice, ByteSlice},
    SaveContents,
};

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct SavedStatus(pub u8) {
        pub write_protect: u8 @ 2..=3,
        pub status_write_disable: bool @ 7,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Status(pub u8) {
        pub write_in_progress: bool @ 0,
        pub write_enabled: bool @ 1,
        pub write_protect: u8 @ 2..=3,
        pub status_write_disable: bool @ 7,
    }
}

#[derive(Clone)]
pub struct EepromFram {
    #[cfg(feature = "log")]
    logger: slog::Logger,

    contents: BoxedByteSlice,
    contents_len_mask: u32,
    page_mask: u32,
    write_fixed_addr_mask: u32,
    addr_bytes: u8,
    contents_dirty: bool,

    status: Status,
    write_protect_start: u32,

    cur_command: u8,
    cur_command_pos: u8,
    cur_addr: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationError {
    IncorrectSize,
}

pub enum CreationContents {}

impl EepromFram {
    pub fn new(
        contents: SaveContents,
        saved_status: Option<SavedStatus>,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Result<Self, CreationError> {
        if !matches!(contents.len(), 0x2000 | 0x1_0000 | 0x2_0000) {
            return Err(CreationError::IncorrectSize);
        }
        let contents_len_mask = (contents.len() - 1) as u32;
        let page_mask = match contents.len().trailing_zeros() {
            13 => 0x1F,
            16 => 0x3F,
            17 => 0xFF,
            _ => unreachable!(),
        };
        let addr_bytes = 2 + (contents.len().trailing_zeros() > 16) as u8;
        let mut result = EepromFram {
            #[cfg(feature = "log")]
            logger,

            contents: contents.get_or_create(|len| {
                let mut contents = BoxedByteSlice::new_zeroed(len);
                contents.fill(0xFF);
                contents
            }),
            contents_len_mask,
            page_mask,
            write_fixed_addr_mask: contents_len_mask & !page_mask,
            addr_bytes,
            contents_dirty: false,

            status: Status(0),
            write_protect_start: contents_len_mask + 1,

            cur_command: 0,
            cur_command_pos: 0,
            cur_addr: 0,
        };
        result.set_status(Status(saved_status.unwrap_or(SavedStatus(0)).0));
        Ok(result)
    }

    #[must_use]
    pub fn reset(self) -> Self {
        EepromFram {
            status: Status(self.saved_status().0),

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
        SavedStatus(self.status.0 & 0x8C)
    }

    #[inline]
    pub fn set_status(&mut self, value: Status) {
        self.status.0 = (self.status.0 & 0x73) | (value.0 & 0x8C);
        let len = self.contents.len() as u32;
        self.write_protect_start = match value.write_protect() {
            0 => len,
            1 => len >> 1 | len >> 2,
            2 => len >> 1,
            _ => 0,
        };
    }

    #[inline]
    pub fn cur_command(&self) -> u8 {
        self.cur_command
    }

    #[inline]
    pub fn cur_addr(&self) -> u32 {
        self.cur_addr
    }

    #[inline]
    pub fn cur_command_pos(&self) -> u8 {
        self.cur_command_pos
    }
}

impl super::SpiDevice for EepromFram {
    fn contents(&self) -> ByteSlice {
        self.contents.as_byte_slice()
    }

    fn contents_mut(&mut self) -> ByteMutSlice {
        self.contents.as_byte_mut_slice()
    }

    fn contents_dirty(&self) -> bool {
        self.contents_dirty
    }

    fn mark_contents_dirty(&mut self) {
        self.contents_dirty = true;
    }

    fn mark_contents_flushed(&mut self) {
        self.contents_dirty = false;
    }

    fn write_data(&mut self, value: u8, first: bool, last: bool) -> u8 {
        // Implemented based on official docs for the ST M95640-W, found in DS cartridges, and
        // the ST M95M01-R, presumably used as the 128 KiB EEPROM in commercial cartridges.
        // TODO:
        // - What happens when writes are disabled and a write command is issued?
        // - What's the actual value for high-z responses? Since no other device should be
        //   connected, does it float to 0 or 0xFF?
        if first {
            self.cur_command = value;
            self.cur_command_pos = 0;
        }
        match self.cur_command {
            0x06 => {
                // Write enable
                self.status.set_write_enabled(true);
                0xFF // High-Z
            }

            0x04 => {
                // Write disable
                self.status.set_write_enabled(false);
                0xFF // High-Z
            }

            0x05 => {
                // Read status register
                if self.cur_command_pos == 0 {
                    self.cur_command_pos += 1;
                    0xFF // High-Z
                } else {
                    self.status.0
                }
            }

            0x01 => {
                // Write status register
                if self.status.write_enabled() {
                    self.set_status(Status(value));
                    if last {
                        self.status.set_write_enabled(false);
                    }
                }
                0xFF // High-Z
            }

            0x03 => {
                // Read
                match self.cur_command_pos {
                    0 => {
                        self.cur_addr = 0;
                        self.cur_command_pos += 1;
                        0xFF // High-Z
                    }
                    pos => {
                        if pos <= self.addr_bytes {
                            self.cur_addr =
                                ((self.cur_addr << 8) | value as u32) & self.contents_len_mask;
                            self.cur_command_pos += 1;
                            0xFF // High-Z
                        } else {
                            let result =
                                unsafe { self.contents.read_unchecked(self.cur_addr as usize) };
                            self.cur_addr = (self.cur_addr + 1) & self.contents_len_mask;
                            result
                        }
                    }
                }
            }

            0x02 => {
                // Write
                if self.status.write_enabled() {
                    match self.cur_command_pos {
                        0 => {
                            self.cur_addr = 0;
                            self.cur_command_pos += 1;
                        }
                        pos => {
                            if pos <= self.addr_bytes {
                                self.cur_addr =
                                    ((self.cur_addr << 8) | value as u32) & self.contents_len_mask;
                                self.cur_command_pos += 1;
                            } else {
                                unsafe {
                                    self.contents.write_unchecked(self.cur_addr as usize, value);
                                }
                                self.cur_addr = (self.cur_addr & self.write_fixed_addr_mask)
                                    | ((self.cur_addr + 1) & self.page_mask);
                            }
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
