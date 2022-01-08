use super::{event_slots, Event, Schedule, Timestamp};
use crate::{
    cpu::Schedule as _,
    utils::{bitfield_debug, schedule::RawTimestamp},
};

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Control(pub u16) {
        pub mode: u8 @ 0..=1,
        pub div_by_0: bool @ 14,
        pub busy: bool @ 15,
    }
}

pub struct DivEngine {
    control: Control,
    numerator: i64,
    denominator: i64,
    quotient: i64,
    remainder: i64,
}

impl DivEngine {
    pub(super) fn new(schedule: &mut Schedule) -> Self {
        schedule.set_event(event_slots::DIV, Event::DivResultReady);
        DivEngine {
            control: Control(0),
            numerator: 0,
            denominator: 0,
            quotient: 0,
            remainder: 0,
        }
    }

    fn schedule_data_ready(&mut self, schedule: &mut Schedule) {
        if self.control.busy() {
            schedule.cancel_event(event_slots::DIV);
        }
        self.control.set_busy(true);
        schedule.schedule_event(
            event_slots::DIV,
            schedule.cur_time()
                + Timestamp(36 + (((self.control.mode() != 0) as RawTimestamp) << 5)),
        );
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    #[inline]
    pub fn set_control(&mut self, value: Control, schedule: &mut Schedule) {
        self.control.0 = (self.control.0 & 0xC000) | (value.0 & 0x0003);
        self.schedule_data_ready(schedule);
    }

    #[inline]
    pub fn numerator(&self) -> i64 {
        self.numerator
    }

    #[inline]
    pub fn set_numerator(&mut self, value: i64, schedule: &mut Schedule) {
        self.numerator = value;
        self.schedule_data_ready(schedule);
    }

    #[inline]
    pub fn denominator(&self) -> i64 {
        self.denominator
    }

    #[inline]
    pub fn set_denominator(&mut self, value: i64, schedule: &mut Schedule) {
        self.denominator = value;
        self.schedule_data_ready(schedule);
    }

    #[inline]
    pub fn quotient(&self) -> i64 {
        self.quotient
    }

    #[inline]
    pub fn remainder(&self) -> i64 {
        self.remainder
    }

    pub(crate) fn handle_result_ready(&mut self) {
        self.control = self
            .control
            .with_busy(false)
            .with_div_by_0(self.denominator == 0);
        match self.control.mode() {
            0 => {
                // 32/32
                let numerator = self.numerator as i32;
                let denominator = self.denominator as i32;
                if denominator == 0 {
                    self.quotient =
                        if numerator >= 0 { -1 } else { 1 } ^ 0xFFFF_FFFF_0000_0000_u64 as i64;
                    self.remainder = numerator as i64;
                } else if numerator == i32::MIN && denominator == -1 {
                    self.quotient = numerator as u32 as i64;
                    self.remainder = 0;
                } else {
                    self.quotient = (numerator / denominator) as i64;
                    self.remainder = (numerator % denominator) as i64;
                }
            }

            1 | 3 => {
                // 64/32
                let numerator = self.numerator;
                let denominator = self.denominator as i32;
                if denominator == 0 {
                    self.quotient = if numerator >= 0 { -1 } else { 1 };
                    self.remainder = self.numerator;
                } else if numerator == i64::MIN && denominator == -1 {
                    self.quotient = numerator;
                    self.remainder = 0;
                } else {
                    self.quotient = numerator / denominator as i64;
                    self.remainder = numerator % denominator as i64;
                }
            }

            _ => {
                // 64/64
                let numerator = self.numerator;
                let denominator = self.denominator;
                if self.denominator == 0 {
                    self.quotient = if numerator >= 0 { -1 } else { 1 };
                    self.remainder = self.numerator;
                } else if numerator == i64::MIN && denominator == -1 {
                    self.quotient = numerator;
                    self.remainder = 0;
                } else {
                    self.quotient = numerator / denominator;
                    self.remainder = numerator % denominator;
                }
            }
        }
    }
}
