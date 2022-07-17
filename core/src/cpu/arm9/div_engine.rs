use super::{event_slots, Event, Schedule, Timestamp};
use crate::{
    cpu::Schedule as _,
    utils::{schedule::RawTimestamp, Savestate},
};

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct Control(pub u16): Debug {
        pub mode: u8 @ 0..=1,
        pub div_by_0: bool @ 14,
        pub busy: bool @ 15,
    }
}

#[derive(Savestate)]
pub struct DivEngine {
    control: Control,
    num: i64,
    denom: i64,
    quot: i64,
    rem: i64,
}

impl DivEngine {
    pub(super) fn new(schedule: &mut Schedule) -> Self {
        schedule.set_event(event_slots::DIV, Event::DivResultReady);
        DivEngine {
            control: Control(0),
            num: 0,
            denom: 0,
            quot: 0,
            rem: 0,
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
    pub fn write_control(&mut self, value: Control, schedule: &mut Schedule) {
        self.control.0 = (self.control.0 & 0xC000) | (value.0 & 0x0003);
        self.schedule_data_ready(schedule);
    }

    #[inline]
    pub fn num(&self) -> i64 {
        self.num
    }

    #[inline]
    pub fn write_num(&mut self, value: i64, schedule: &mut Schedule) {
        self.num = value;
        self.schedule_data_ready(schedule);
    }

    #[inline]
    pub fn denom(&self) -> i64 {
        self.denom
    }

    #[inline]
    pub fn write_denom(&mut self, value: i64, schedule: &mut Schedule) {
        self.denom = value;
        self.schedule_data_ready(schedule);
    }

    #[inline]
    pub fn quot(&self) -> i64 {
        self.quot
    }

    #[inline]
    pub fn rem(&self) -> i64 {
        self.rem
    }

    pub(crate) fn handle_result_ready(&mut self) {
        self.control = self.control.with_busy(false).with_div_by_0(self.denom == 0);
        match self.control.mode() {
            0 => {
                // 32/32
                let num = self.num as i32;
                let denom = self.denom as i32;
                if denom == 0 {
                    self.quot = if num >= 0 { -1 } else { 1 } ^ 0xFFFF_FFFF_0000_0000_u64 as i64;
                    self.rem = num as i64;
                } else if num == i32::MIN && denom == -1 {
                    self.quot = num as u32 as i64;
                    self.rem = 0;
                } else {
                    self.quot = (num / denom) as i64;
                    self.rem = (num % denom) as i64;
                }
            }

            1 | 3 => {
                // 64/32
                let num = self.num;
                let denom = self.denom as i32;
                if denom == 0 {
                    self.quot = if num >= 0 { -1 } else { 1 };
                    self.rem = self.num;
                } else if num == i64::MIN && denom == -1 {
                    self.quot = num;
                    self.rem = 0;
                } else {
                    self.quot = num / denom as i64;
                    self.rem = num % denom as i64;
                }
            }

            _ => {
                // 64/64
                let num = self.num;
                let denom = self.denom;
                if self.denom == 0 {
                    self.quot = if num >= 0 { -1 } else { 1 };
                    self.rem = self.num;
                } else if num == i64::MIN && denom == -1 {
                    self.quot = num;
                    self.rem = 0;
                } else {
                    self.quot = num / denom;
                    self.rem = num % denom;
                }
            }
        }
    }
}
