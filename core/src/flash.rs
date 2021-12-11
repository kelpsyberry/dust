use crate::utils::{bitfield_debug, fill_8, BoxedByteSlice};

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Status(pub u8) {
        pub write_in_progress: bool @ 0,
        pub write_enabled: bool @ 1,
    }
}

pub struct Flash {
    #[cfg(feature = "log")]
    logger: slog::Logger,
    pub contents: BoxedByteSlice,
    contents_len_mask: u32,
    cur_addr: u32,
    cur_command_pos: u8,
    id: [u8; 20],
    status: Status,
    power_down: bool,
    cur_command: u8,
    pub contents_dirty: bool,
    write_buffer: [u8; 256],
    write_buffer_start: u8,
    write_buffer_end: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationError {
    SizeNotPowerOfTwo,
}

impl Flash {
    pub(crate) fn new(
        contents: BoxedByteSlice,
        id: [u8; 20],
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Result<Self, CreationError> {
        if !contents.len().is_power_of_two() {
            return Err(CreationError::SizeNotPowerOfTwo);
        }
        let contents_len_mask = (contents.len() - 1) as u32;
        Ok(Flash {
            #[cfg(feature = "log")]
            logger,
            contents,
            contents_len_mask,
            cur_addr: 0,
            cur_command_pos: 0,
            id,
            status: Status(0),
            power_down: false,
            cur_command: 0,
            contents_dirty: false,
            write_buffer: [0; 256],
            write_buffer_start: 0,
            write_buffer_end: 0,
        })
    }

    #[inline]
    pub const fn id(&self) -> &[u8; 20] {
        &self.id
    }

    #[inline]
    pub const fn status(&self) -> Status {
        self.status
    }

    #[inline]
    pub const fn power_down(&self) -> bool {
        self.power_down
    }

    #[inline]
    pub const fn cur_command(&self) -> u8 {
        self.cur_command
    }

    #[inline]
    pub const fn cur_addr(&self) -> u32 {
        self.cur_addr
    }

    #[inline]
    pub const fn cur_command_pos(&self) -> u8 {
        self.cur_command_pos
    }

    pub fn handle_byte(&mut self, value: u8, is_first: bool, is_last: bool) -> u8 {
        // Implemented based on official docs for the ST M25PE40, found in the iQue DS, and
        // Sanyo LE25FW403A (similar to the flash memory used in some DS cartridges)
        // TODO:
        // - What happens when writes are disabled and a write command is issued?
        // - What's the range for the amount of written bytes (GBATEK says 1-256) and what
        //   happens if it's exceeded?
        // - What's the actual value for high-z responses? Since no other device should be
        //   connected, does it float to 0 or 1?
        if is_first {
            self.cur_command = value;
            self.cur_command_pos = 0;
        }
        if self.power_down {
            if self.cur_command == 0xAB {
                self.power_down = false;
            }
            0xFF // High-Z
        } else {
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

                0x9F => {
                    // Read ID
                    match self.cur_command_pos {
                        0 => {
                            self.cur_command_pos += 1;
                            0xFF // High-Z
                        }
                        1..=19 => {
                            let result = self.id[(self.cur_command_pos - 1) as usize];
                            self.cur_command_pos += 1;
                            result
                        }
                        _ => self.id[19],
                    }
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

                0x03 => {
                    // Read
                    match self.cur_command_pos {
                        0 => {
                            self.cur_addr = 0;
                            self.cur_command_pos += 1;
                            0xFF // High-Z
                        }
                        1..=3 => {
                            self.cur_addr =
                                ((self.cur_addr << 8) | value as u32) & self.contents_len_mask;
                            self.cur_command_pos += 1;
                            0xFF // High-Z
                        }
                        _ => {
                            let result =
                                unsafe { self.contents.read_unchecked(self.cur_addr as usize) };
                            self.cur_addr = self.cur_addr.wrapping_add(1) & self.contents_len_mask;
                            result
                        }
                    }
                }

                0x0B => {
                    // Read at higher speed
                    match self.cur_command_pos {
                        0 => {
                            self.cur_addr = 0;
                            self.cur_command_pos += 1;
                            0xFF // High-Z
                        }
                        1..=3 => {
                            self.cur_addr =
                                ((self.cur_addr << 8) | value as u32) & self.contents_len_mask;
                            self.cur_command_pos += 1;
                            0xFF // High-Z
                        }
                        4 => {
                            self.cur_command_pos += 1;
                            0xFF // High-Z
                        }
                        _ => {
                            let result =
                                unsafe { self.contents.read_unchecked(self.cur_addr as usize) };
                            self.cur_addr = self.cur_addr.wrapping_add(1) & self.contents_len_mask;
                            result
                        }
                    }
                }

                0x0A => {
                    // Write
                    match self.cur_command_pos {
                        0 => {
                            self.cur_addr = 0;
                            self.write_buffer_start = 0;
                            self.write_buffer_end = 0;
                            self.cur_command_pos += 1;
                        }
                        1..=3 => {
                            self.cur_addr =
                                ((self.cur_addr << 8) | value as u32) & self.contents_len_mask;
                            self.cur_command_pos += 1;
                        }
                        _ => {
                            self.write_buffer[self.write_buffer_end as usize] = value;
                            if self.write_buffer_end == self.write_buffer_start {
                                // Drop oldest bytes
                                self.write_buffer_start = self.write_buffer_start.wrapping_add(1);
                            }
                            self.write_buffer_end = self.write_buffer_end.wrapping_add(1);
                            if is_last {
                                // TODO: When more than 256 bytes are written, should the address be
                                // advanced even for the unwritten ones or should the write start at
                                // the original address, completely ignoring the skipped leading
                                // bytes? Right now, the former is assumed
                                let mut addr = self.cur_addr;
                                let page_base_addr = addr & !0xFF;
                                addr = page_base_addr
                                    | (addr as u8).wrapping_add(self.write_buffer_start) as u32;
                                let mut i = self.write_buffer_start;
                                while i != self.write_buffer_end {
                                    unsafe {
                                        self.contents.write_unchecked(
                                            addr as usize,
                                            self.write_buffer[i as usize],
                                        );
                                    }
                                    addr = page_base_addr | (addr as u8).wrapping_add(1) as u32;
                                    i = i.wrapping_add(1);
                                }
                                self.contents_dirty = true;
                            }
                        }
                    }
                    0xFF // High-Z
                }

                0x02 => {
                    // Program
                    match self.cur_command_pos {
                        0 => {
                            self.cur_command_pos += 1;
                            self.write_buffer_start = 0;
                            self.write_buffer_end = 0;
                        }
                        1..=3 => {
                            self.cur_addr =
                                ((self.cur_addr << 8) | value as u32) & self.contents_len_mask;
                            self.cur_command_pos += 1;
                        }
                        _ => {
                            self.write_buffer[self.write_buffer_end as usize] = value;
                            if self.write_buffer_end == self.write_buffer_start {
                                // Drop oldest bytes
                                self.write_buffer_start = self.write_buffer_start.wrapping_add(1);
                            }
                            self.write_buffer_end = self.write_buffer_end.wrapping_add(1);
                            if is_last {
                                // TODO: See note for write command
                                let mut addr = self.cur_addr;
                                let page_base_addr = addr & !0xFF;
                                addr = page_base_addr
                                    | (addr as u8).wrapping_add(self.write_buffer_start) as u32;
                                let mut i = self.write_buffer_start;
                                while i != self.write_buffer_end {
                                    unsafe {
                                        *self.contents.get_unchecked_mut(addr as usize) &=
                                            self.write_buffer[i as usize];
                                    }
                                    addr = page_base_addr | (addr as u8).wrapping_add(1) as u32;
                                    i = i.wrapping_add(1);
                                }
                                self.contents_dirty = true;
                            }
                        }
                    }
                    0xFF // High-Z
                }

                0xDB => {
                    // Erase page (256 B)
                    match self.cur_command_pos {
                        0 => {
                            self.cur_command_pos += 1;
                        }
                        1..=3 => {
                            self.cur_addr =
                                ((self.cur_addr << 8) | value as u32) & self.contents_len_mask;
                            self.cur_command_pos += 1;
                        }
                        _ => {
                            self.cur_addr &= !0xFF;
                            if is_last {
                                fill_8(
                                    unsafe {
                                        self.contents.get_unchecked_mut(
                                            self.cur_addr as usize..self.cur_addr as usize + 0x100,
                                        )
                                    },
                                    0xFF,
                                );
                                self.contents_dirty = true;
                            }
                        }
                    }
                    0xFF // High-Z
                }

                0xD8 => {
                    // Erase sector (64 KiB)
                    match self.cur_command_pos {
                        0 => {
                            self.cur_command_pos += 1;
                        }
                        1..=3 => {
                            self.cur_addr =
                                ((self.cur_addr << 8) | value as u32) & self.contents_len_mask;
                            self.cur_command_pos += 1;
                        }
                        _ => {
                            self.cur_addr &= !0xFFFF;
                            if is_last {
                                fill_8(
                                    unsafe {
                                        self.contents.get_unchecked_mut(
                                            self.cur_addr as usize
                                                ..self.cur_addr as usize + 0x1_0000,
                                        )
                                    },
                                    0xFF,
                                );
                                self.contents_dirty = true;
                            }
                        }
                    }
                    0xFF // High-Z
                }

                0xC7 => {
                    // Erase entire chip
                    if is_last {
                        fill_8(&mut self.contents[..], 0xFF);
                        self.contents_dirty = true;
                    }
                    0xFF // High-Z
                }

                0xB9 => {
                    // Power down
                    self.power_down = true;
                    0xFF // High-Z
                }

                _ => {
                    if is_first {
                        #[cfg(feature = "log")]
                        slog::warn!(self.logger, "Unrecognized command: {:#X}", self.cur_command);
                    }
                    0xFF // High-Z
                }
            }
        }
    }
}
