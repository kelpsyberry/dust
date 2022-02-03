use super::{event_slots, Event, Schedule, Timestamp};
use crate::{cpu::Schedule as _, utils::bitfield_debug};

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Control(pub u16) {
        pub input_64_bit: bool @ 0,
        pub busy: bool @ 15,
    }
}

pub struct SqrtEngine {
    control: Control,
    input: u64,
    result: u32,
}

impl SqrtEngine {
    pub(super) fn new(schedule: &mut Schedule) -> Self {
        schedule.set_event(event_slots::SQRT, Event::SqrtResultReady);
        SqrtEngine {
            control: Control(0),
            input: 0,
            result: 0,
        }
    }

    fn schedule_data_ready(&mut self, schedule: &mut Schedule) {
        if self.control.busy() {
            schedule.cancel_event(event_slots::SQRT);
        }
        self.control.set_busy(true);
        schedule.schedule_event(event_slots::SQRT, schedule.cur_time() + Timestamp(26));
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    #[inline]
    pub fn write_control(&mut self, value: Control, schedule: &mut Schedule) {
        self.control.0 = (self.control.0 & 0x8000) | (value.0 & 0x0001);
        self.schedule_data_ready(schedule);
    }

    #[inline]
    pub fn input(&self) -> u64 {
        self.input
    }

    #[inline]
    pub fn write_input(&mut self, value: u64, schedule: &mut Schedule) {
        self.input = value;
        self.schedule_data_ready(schedule);
    }

    #[inline]
    pub fn result(&self) -> u32 {
        self.result
    }

    pub(crate) fn handle_result_ready(&mut self) {
        self.control.set_busy(false);
        let (mut input, mut bit) = if self.control.input_64_bit() {
            (self.input, 1 << 62)
        } else {
            (self.input as u32 as u64, 1 << 30)
        };
        let mut result = 0;
        while bit > input {
            bit >>= 2;
        }
        while bit != 0 {
            if input >= result + bit {
                input -= result + bit;
                result = (result >> 1) + bit;
            } else {
                result >>= 1;
            }
            bit >>= 2;
        }
        self.result = result as u32;
    }
}
