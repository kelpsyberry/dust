use crate::utils::{bitfield_debug, bounded_int};

// TODO: Implement INT1 and INT2 (and also expose them)

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Control(pub u16) {
        pub data: u8 @ 0..=0,
        pub clock: bool @ 1,
        pub chipselect: bool @ 2,
        pub data_write: bool @ 4,
        pub clock_write: bool @ 5,
        pub chipselect_write: bool @ 6,
    }
}

bounded_int!(pub struct RegIndex(u8), max 7);

impl RegIndex {
    pub const STATUS1: Self = RegIndex::new(0b000);
    pub const STATUS2: Self = RegIndex::new(0b100);
    pub const DATE_TIME: Self = RegIndex::new(0b010);
    pub const TIME: Self = RegIndex::new(0b110);
    pub const INT1: Self = RegIndex::new(0b001);
    pub const INT2: Self = RegIndex::new(0b101);
    pub const ADJUST: Self = RegIndex::new(0b011);
    pub const FREE: Self = RegIndex::new(0b111);
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Status1(pub u8) {
        pub reset: bool @ 0,
        pub is_in_24_hour_mode: bool @ 1,
        pub int1_flag: bool @ 4,
        pub int2_flag: bool @ 5,
        pub power_low: bool @ 6,
        pub powered_on: bool @ 7,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Status2(pub u8) {
        pub int1_mode: u8 @ 0..=3,
        pub int2_enabled: bool @ 6,
        pub test_mode: bool @ 7,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub years_since_2000: u8,
    pub month: u8,
    pub day: u8,
    pub days_from_sunday: u8,
}

impl Default for Date {
    #[inline]
    fn default() -> Self {
        Date {
            years_since_2000: 5,
            month: 1,
            day: 1,
            days_from_sunday: 6,
        }
    }
}

pub trait Backend {
    fn get_time(&mut self) -> Time;
    fn get_date_time(&mut self) -> (Date, Time);
    fn set_date_time(&mut self, value: (Date, Time));
}

pub struct DummyBackend;

impl Backend for DummyBackend {
    fn get_time(&mut self) -> Time {
        Time {
            hour: 9,
            minute: 27,
            second: 45,
        }
    }

    fn get_date_time(&mut self) -> (Date, Time) {
        (
            Date {
                years_since_2000: 22,
                month: 1,
                day: 26,
                days_from_sunday: 3,
            },
            Time {
                hour: 9,
                minute: 27,
                second: 45,
            },
        )
    }

    fn set_date_time(&mut self, _: (Date, Time)) {}
}

pub struct Rtc {
    #[cfg(feature = "log")]
    logger: slog::Logger,
    pub backend: Box<dyn Backend>,
    latched_date_time: [u8; 7],
    date_written: bool,
    time_written: bool,
    last_date_time_write_index: u8,
    control: Control,
    data: u8,
    data_pos: u8,
    data_bit: u8,
    cur_reg: RegIndex,
    reading: bool,
    status1: Status1,
    status2: Status2,
    int1: [u8; 3],
    int2: [u8; 3],
    pub clock_adjust: u8,
    pub free_reg: u8,
}

fn from_bcd(value: u8) -> u8 {
    (value >> 4) * 10 + (value & 0xF)
}

fn to_bcd(value: u8) -> u8 {
    (value / 10) << 4 | (value % 10)
}

impl Rtc {
    pub(crate) fn new(
        backend: Box<dyn Backend>,
        first_launch: bool,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Self {
        Rtc {
            #[cfg(feature = "log")]
            logger,
            backend,
            latched_date_time: [0; 7],
            date_written: false,
            time_written: false,
            last_date_time_write_index: 0,
            control: Control(0),
            data: 0,
            data_pos: 0,
            data_bit: 0,
            cur_reg: RegIndex::new(0),
            reading: false,
            status1: Status1(0).with_powered_on(first_launch),
            status2: Status2(0x01),
            int1: [0, 0, 1],
            int2: [0; 3],
            clock_adjust: 0,
            free_reg: 0,
        }
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    #[inline]
    pub fn data(&self) -> u8 {
        self.data
    }

    #[inline]
    pub fn data_pos(&self) -> u8 {
        self.data_pos
    }

    #[inline]
    pub fn data_bit(&self) -> u8 {
        self.data_bit
    }

    #[inline]
    pub fn cur_reg(&self) -> RegIndex {
        self.cur_reg
    }

    #[inline]
    pub fn reading(&self) -> bool {
        self.reading
    }

    #[inline]
    pub fn status1(&self) -> Status1 {
        self.status1
    }

    #[inline]
    pub fn write_status1(&mut self, value: Status1) {
        self.status1.0 = (self.status1.0 & 0xF0) | (value.0 & 0x0E);
        if value.reset() {
            self.status1.0 &= 0x0E;
            self.write_status2(Status2(0));
            self.int1 = [0; 3];
            self.int2 = [0; 3];
            self.clock_adjust = 0;
            self.free_reg = 0;
            // TODO: Reset date and time to 1/1/2000, 12:00:00 AM
        }
    }

    #[inline]
    pub fn set_power_low(&mut self, value: bool) {
        self.status1.0 |= (value as u8) << 6;
    }

    #[inline]
    pub fn status2(&self) -> Status2 {
        self.status2
    }

    #[inline]
    pub fn write_status2(&mut self, value: Status2) {
        if value.test_mode() {
            #[cfg(feature = "log")]
            slog::warn!(self.logger, "Tried to enter unimplemented test mode");
        }
        if value.int1_mode() != 0 {
            #[cfg(feature = "log")]
            slog::warn!(self.logger, "Tried to enable unimplemented alarm 1");
        }
        if value.int2_enabled() {
            #[cfg(feature = "log")]
            slog::warn!(self.logger, "Tried to enable unimplemented alarm 2");
        }
        self.status2 = value;
    }

    fn latch_date(&mut self, date: Date) {
        self.latched_date_time[0] = to_bcd(date.years_since_2000);
        self.latched_date_time[1] = to_bcd(date.month);
        self.latched_date_time[2] = to_bcd(date.day);
        self.latched_date_time[3] = date.days_from_sunday;
    }

    fn latch_time(&mut self, time: Time) {
        self.latched_date_time[4] = ((time.hour >= 12) as u8) << 6
            | to_bcd(if self.status1.is_in_24_hour_mode() || time.hour < 12 {
                time.hour
            } else {
                time.hour - 12
            });
        self.latched_date_time[5] = to_bcd(time.minute);
        self.latched_date_time[6] = to_bcd(time.second);
    }

    fn flush_date_time_changes(&mut self) {
        let (mut date, mut time) = self.backend.get_date_time();
        if self.time_written {
            self.time_written = false;
            if self.last_date_time_write_index >= 4 {
                time.hour = from_bcd(self.latched_date_time[4] & 0x3F)
                    + if self.status1.is_in_24_hour_mode() {
                        0
                    } else {
                        12 * (self.latched_date_time[4] >> 6 & 1)
                    };
            }
            if self.last_date_time_write_index >= 5 {
                time.minute = from_bcd(self.latched_date_time[5] & 0x7F);
            }
            if self.last_date_time_write_index >= 6 {
                time.second = from_bcd(self.latched_date_time[6] & 0x7F);
            }
            if self.date_written {
                self.date_written = false;
                date.years_since_2000 = from_bcd(self.latched_date_time[0]);
                if self.last_date_time_write_index >= 1 {
                    date.month = from_bcd(self.latched_date_time[1] & 0x1F);
                }
                if self.last_date_time_write_index >= 2 {
                    date.day = from_bcd(self.latched_date_time[2] & 0x3F);
                }
                if self.last_date_time_write_index >= 3 {
                    date.days_from_sunday = from_bcd(self.latched_date_time[3] & 7);
                }
            }
            self.backend.set_date_time((date, time));
        }
    }

    fn read_byte(&mut self) -> u8 {
        // TODO: What happens when reading beyond the end of registers? Right now 0 is returned.
        if self.data_pos == 0 {
            // TODO: What happens when starting a transfer with a byte read?
            #[cfg(feature = "log")]
            slog::warn!(self.logger, "Started a transfer with a byte read");
            return 0;
        }

        match self.cur_reg {
            RegIndex::STATUS1 => {
                if self.data_pos == 1 {
                    let value = self.status1.0;
                    self.status1.0 &= !0xB0;
                    return value;
                }
            }

            RegIndex::STATUS2 => {
                if self.data_pos == 1 {
                    return self.status2.0;
                }
            }

            RegIndex::DATE_TIME => {
                if self.data_pos <= 7 {
                    if self.data_pos == 1 {
                        let (date, time) = self.backend.get_date_time();
                        self.latch_date(date);
                        self.latch_time(time);
                    }
                    return self.latched_date_time[(self.data_pos - 1) as usize];
                }
            }

            RegIndex::TIME => {
                if self.data_pos <= 3 {
                    if self.data_pos == 1 {
                        let time = self.backend.get_time();
                        self.latch_time(time);
                    }
                    return self.latched_date_time[(self.data_pos + 3) as usize];
                }
            }

            RegIndex::INT1 => {
                if self.status2.int1_mode() == 4 && self.data_pos <= 3 {
                    return self.int1[(self.data_pos - 1) as usize];
                } else if self.data_pos == 1 {
                    return self.int1[2];
                }
            }

            RegIndex::INT2 => {
                if self.data_pos <= 3 {
                    return self.int2[(self.data_pos - 1) as usize];
                }
            }

            RegIndex::ADJUST => {
                if self.data_pos == 1 {
                    return self.clock_adjust;
                }
            }

            RegIndex::FREE => {
                if self.data_pos == 1 {
                    return self.free_reg;
                }
            }

            _ => unreachable!(),
        }

        #[cfg(feature = "log")]
        slog::warn!(
            self.logger,
            "Reading unknown reg {} byte {}",
            self.cur_reg.get(),
            self.data_pos - 1,
        );
        0
    }

    #[allow(clippy::needless_return)]
    #[allow(clippy::match_same_arms)] // TODO: Remove this #[allow]
    fn write_byte(&mut self, value: u8) {
        if self.data_pos == 0 {
            // TODO: What if the CPU reads/writes after specifying the opposite action in the index
            //       byte?
            if value & 0xF == 0x6 {
                self.reading = value >> 7 != 0;
                self.cur_reg = RegIndex::new(value >> 4 & 7);
            } else {
                self.reading = value & 1 != 0;
                self.cur_reg = [
                    RegIndex::STATUS1,
                    RegIndex::STATUS2,
                    RegIndex::DATE_TIME,
                    RegIndex::TIME,
                    RegIndex::INT1,
                    RegIndex::INT2,
                    RegIndex::ADJUST,
                    RegIndex::FREE,
                ][(value >> 1 & 7) as usize];
            }
            return;
        }

        match self.cur_reg {
            RegIndex::STATUS1 => {
                if self.data_pos == 1 {
                    return self.write_status1(Status1(value));
                }
            }

            RegIndex::STATUS2 => {
                if self.data_pos == 1 {
                    return self.write_status2(Status2(value));
                }
            }

            RegIndex::DATE_TIME => {
                self.date_written = true;
                self.time_written = true;
                if self.data_pos <= 7 {
                    self.last_date_time_write_index = self.data_pos - 1;
                    return self.latched_date_time[self.last_date_time_write_index as usize] =
                        value;
                }
            }

            RegIndex::TIME => {
                self.time_written = true;
                if self.data_pos <= 3 {
                    self.last_date_time_write_index = self.data_pos + 3;
                    return self.latched_date_time[self.last_date_time_write_index as usize] =
                        value;
                }
            }

            RegIndex::INT1 => {
                if self.status2.int1_mode() == 4 {
                    match self.data_pos {
                        1 => return self.int1[0] = value & 0x87,
                        2 => return self.int1[1] = value,
                        3 => return self.int1[2] = value,
                        _ => {}
                    }
                } else if self.data_pos == 1 {
                    return self.int1[2] = value;
                }
            }

            RegIndex::INT2 => {
                if self.data_pos <= 3 {
                    match self.data_pos {
                        1 => return self.int2[0] = value & 0x87,
                        2 => return self.int2[1] = value,
                        3 => return self.int2[2] = value,
                        _ => {}
                    }
                }
            }

            RegIndex::ADJUST => {
                if self.data_pos == 1 {
                    return self.clock_adjust = value;
                }
            }

            RegIndex::FREE => {
                if self.data_pos == 1 {
                    return self.free_reg = value;
                }
            }

            _ => unreachable!(),
        }

        #[cfg(feature = "log")]
        slog::warn!(
            self.logger,
            "Writing unknown reg {} byte {}: {:#04X}",
            self.cur_reg.get(),
            self.data_pos - 1,
            value,
        );
    }

    #[inline]
    pub fn write_control(&mut self, value: Control) {
        // TODO: What happens if the data direction is changed in the middle of a byte transfer or
        //       during a command?
        if !value.clock_write() || !value.chipselect_write() {
            #[cfg(feature = "log")]
            slog::warn!(self.logger, "Set clock/chipselect direction to read");
        }
        if value.chipselect() {
            let is_first = !self.control.chipselect() && value.chipselect();
            let clock_falling_edge = self.control.clock() && !value.clock();
            if is_first {
                if !value.clock() {
                    // TODO: What's supposed to happen when CS rises while /SCK is low?
                    #[cfg(feature = "log")]
                    slog::warn!(self.logger, "Started a transfer with /SCK = LOW");
                }
                self.data = 0;
                self.data_pos = 0;
                self.flush_date_time_changes();
                self.data_bit = 0;
            } else if clock_falling_edge {
                if value.data_write() {
                    self.data |= value.data() << self.data_bit;
                    self.data_bit += 1;
                    if self.data_bit >= 8 {
                        self.data_bit = 0;
                        self.write_byte(self.data);
                        self.data = 0;
                        self.data_pos = self.data_pos.saturating_add(1);
                    }
                } else {
                    if self.data_bit == 0 {
                        self.data = self.read_byte();
                        self.data_pos = self.data_pos.saturating_add(1);
                    }
                    self.control.set_data(self.data >> self.data_bit);
                    self.data_bit = (self.data_bit + 1) & 7;
                }
            }
        }
        self.control.0 = if value.data_write() {
            value.0
        } else {
            (self.control.0 & 1) | (value.0 & 0xFFFE)
        };
    }
}
