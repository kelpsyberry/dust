use crate::utils::ByteSlice;

#[derive(Clone)]
pub struct Empty {
    #[cfg(feature = "log")]
    logger: slog::Logger,
}

#[allow(clippy::new_without_default)]
impl Empty {
    #[inline]
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

impl super::SpiDevice for Empty {
    fn contents(&self) -> ByteSlice {
        ByteSlice::new(&[])
    }

    fn contents_dirty(&self) -> bool {
        false
    }

    fn mark_contents_flushed(&mut self) {}

    fn write_data(&mut self, _data: u8, _first: bool, _last: bool) -> u8 {
        #[cfg(feature = "log")]
        slog::info!(
            self.logger,
            "{:#04X}{}",
            _data,
            match (_first, _last) {
                (false, false) => "",
                (true, false) => " (first)",
                (false, true) => " (last)",
                (true, true) => " (first, last)",
            }
        );
        0
    }
}
