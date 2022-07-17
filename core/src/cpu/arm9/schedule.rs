use crate::{
    cpu::{self, timers},
    emu,
    utils::{
        schedule::{self, RawTimestamp},
        Savestate,
    },
};
use core::ops::Add;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Savestate)]
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

impl From<emu::Timestamp> for Timestamp {
    #[inline]
    fn from(v: emu::Timestamp) -> Self {
        Self(v.0 << 1)
    }
}

impl From<Timestamp> for emu::Timestamp {
    #[inline]
    fn from(v: Timestamp) -> Self {
        Self(v.0 >> 1)
    }
}

impl From<timers::Timestamp> for Timestamp {
    #[inline]
    fn from(v: timers::Timestamp) -> Self {
        Self(v.0 << 1)
    }
}

impl From<Timestamp> for timers::Timestamp {
    #[inline]
    fn from(v: Timestamp) -> Self {
        Self(v.0 >> 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Savestate)]
pub enum Event {
    DsSlotRomDataReady,      // Max 1
    DsSlotSpiDataReady,      // Max 1
    DivResultReady,          // Max 1
    SqrtResultReady,         // Max 1
    Timer(timers::Index),    // Max 4
    GxFifoStall,             // Max 1
    Engine3dCommandFinished, // Max 1
}

impl Default for Event {
    fn default() -> Self {
        Event::DsSlotRomDataReady
    }
}

pub mod event_slots {
    use crate::utils::def_event_slots;
    def_event_slots! {
        super::EventSlotIndex,
        DS_SLOT_ROM,
        DS_SLOT_SPI,
        DIV,
        SQRT,
        TIMERS_START..TIMERS_END 4,
        GX_FIFO,
        ENGINE_3D,
    }
}

mod bounded {
    use crate::utils::{bounded_int, bounded_int_savestate};
    bounded_int!(pub struct EventSlotIndex(u8), max (super::event_slots::LEN - 1) as u8);
    bounded_int_savestate!(EventSlotIndex(u8));
}
pub use bounded::*;

impl From<usize> for EventSlotIndex {
    #[inline]
    fn from(v: usize) -> Self {
        assert!(v < event_slots::LEN as usize);
        unsafe { Self::new_unchecked(v as u8) }
    }
}

impl From<EventSlotIndex> for usize {
    #[inline]
    fn from(v: EventSlotIndex) -> Self {
        v.get() as usize
    }
}

#[derive(Clone, Savestate)]
#[repr(C)]
pub struct Schedule {
    cur_time: Timestamp,
    target_time: Timestamp,
    schedule: schedule::Schedule<Timestamp, Event, EventSlotIndex, { event_slots::LEN }>,
}

impl Schedule {
    pub(super) fn new() -> Self {
        Schedule {
            cur_time: Timestamp(0),
            target_time: Timestamp(0),
            schedule: schedule::Schedule::new(),
        }
    }

    #[inline]
    pub fn schedule(
        &self,
    ) -> &schedule::Schedule<Timestamp, Event, EventSlotIndex, { event_slots::LEN }> {
        &self.schedule
    }

    #[inline]
    pub(in crate::cpu) fn pop_pending_event(&mut self) -> Option<(Event, Timestamp)> {
        self.schedule.pop_pending_event(self.cur_time)
    }
}

impl cpu::Schedule for Schedule {
    type Timestamp = Timestamp;
    type Event = Event;
    type EventSlotIndex = EventSlotIndex;

    #[inline]
    fn cur_time(&self) -> Timestamp {
        self.cur_time
    }

    #[inline]
    fn set_cur_time(&mut self, value: Timestamp) {
        self.cur_time = value;
    }

    #[inline]
    fn target_time(&self) -> Timestamp {
        self.target_time
    }

    #[inline]
    fn set_target_time(&mut self, target: Timestamp) {
        self.target_time = target;
    }

    #[inline]
    fn timer_event_slot(i: timers::Index) -> EventSlotIndex {
        EventSlotIndex::new(event_slots::TIMERS_START.get() + i.get())
    }

    #[inline]
    fn set_event(&mut self, slot_index: EventSlotIndex, event: Event) {
        self.schedule.set_event(slot_index, event);
    }

    #[inline]
    fn set_timer_event(&mut self, slot_index: EventSlotIndex, i: timers::Index) {
        self.schedule.set_event(slot_index, Event::Timer(i));
    }

    #[inline]
    fn schedule_event(&mut self, slot_index: EventSlotIndex, time: Timestamp) {
        self.set_target_time_before(time);
        self.schedule.schedule(slot_index, time);
    }

    #[inline]
    fn cancel_event(&mut self, slot_index: EventSlotIndex) {
        self.schedule.cancel(slot_index);
    }
}
