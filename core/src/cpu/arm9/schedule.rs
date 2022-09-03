use crate::{
    cpu::{self, timers},
    emu,
    utils::{def_event_slot_index, def_event_slots, def_timestamp, schedule, Savestate},
};

def_timestamp!(#[derive(Savestate)] pub struct Timestamp);

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Savestate)]
pub enum Event {
    #[default]
    DsSlotRomDataReady, // Max 1
    DsSlotSpiDataReady,      // Max 1
    DivResultReady,          // Max 1
    SqrtResultReady,         // Max 1
    Timer(timers::Index),    // Max 4
    GxFifoStall,             // Max 1
    Engine3dCommandFinished, // Max 1
}

def_event_slots! {
    pub mod event_slots,
    EventSlotIndex,
    DS_SLOT_ROM,
    DS_SLOT_SPI,
    DIV,
    SQRT,
    TIMERS_START..TIMERS_END 4,
    GX_FIFO,
    ENGINE_3D,
}

def_event_slot_index!(bounded_esi, event_slots, pub struct EventSlotIndex(u8));

pub type RawSchedule = schedule::Schedule<Timestamp, Event, EventSlotIndex, { event_slots::LEN }>;

#[derive(Clone, Savestate)]
#[repr(C)]
pub struct Schedule {
    cur_time: Timestamp,
    target_time: Timestamp,
    schedule: RawSchedule,
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
    pub fn schedule(&self) -> &RawSchedule {
        &self.schedule
    }

    #[inline]
    pub(in crate::cpu) fn pop_pending_event(&mut self) -> Option<(Event, Timestamp)> {
        self.schedule.pop_pending_event(self.cur_time)
    }
}

impl const cpu::ScheduleConst for Schedule {
    type Timestamp = Timestamp;
    type Event = Event;
    type EventSlotIndex = EventSlotIndex;

    #[inline]
    fn timer_event_slot(i: timers::Index) -> EventSlotIndex {
        EventSlotIndex::new(event_slots::TIMERS_START.get() + i.get())
    }
}

impl cpu::Schedule for Schedule {
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
    fn set_event(&mut self, slot_index: EventSlotIndex, event: Event) {
        self.schedule.set_event(slot_index, event);
    }

    #[inline]
    fn set_timer_event(&mut self, i: timers::Index) {
        self.schedule.set_event(
            <Self as cpu::ScheduleConst>::timer_event_slot(i),
            Event::Timer(i),
        );
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
