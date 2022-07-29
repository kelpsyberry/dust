use crate::{
    audio::Audio,
    cpu::{self, timers, Engine},
    ds_slot::DsSlot,
    emu::{self, Emu},
    utils::{def_event_slot_index, def_event_slots, def_timestamp, schedule, Savestate},
};

def_timestamp!(#[derive(Savestate)] pub struct Timestamp);

impl From<emu::Timestamp> for Timestamp {
    #[inline]
    fn from(v: emu::Timestamp) -> Self {
        Self(v.0)
    }
}

impl From<Timestamp> for emu::Timestamp {
    #[inline]
    fn from(v: Timestamp) -> Self {
        Self(v.0)
    }
}

impl From<timers::Timestamp> for Timestamp {
    #[inline]
    fn from(v: timers::Timestamp) -> Self {
        Self(v.0)
    }
}

impl From<Timestamp> for timers::Timestamp {
    #[inline]
    fn from(v: Timestamp) -> Self {
        Self(v.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Savestate)]
pub enum Event {
    Shutdown,           // Max 1
    DsSlotRomDataReady, // Max 1
    DsSlotSpiDataReady, // Max 1
    SpiDataReady,       // Max 1
    AudioSampleReady,   // Max 1
    #[cfg(feature = "xq-audio")]
    XqAudioSampleReady, // Max 1
    Timer(timers::Index), // Max 4
}

impl Default for Event {
    fn default() -> Self {
        Event::DsSlotRomDataReady
    }
}

def_event_slots! {
    pub mod event_slots,
    EventSlotIndex,
    SHUTDOWN,
    DS_SLOT_ROM,
    DS_SLOT_SPI,
    SPI,
    AUDIO,
    #[cfg(feature = "xq-audio")]
    XQ_AUDIO,
    TIMERS_START..TIMERS_END 4,
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
    pub(in crate::cpu) fn handle_pending_events<E: Engine>(emu: &mut Emu<E>) {
        while let Some((event, time)) = emu
            .arm7
            .schedule
            .schedule
            .pop_pending_event(emu.arm7.schedule.cur_time)
        {
            match event {
                Event::Shutdown => return,
                Event::DsSlotRomDataReady => DsSlot::handle_rom_data_ready(emu),
                Event::DsSlotSpiDataReady => emu.ds_slot.handle_spi_data_ready(),
                Event::SpiDataReady => emu.spi.handle_data_ready(&mut emu.arm7.irqs),
                Event::AudioSampleReady => Audio::handle_sample_ready(emu, time),
                #[cfg(feature = "xq-audio")]
                Event::XqAudioSampleReady => Audio::handle_xq_sample_ready(emu, time),
                Event::Timer(i) => emu.arm7.timers.handle_scheduled_overflow(
                    i,
                    time,
                    &mut emu.arm7.schedule,
                    &mut emu.arm7.irqs,
                ),
            }
        }
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
