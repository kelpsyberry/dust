use crate::{flash, SaveContents};

pub type Status = flash::Status;

#[derive(Clone)]
pub struct Flash {
    pub contents: flash::Flash,
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
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Result<Self, CreationError> {
        if !matches!(contents.len(), 0x4_0000 | 0x8_0000 | 0x10_0000 | 0x80_0000) {
            return Err(CreationError::IncorrectSize);
        }
        Ok(Flash {
            contents: flash::Flash::new(
                contents,
                id,
                #[cfg(feature = "log")]
                logger,
            )
            .unwrap(),
        })
    }

    pub fn reset(self) -> Self {
        Flash {
            contents: self.contents.reset(),
        }
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

    fn contents_dirty(&self) -> bool {
        self.contents.contents_dirty()
    }

    fn mark_contents_flushed(&mut self) {
        self.contents.mark_contents_flushed();
    }

    fn write_data(&mut self, value: u8, first: bool, last: bool) -> u8 {
        self.contents.handle_byte(value, first, last)
    }
}
