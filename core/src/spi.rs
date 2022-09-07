pub mod firmware;
pub mod power;
pub mod tsc;

use crate::{
    cpu::{arm7, Schedule as _},
    emu::{self, input},
    flash::Flash,
    utils::Savestate,
    Model,
};
use power::Power;
use tsc::Tsc;

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct Control(pub u16): Debug {
        pub baud_rate: u8 @ 0..=1,
        pub busy: bool @ 7,
        pub device: u8 @ 8..=9,
        // TODO (GBATEK says this is bugged...?)
        pub transfer_size: bool @ 10,
        pub hold: bool @ 11,
        pub irq_enabled: bool @ 14,
        pub enabled: bool @ 15,
    }
}

#[derive(Savestate)]
#[load(in_place_only)]
pub struct Controller {
    #[cfg(feature = "log")]
    #[savestate(skip)]
    logger: slog::Logger,
    control: Control,
    data_out: u8,
    firmware_hold: bool,
    pub firmware: Flash,
    power_hold: bool,
    pub power: Power,
    touchscreen_hold: bool,
    pub tsc: Tsc,
}

impl Controller {
    pub(crate) fn new(
        model: Model,
        firmware: Flash,
        mic_backend: Option<Box<dyn tsc::MicBackend>>,
        arm7_schedule: &mut arm7::Schedule,
        emu_schedule: &mut emu::Schedule,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Self {
        arm7_schedule.set_event(arm7::event_slots::SPI, arm7::Event::SpiDataReady);
        Controller {
            control: Control(0),
            data_out: 0,
            firmware_hold: false,
            firmware,
            power_hold: false,
            power: Power::new(
                matches!(model, Model::Lite | Model::IqueLite),
                arm7_schedule,
                emu_schedule,
            ),
            touchscreen_hold: false,
            tsc: Tsc::new(
                model == Model::Lite,
                mic_backend,
                #[cfg(feature = "log")]
                logger.new(slog::o!("tsc" => "")),
            ),
            #[cfg(feature = "log")]
            logger,
        }
    }

    #[inline]
    pub const fn control(&self) -> Control {
        self.control
    }

    #[inline]
    pub fn write_control(&mut self, value: Control) {
        // TODO: What happens if SPICNT is modified while busy?
        if !value.enabled() && self.control.enabled() {
            // Turning off SPI should clear all chipselect pins
            self.firmware_hold = false;
            self.power_hold = false;
            self.touchscreen_hold = false;
        }
        self.control.0 = (self.control.0 & 0x0080) | (value.0 & 0xCF03);
    }

    #[inline]
    pub const fn read_data(&self) -> u8 {
        // TODO: What's actually returned while busy/disabled? Right now it's assumed to be the
        // previous value
        self.data_out
    }

    pub(crate) fn handle_data_ready(&mut self, arm7_irqs: &mut arm7::Irqs) {
        self.control.set_busy(false);
        if self.control.irq_enabled() {
            arm7_irqs.write_requested(arm7_irqs.requested().with_spi_data_ready(true), ());
        }
    }

    pub fn write_data(
        &mut self,
        value: u8,
        arm7_schedule: &mut arm7::Schedule,
        emu_schedule: &mut emu::Schedule,
        input_status: &mut input::Status,
    ) {
        // TODO: What happens if SPICNT bit 11 is set before changing the device?
        if self.control.busy() || !self.control.enabled() {
            return;
        }
        self.control.set_busy(true);
        self.data_out = match self.control.device() {
            0 => {
                let is_first = !self.power_hold;
                self.power_hold = self.control.hold();
                self.power
                    .handle_byte(value, is_first, arm7_schedule, emu_schedule)
            }

            1 => {
                let is_first = !self.firmware_hold;
                self.firmware_hold = self.control.hold();
                let is_last = !self.firmware_hold;
                self.firmware.handle_byte(value, is_first, is_last)
            }

            2 => {
                let is_first = !self.touchscreen_hold;
                self.touchscreen_hold = self.control.hold();
                self.tsc.handle_byte(
                    value,
                    is_first,
                    arm7_schedule.cur_time().into(),
                    &self.power,
                    input_status,
                )
            }

            _ => {
                // TODO: What's actually supposed to happen?
                #[cfg(feature = "log")]
                slog::warn!(
                    self.logger,
                    "Accessing unknown device 3: {:#04X}{}",
                    value,
                    if self.control.hold() { "(hold)" } else { "" },
                );
                0
            }
        };
        // 8 bits at (8 << baud rate) cycles per bit
        let end_time = arm7_schedule.cur_time() + arm7::Timestamp(64 << self.control.baud_rate());
        arm7_schedule.schedule_event(arm7::event_slots::SPI, end_time);
    }
}
