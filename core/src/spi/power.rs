use crate::{
    cpu::{arm7, Schedule as _},
    emu,
    utils::Savestate,
};

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct RegIndex(pub u8): Debug {
        pub reg: u8 @ 0..=6,
        pub read: bool @ 7,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct Control(pub u8): Debug {
        pub sound_amplifier_enabled: bool @ 0,
        pub sound_amplifier_muted: bool @ 1,
        pub lower_backlight_enabled: bool @ 2,
        pub upper_backlight_enabled: bool @ 3,
        pub power_led_blinking: bool @ 4,
        pub power_led_blink_speed: bool @ 5,
        pub shutdown: bool @ 6,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Savestate)]
pub enum SoundLevel {
    Muted,
    Low,
    Normal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Savestate)]
pub enum PowerLedState {
    Normal,
    Blinking,
    BlinkingFast,
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct MicAmplifierGainControl(pub u8): Debug {
        pub gain_shift: u8 @ 0..=1,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct DsLiteBacklightControl(pub u8): Debug {
        pub backlight_level: u8 @ 0..=1,
        pub max_level_with_ext_power: bool @ 2,
        pub external_power: bool @ 3,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Savestate)]
pub enum DsLiteBacklightLevel {
    Low,
    Medium,
    High,
    Max,
}

#[derive(Savestate)]
#[load(in_place_only)]
pub struct Power {
    #[savestate(skip)]
    is_ds_lite: bool,
    #[savestate(skip)]
    reg_mask: u8,
    cur_reg_index: RegIndex,
    control: Control,
    sound_level: SoundLevel,
    power_led_state: PowerLedState,
    pub battery_low: bool,
    mic_amplifier_enabled: bool,
    mic_amplifier_gain_control: MicAmplifierGainControl,
    mic_gain: u8,
    ds_lite_backlight_control: DsLiteBacklightControl,
    ds_lite_backlight_level: DsLiteBacklightLevel,
}

impl Power {
    pub(crate) fn new(
        is_ds_lite: bool,
        arm7_schedule: &mut arm7::Schedule,
        emu_schedule: &mut emu::Schedule,
    ) -> Self {
        arm7_schedule.set_event(arm7::event_slots::SHUTDOWN, arm7::Event::Shutdown);
        emu_schedule.set_event(emu::event_slots::SHUTDOWN, emu::Event::Shutdown);
        Power {
            is_ds_lite,
            reg_mask: if is_ds_lite { 7 } else { 3 },
            cur_reg_index: RegIndex(0),
            control: Control(0),
            sound_level: if is_ds_lite {
                SoundLevel::Muted
            } else {
                SoundLevel::Low
            },
            power_led_state: PowerLedState::Normal,
            battery_low: false,
            mic_amplifier_enabled: false,
            mic_amplifier_gain_control: MicAmplifierGainControl(0),
            mic_gain: 0,
            ds_lite_backlight_control: DsLiteBacklightControl(0x40),
            ds_lite_backlight_level: DsLiteBacklightLevel::Low,
        }
    }

    #[inline]
    pub fn is_ds_lite(&self) -> bool {
        self.is_ds_lite
    }

