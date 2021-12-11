pub trait SpiDevice {
    fn write_data(&mut self, data: u8, first: bool, last: bool) -> u8;
}

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
}

impl SpiDevice for Empty {
    fn write_data(&mut self, _data: u8, _first: bool, _last: bool) -> u8 {
        #[cfg(feature = "log")]
        slog::warn!(
            self.logger,
            "{:#04X} {}",
            _data,
            match (_first, _last) {
                (false, false) => "",
                (true, false) => "(first)",
                (false, true) => "(last)",
                (true, true) => "(first, last)",
            }
        );
        0
    }
}
