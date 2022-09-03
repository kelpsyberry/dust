use crate::{
    gpu,
    utils::{def_event_slot_index, def_event_slots, def_timestamp, schedule, Savestate},
};

pub const DEFAULT_BATCH_DURATION: u32 = 64;

def_timestamp!(#[derive(Savestate)] pub struct Timestamp);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Savestate)]
pub enum Event {
    Gpu(gpu::Event), // Max 1
    #[default]
    Shutdown, // Max 1
    Engine3dCommandFinished, // Max 1
}

def_event_slots! {
    pub mod event_slots,
    EventSlotIndex,
    GPU,
    SHUTDOWN,
    ENGINE_3D,
}

def_event_slot_index!(bounded_esi, event_slots, pub struct EventSlotIndex(u8));

pub type RawSchedule = schedule::Schedule<Timestamp, Event, EventSlotIndex, { event_slots::LEN }>;

#[derive(Clone, Savestate)]
#[load(in_place_only)]
#[repr(C)]
pub struct Schedule {
    cur_time: Timestamp,
    #[savestate(skip)]
    pub batch_cycles: Timestamp,
    schedule: RawSchedule,
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
    pub fn schedule(&self) -> &RawSchedule {
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
