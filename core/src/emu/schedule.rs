use crate::{
    gpu,
    utils::{
        bounded_int,
        schedule::{self, RawTimestamp},
    },
};
use core::ops::Add;

pub const DEFAULT_BATCH_DURATION: u32 = 64;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(pub RawTimestamp);

impl Add for Timestamp {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl From<RawTimestamp> for Timestamp {
    #[inline]
    fn from(v: RawTimestamp) -> Self {
        Self(v)
    }
}

impl From<Timestamp> for RawTimestamp {
    #[inline]
    fn from(v: Timestamp) -> Self {
        v.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Gpu(gpu::Event),         // Max 1
    Shutdown,                // Max 1
    Engine3dCommandFinished, // Max 1
}

impl Default for Event {
    #[inline]
    fn default() -> Self {
        Event::Gpu(gpu::Event::EndHDraw)
    }
}

pub mod event_slots {
    use crate::utils::def_event_slots;
    def_event_slots! {
        super::EventSlotIndex,
        GPU,
        SHUTDOWN,
        ENGINE_3D,
    }
}
bounded_int!(pub struct EventSlotIndex(u8), max (event_slots::LEN - 1) as u8);

impl From<usize> for EventSlotIndex {
    #[inline]
    fn from(v: usize) -> Self {
        assert!(v < event_slots::LEN);
        unsafe { Self::new_unchecked(v as u8) }
    }
}

impl From<EventSlotIndex> for usize {
    #[inline]
    fn from(v: EventSlotIndex) -> Self {
        v.get() as usize
    }
}

#[derive(Clone)]
#[repr(C)]
pub struct Schedule {
    cur_time: Timestamp,
    pub batch_cycles: Timestamp,
    schedule: schedule::Schedule<Timestamp, Event, EventSlotIndex, { event_slots::LEN }>,
}

impl Schedule {
    pub(super) fn new(batch_cycles: Timestamp) -> Self {
        Schedule {
            cur_time: Timestamp(0),
            batch_cycles,
            schedule: schedule::Schedule::new(),
        }
    }

    #[inline]
    pub fn cur_time(&self) -> Timestamp {
        self.cur_time
    }

    #[inline]
    pub(super) fn set_cur_time(&mut self, value: Timestamp) {
        self.cur_time = value;
    }

    #[inline]
    pub fn schedule(
        &self,
    ) -> &schedule::Schedule<Timestamp, Event, EventSlotIndex, { event_slots::LEN }> {
        &self.schedule
    }

    #[inline]
    pub(super) fn batch_end_time(&self) -> Timestamp {
        self.schedule
            .next_event_time()
            .min(self.cur_time + self.batch_cycles)
    }

    #[inline]
    pub(crate) fn set_event(&mut self, slot_index: EventSlotIndex, event: Event) {
        self.schedule.set_event(slot_index, event);
    }

    #[inline]
    pub(crate) fn schedule_event(&mut self, slot_index: EventSlotIndex, time: Timestamp) {
        self.schedule.schedule(slot_index, time);
    }

    #[inline]
    pub(crate) fn pop_pending_event(&mut self) -> Option<(Event, Timestamp)> {
        self.schedule.pop_pending_event(self.cur_time)
    }
}
