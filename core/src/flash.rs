use crate::{
    utils::{bitfield_debug, zeroed_box, BoxedByteSlice, ByteMutSlice, ByteSlice},
    SaveContents,
};

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Status(pub u8) {
        pub write_in_progress: bool @ 0,
        pub write_enabled: bool @ 1,
    }
}

#[derive(Clone)]
pub struct Flash {
    #[cfg(feature = "log")]
    logger: slog::Logger,

    id: [u8; 20],

    contents: BoxedByteSlice,
    contents_len_mask: u32,
    contents_dirty: bool,

    status: Status,
    powered_down: bool,

    write_buffer: Box<[u8; 256]>,
    write_buffer_end: u8,
    write_buffer_len: u16,

    cur_command: u8,
    cur_command_pos: u8,
    cur_addr: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationError {
    SizeNotPowerOfTwo,
}

impl Flash {
    pub fn new(
        contents: SaveContents,
        id: [u8; 20],
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Result<Self, CreationError> {
        if !contents.len().is_power_of_two() || contents.len() < 0x4_0000 {
            return Err(CreationError::SizeNotPowerOfTwo);
        }
        let contents_len_mask = (contents.len() - 1) as u32;
        Ok(Flash {
            #[cfg(feature = "log")]
            logger,

            id,

            contents: contents.get_or_create(|len| {
                let mut contents = BoxedByteSlice::new_zeroed(len);
                contents.fill(0xFF);
                contents
            }),
            contents_len_mask,
            contents_dirty: false,

            status: Status(0),
            powered_down: false,

            write_buffer: zeroed_box(),
            write_buffer_end: 0,
            write_buffer_len: 0,

            cur_command: 0,
            cur_command_pos: 0,
            cur_addr: 0,
        })
    }

    #[must_use]
    pub fn reset(self) -> Self {
        Flash {
            status: Status(0),
            powered_down: false,

            write_buffer_end: 0,
            write_buffer_len: 0,

            cur_command: 0,
            cur_command_pos: 0,
            cur_addr: 0,

            ..self
        }
    }

    #[inline]
    pub fn contents(&self) -> ByteSlice {
        self.contents.as_byte_slice()
    }

    #[inline]
    pub fn contents_mut(&mut self) -> ByteMutSlice {
        self.contents.as_byte_mut_slice()
    }

    #[inline]
    pub fn contents_dirty(&self) -> bool {
        self.contents_dirty
    }

    #[inline]
    pub fn mark_contents_dirty(&mut self) {
        self.contents_dirty = true;
    }

    #[inline]
    pub fn mark_contents_flushed(&mut self) {
        self.contents_dirty = false;
    }

    #[inline]
    pub fn id(&self) -> &[u8; 20] {
        &self.id
    }

    #[inline]
    pub fn status(&self) -> Status {
        self.status
    }

    #[inline]
    pub fn powered_down(&self) -> bool {
        self.powered_down
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

    pub fn handle_byte(&mut self, value: u8, first: bool, last: bool) -> u8 {
        // Implemented based on official docs for the ST M25PE40, found in the iQue DS, and
        // Sanyo LE25FW403A (similar to the flash memory used in some DS cartridges).
        // TODO:
        // - What happens when writes are disabled and a write command is issued?
        // - What's the actual value for high-z responses? Since no other device should be
        //   connected, does it float to 0 or 0xFF?
        if first {
            self.cur_command = value;
            self.cur_command_pos = 0;
        }
        if self.powered_down {
            if self.cur_command == 0xAB {
                self.powered_down = false;
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
                    if self.status.write_enabled() {
                        match self.cur_command_pos {
                            0 => {
                                self.cur_addr = 0;
                                self.write_buffer_end = 0;
                                self.write_buffer_len = 0;
                                self.cur_command_pos += 1;
                            }
                            1..=3 => {
                                self.cur_addr =
                                    ((self.cur_addr << 8) | value as u32) & self.contents_len_mask;
                                self.cur_command_pos += 1;
                            }
                            _ => {
                                self.write_buffer[self.write_buffer_end as usize] = value;
                                self.write_buffer_end = self.write_buffer_end.wrapping_add(1);
                                self.write_buffer_len = (self.write_buffer_len + 1).min(256);
                                if last {
                                    // TODO: When more than 256 bytes are written, should the
                                    // address be advanced even for the unwritten ones or should the
                                    // write start at the original address, completely ignoring the
                                    // skipped leading bytes? Right now, the former is assumed.
                                    let mut addr = self.cur_addr;
                                    let page_base_addr = addr & !0xFF;
                                    let write_buffer_start = self
                                        .write_buffer_end
                                        .wrapping_sub(self.write_buffer_len as u8);
                                    addr = page_base_addr
                                        | (addr as u8).wrapping_add(write_buffer_start) as u32;
                                    let mut i = write_buffer_start;
                                    loop {
                                        unsafe {
                                            self.contents.write_unchecked(
                                                addr as usize,
                                                self.write_buffer[i as usize],
                                            );
                                        }
                                        addr = page_base_addr | (addr as u8).wrapping_add(1) as u32;
                                        i = i.wrapping_add(1);
                                        if i == self.write_buffer_end {
                                            break;
                                        }
                                    }
                                    self.contents_dirty = true;
                                }
                            }
                        }
                    }
                    0xFF // High-Z
                }

                0x02 => {
                    // Program
                    if self.status.write_enabled() {
                        match self.cur_command_pos {
                            0 => {
                                self.cur_addr = 0;
                                self.write_buffer_end = 0;
                                self.write_buffer_len = 0;
                                self.cur_command_pos += 1;
                            }
                            1..=3 => {
                                self.cur_addr =
                                    ((self.cur_addr << 8) | value as u32) & self.contents_len_mask;
                                self.cur_command_pos += 1;
                            }
                            _ => {
                                self.write_buffer[self.write_buffer_end as usize] = value;
                                self.write_buffer_end = self.write_buffer_end.wrapping_add(1);
                                self.write_buffer_len = (self.write_buffer_len + 1).min(256);
                                if last {
                                    // TODO: See note for write command
                                    let mut addr = self.cur_addr;
                                    let page_base_addr = addr & !0xFF;
                                    let write_buffer_start = self
                                        .write_buffer_end
                                        .wrapping_sub(self.write_buffer_len as u8);
                                    addr = page_base_addr
                                        | (addr as u8).wrapping_add(write_buffer_start) as u32;
                                    let mut i = write_buffer_start;
                                    loop {
                                        unsafe {
                                            *self.contents.get_unchecked_mut(addr as usize) &=
                                                self.write_buffer[i as usize];
                                        }
                                        addr = page_base_addr | (addr as u8).wrapping_add(1) as u32;
                                        i = i.wrapping_add(1);
                                        if i == self.write_buffer_end {
                                            break;
                                        }
                                    }
                                    self.contents_dirty = true;
                                }
                            }
                        }
                    }
                    0xFF // High-Z
                }

                0xDB => {
                    // Erase page (256 B)
                    if self.status.write_enabled() {
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
                                if last {
                                    unsafe {
                                        self.contents
                                            .get_unchecked_mut(
                                                self.cur_addr as usize
                                                    ..self.cur_addr as usize + 0x100,
                                            )
                                            .fill(0xFF);
                                    }
                                    self.contents_dirty = true;
                                }
                            }
                        }
                    }
                    0xFF // High-Z
                }

                0xD8 => {
                    // Erase sector (64 KiB)
                    if self.status.write_enabled() {
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
                                if last {
                                    unsafe {
                                        self.contents
                                            .get_unchecked_mut(
                                                self.cur_addr as usize
                                                    ..self.cur_addr as usize + 0x1_0000,
                                            )
                                            .fill(0xFF);
                                    }
                                    self.contents_dirty = true;
                                }
                            }
                        }
                    }
                    0xFF // High-Z
                }

                0xC7 => {
                    // Erase entire chip
                    if self.status.write_enabled() && last {
                        self.contents.fill(0xFF);
                        self.contents_dirty = true;
                    }
                    0xFF // High-Z
                }

                0xB9 => {
                    // Power down
                    self.powered_down = true;
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
}