    #[inline]
    pub fn cur_reg_index(&self) -> RegIndex {
        self.cur_reg_index
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    #[inline]
    pub fn request_shutdown(
        &mut self,
        arm7_schedule: &mut arm7::Schedule,
        emu_schedule: &mut emu::Schedule,
    ) {
        arm7_schedule.schedule_event(arm7::event_slots::SHUTDOWN, arm7_schedule.cur_time());
        emu_schedule.schedule_event(emu::event_slots::SHUTDOWN, emu_schedule.cur_time());
    }

    #[inline]
    pub fn write_control(
        &mut self,
        value: Control,
        arm7_schedule: &mut arm7::Schedule,
        emu_schedule: &mut emu::Schedule,
    ) {
        if self.is_ds_lite {
            self.control.0 = value.0 & 0x7D;
            self.sound_level = if self.control.sound_amplifier_enabled() {
                SoundLevel::Normal
            } else {
                SoundLevel::Muted
            };
        } else {
            self.control.0 = value.0 & 0x7F;
            self.sound_level = if self.control.sound_amplifier_enabled() {
                if self.control.sound_amplifier_muted() {
                    SoundLevel::Muted
                } else {
                    SoundLevel::Normal
                }
            } else {
                SoundLevel::Low
            };
        }
        self.power_led_state = if self.control.power_led_blinking() {
            if self.control.power_led_blink_speed() {
                PowerLedState::BlinkingFast
            } else {
                PowerLedState::Blinking
            }
        } else {
            PowerLedState::Normal
        };
        if value.shutdown() {
            self.request_shutdown(arm7_schedule, emu_schedule);
        }
    }

    #[inline]
    pub fn sound_level(&self) -> SoundLevel {
        self.sound_level
    }

    pub fn set_sound_level(&mut self, value: SoundLevel) -> bool {
        if self.is_ds_lite {
            match value {
                SoundLevel::Normal => {
                    self.control.set_sound_amplifier_enabled(true);
                }
                SoundLevel::Low => return false,
                SoundLevel::Muted => {
                    self.control.set_sound_amplifier_enabled(false);
                }
            }
        } else {
            match value {
                SoundLevel::Normal => {
                    self.control = self
                        .control
                        .with_sound_amplifier_enabled(true)
                        .with_sound_amplifier_muted(false);
                }
                SoundLevel::Low => {
                    self.control = self
                        .control
                        .with_sound_amplifier_enabled(false)
                        .with_sound_amplifier_muted(false);
                }
                SoundLevel::Muted => {
                    self.control = self
                        .control
                        .with_sound_amplifier_enabled(true)
                        .with_sound_amplifier_muted(true);
                }
            }
        }
        true
    }

    #[inline]
    pub fn power_led_state(&self) -> PowerLedState {
        self.power_led_state
    }

    #[inline]
    pub fn set_power_led_state(&mut self, value: PowerLedState) {
        match value {
            PowerLedState::Normal => {
                self.control = self
                    .control
                    .with_power_led_blinking(false)
                    .with_power_led_blink_speed(false);
            }
            PowerLedState::Blinking => {
                self.control = self
                    .control
                    .with_power_led_blinking(true)
                    .with_power_led_blink_speed(false);
            }
            PowerLedState::BlinkingFast => {
                self.control = self
                    .control
                    .with_power_led_blinking(true)
                    .with_power_led_blink_speed(true);
            }
        }
    }

    #[inline]
    pub fn mic_amplifier_enabled(&self) -> bool {
        self.mic_amplifier_enabled
    }

    #[inline]
    pub fn set_mic_amplifier_enabled(&mut self, value: bool) {
        self.mic_amplifier_enabled = value;
        self.mic_gain = if value {
            20 << self.mic_amplifier_gain_control.gain_shift()
        } else {
            0
        };
    }

    #[inline]
    pub fn mic_amplifier_gain_control(&self) -> MicAmplifierGainControl {
        self.mic_amplifier_gain_control
    }

    #[inline]
    pub fn set_mic_amplifier_gain_control(&mut self, value: MicAmplifierGainControl) {
        self.mic_amplifier_gain_control.0 = value.0 & 3;
        if self.mic_amplifier_enabled {
            self.mic_gain = 20 << value.gain_shift();
        }
    }

    #[inline]
    pub fn mic_gain(&self) -> u8 {
        self.mic_gain
    }

    #[inline]
    pub fn set_mic_gain(&mut self, value: u8) -> bool {
        if value == 0 {
            self.mic_amplifier_gain_control.0 = 0;
            self.mic_amplifier_enabled = false;
        } else {
            self.mic_amplifier_gain_control.set_gain_shift(match value {
                20 => 0,
                40 => 1,
                80 => 2,
                160 => 3,
                _ => return false,
            });
            self.mic_amplifier_enabled = true;
        }
        true
    }

    #[inline]
    pub fn ds_lite_backlight_control(&self) -> DsLiteBacklightControl {
        self.ds_lite_backlight_control
    }

    fn update_ds_lite_backlight_level(&mut self) {
        self.ds_lite_backlight_level = if self.ds_lite_backlight_control.external_power()
            && self.ds_lite_backlight_control.max_level_with_ext_power()
        {
            DsLiteBacklightLevel::Max
        } else {
            match self.ds_lite_backlight_control.backlight_level() {
                0 => DsLiteBacklightLevel::Low,
                1 => DsLiteBacklightLevel::Medium,
                2 => DsLiteBacklightLevel::High,
                _ => DsLiteBacklightLevel::Max,
            }
        };
    }

    #[inline]
    pub fn set_ds_lite_backlight_control(&mut self, value: DsLiteBacklightControl) {
        self.ds_lite_backlight_control.0 =
            (self.ds_lite_backlight_control.0 & 0xF8) | (value.0 & 7);
        self.update_ds_lite_backlight_level();
    }

    #[inline]
    pub fn set_ds_lite_external_power(&mut self, value: bool) {
        self.ds_lite_backlight_control.set_external_power(value);
        self.update_ds_lite_backlight_level();
    }

    #[inline]
    pub fn ds_lite_backlight_level(&self) -> DsLiteBacklightLevel {
        self.ds_lite_backlight_level
    }

    #[inline]
    pub fn set_ds_lite_backlight_level(
        &mut self,
        value: DsLiteBacklightLevel,
        max_level_with_ext_power: bool,
    ) {
        self.ds_lite_backlight_control = self
            .ds_lite_backlight_control
            .with_backlight_level(match value {
                DsLiteBacklightLevel::Low => 0,
                DsLiteBacklightLevel::Medium => 1,
                DsLiteBacklightLevel::High => 2,
                DsLiteBacklightLevel::Max => 3,
            })
            .with_max_level_with_ext_power(max_level_with_ext_power);
        self.update_ds_lite_backlight_level();
    }

    pub(super) fn handle_byte(
        &mut self,
        value: u8,
        is_first: bool,
        arm7_schedule: &mut arm7::Schedule,
        emu_schedule: &mut emu::Schedule,
    ) -> u8 {
        // TODO: What response is returned after writes?
        if is_first {
            self.cur_reg_index = RegIndex(value);
            return 0;
        }
        if self.cur_reg_index.read() {
            match self.cur_reg_index.0 & self.reg_mask {
                0 => self.control.0,
                1 => self.battery_low as u8,
                2 => self.mic_amplifier_enabled as u8,
                3 => self.mic_amplifier_gain_control.0,
                _ => {
                    (self.ds_lite_backlight_control.0 & 0xFC) | (self.ds_lite_backlight_level as u8)
                }
            }
        } else {
            match self.cur_reg_index.0 & self.reg_mask {
                0 => self.write_control(Control(value), arm7_schedule, emu_schedule),
                1 => {}
                2 => self.set_mic_amplifier_enabled(value & 1 != 0),
                3 => self.set_mic_amplifier_gain_control(MicAmplifierGainControl(value)),
                _ => self.set_ds_lite_backlight_control(DsLiteBacklightControl(value)),
            }
            0
        }
    }
}
