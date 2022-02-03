use super::Schedule;
use crate::{
    cpu::{self, dma, timers, Schedule as _},
    utils::bitfield_debug,
};

pub trait ScheduleUpdate {
    fn stop_execution(self);
}

impl ScheduleUpdate for () {
    fn stop_execution(self) {}
}

impl ScheduleUpdate for &mut Schedule {
    fn stop_execution(self) {
        self.set_target_time(self.cur_time());
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct IrqFlags(pub u32) {
        pub vblank: bool @ 0,                       // x
        pub hblank: bool @ 1,                       // x
        pub vcount_match: bool @ 2,                 // x
        pub timer0: bool @ 3,                       // x
        pub timer1: bool @ 4,                       // x
        pub timer2: bool @ 5,                       // x
        pub timer3: bool @ 6,                       // x
        pub dma0: bool @ 8,                         // x
        pub dma1: bool @ 9,                         // x
        pub dma2: bool @ 10,                        // x
        pub dma3: bool @ 11,                        // x
        pub keypad: bool @ 12,                      // -
        pub gba_slot_ext: bool @ 13,                // -
        pub ipc_sync: bool @ 16,                    // x
        pub ipc_send_fifo_empty: bool @ 17,         // x
        pub ipc_recv_fifo_not_empty: bool @ 18,     // x
        pub ds_slot_transfer_complete: bool @ 19,   // x
        pub ds_slot_ext: bool @ 20,                 // -
        pub gx_fifo: bool @ 21,                     // x
    }
}

pub struct Irqs {
    enabled: IrqFlags,
    requested: IrqFlags,
    master_enable: bool,
    halted: bool,
    cpu_irq_line: bool,
    enabled_in_cpsr: bool,
    triggered: bool,
}

impl Irqs {
    pub(super) fn new() -> Self {
        Irqs {
            enabled: IrqFlags(0),
            requested: IrqFlags(0),
            master_enable: false,
            halted: false,
            cpu_irq_line: false,
            enabled_in_cpsr: false,
            triggered: false,
        }
    }

    #[inline]
    pub fn enabled(&self) -> IrqFlags {
        self.enabled
    }

    #[inline]
    pub fn requested(&self) -> IrqFlags {
        self.requested
    }

    #[inline]
    pub fn master_enable(&self) -> bool {
        self.master_enable
    }

    #[inline]
    pub fn halted(&self) -> bool {
        self.halted
    }

    #[inline]
    pub fn halt<S: ScheduleUpdate>(&mut self, schedule: S) {
        self.halted = !self.cpu_irq_line;
        if self.halted {
            schedule.stop_execution();
        }
    }

    #[inline]
    pub fn cpu_irq_line(&self) -> bool {
        self.cpu_irq_line
    }

    #[inline]
    fn set_irq_line<S: ScheduleUpdate>(&mut self, value: bool, schedule: S) {
        self.cpu_irq_line = value;
        self.halted &= !value;
        self.update_triggered(schedule);
    }

    #[inline]
    pub fn enabled_in_cpsr(&self) -> bool {
        self.enabled_in_cpsr
    }

    pub(in super::super) fn set_enabled_in_cpsr<S: ScheduleUpdate>(
        &mut self,
        value: bool,
        schedule: S,
    ) {
        self.enabled_in_cpsr = value;
        self.update_triggered(schedule);
    }

    #[inline]
    fn update_triggered<S: ScheduleUpdate>(&mut self, schedule: S) {
        self.triggered = self.cpu_irq_line && self.enabled_in_cpsr;
        if self.triggered {
            schedule.stop_execution();
        }
    }

    #[inline]
    pub fn triggered(&self) -> bool {
        self.triggered
    }

    #[inline]
    fn update_pending<S: ScheduleUpdate>(&mut self, schedule: S) {
        if self.master_enable {
            self.set_irq_line(self.enabled.0 & self.requested.0 != 0, schedule);
        }
    }

    #[inline]
    pub fn write_enabled<S: ScheduleUpdate>(&mut self, value: IrqFlags, schedule: S) {
        self.enabled = IrqFlags(value.0 & 0x003F_3F7F);
        self.update_pending(schedule);
    }

    #[inline]
    pub fn write_requested<S: ScheduleUpdate>(&mut self, value: IrqFlags, schedule: S) {
        self.requested = IrqFlags(value.0 & 0x003F_3F7F);
        self.update_pending(schedule);
    }

    #[inline]
    pub fn write_master_enable<S: ScheduleUpdate>(&mut self, value: bool, schedule: S) {
        self.master_enable = value;
        self.set_irq_line(value && self.enabled.0 & self.requested.0 != 0, schedule);
    }
}

impl cpu::Irqs for Irqs {
    type Schedule = Schedule;

    fn request_timer(&mut self, i: timers::Index, schedule: &mut Schedule) {
        self.write_requested(IrqFlags(self.requested().0 | 8 << i.get()), schedule);
    }

    fn request_dma(&mut self, i: dma::Index) {
        self.write_requested(IrqFlags(self.requested().0 | 0x100 << i.get()), ());
    }
}
