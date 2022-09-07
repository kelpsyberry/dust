use super::Power;
use crate::{
    emu::{input, Timestamp},
    utils::{zeroed_box, Savestate},
};

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct ControlByte(pub u8): Debug {
        pub power_down_mode: u8 @ 0..=1,
        pub single_ended_mode: bool @ 2,
        pub res_8_bit: bool @ 3,
        pub channel: u8 @ 4..=6,
        pub start: bool @ 7,
    }
}

pub const MIC_SAMPLES_PER_FRAME: usize = (6 * 355 * 263 + 128) / 128;

pub trait MicBackend {
    fn start_frame(&mut self);
    fn read_frame_samples(&mut self, offset: usize, samples: &mut [i16]);
}

pub struct MicData {
    pub backend: Box<dyn MicBackend>,
    read_in_current_frame: bool,
    samples: Box<[i16; MIC_SAMPLES_PER_FRAME]>,
}

impl MicData {
    pub fn new(backend: Box<dyn MicBackend>) -> Self {
        MicData {
            backend,
            read_in_current_frame: false,
            samples: zeroed_box(),
        }
    }
}

#[derive(Savestate)]
#[load(in_place_only)]
pub struct Tsc {
    #[cfg(feature = "log")]
    #[savestate(skip)]
    logger: slog::Logger,
    #[savestate(skip)]
    is_ds_lite: bool,
    #[savestate(skip)]
    pub mic_data: Option<MicData>,
    mic_frame_start_time: Timestamp,
    pen_down: bool,
    pos: u8,
    cur_control_byte: ControlByte,
    data_out: u16,
    x_pos: u16,
    y_pos: u16,
}

impl Tsc {
    pub(super) fn new(
        is_ds_lite: bool,
        mic_backend: Option<Box<dyn MicBackend>>,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Self {
        Tsc {
            #[cfg(feature = "log")]
            logger,
            is_ds_lite,
            mic_data: mic_backend.map(MicData::new),
            mic_frame_start_time: Timestamp(0),
            pen_down: false,
            pos: 0,
            cur_control_byte: ControlByte(0),
            data_out: 0,
            x_pos: 0,
            y_pos: 0,
        }
    }

    #[inline]
    pub fn x_pos(&self) -> u16 {
        self.x_pos
    }

    #[inline]
    pub(crate) fn set_x_pos(&mut self, value: u16) {
        self.x_pos = value & 0xFFF;
    }

    #[inline]
    pub(crate) fn clear_x_pos(&mut self) {
        self.x_pos = 0;
    }

    #[inline]
    pub fn y_pos(&self) -> u16 {
        self.y_pos
    }

    #[inline]
    pub(crate) fn set_y_pos(&mut self, value: u16) {
        self.y_pos = value & 0xFFF;
    }

    #[inline]
    pub(crate) fn clear_y_pos(&mut self) {
        self.y_pos = 0xFFF;
    }

    #[inline]
    pub fn pen_down(&self) -> bool {
        self.pen_down
    }

    #[inline]
    pub(crate) fn set_pen_down(&mut self, value: bool, input_status: &mut input::Status) {
        self.pen_down = value;
        if self.cur_control_byte.power_down_mode() & 1 == 0 {
            input_status.set_pen_down(!value);
        }
    }

    pub(crate) fn start_frame(&mut self, time: Timestamp) {
        self.mic_frame_start_time = time;
        if let Some(mic_data) = &mut self.mic_data {
            mic_data.backend.start_frame();
        }
    }

    fn handle_control_byte(
        &mut self,
        value: ControlByte,
        time: Timestamp,
        _power: &Power,
        input_status: &mut input::Status,
    ) -> u16 {
        if value.power_down_mode() & 1 == 0 {
            input_status.set_pen_down(!self.pen_down);
        } else {
            input_status.set_pen_down(true);
        }
        self.cur_control_byte = value;
        #[allow(clippy::match_same_arms)]
        let result = match value.channel() {
            0 => {
                #[cfg(feature = "log")]
                if value.single_ended_mode() {
                    slog::debug!(
                        self.logger,
                        "Reading from unimplemented channel 0 (temperature 0)"
                    );
                } else {
                    slog::warn!(
                        self.logger,
                        "Reading from channel 0 (temperature 0) in differential mode"
                    );
                }
                0xFFF
            }
            1 => self.y_pos,
            2 => {
                #[cfg(feature = "log")]
                if !value.single_ended_mode() {
                    slog::warn!(
                        self.logger,
                        "Reading from channel 2 (battery voltage) in differential mode"
                    );
                }
                0xFFF
            }
            3 => {
                #[cfg(feature = "log")]
                slog::debug!(
                    self.logger,
                    "Reading from unimplemented channel 3 (Z1-position)"
                );
                0xFFF
            }
            4 => {
                #[cfg(feature = "log")]
                slog::debug!(
                    self.logger,
                    "Reading from unimplemented channel 4 (Z2-position)"
                );
                0xFFF
            }
            5 => self.x_pos,
            6 => {
                if value.single_ended_mode() {
                    let sample = if let Some(mic_data) = &mut self.mic_data {
                        let offset = ((time.0 - self.mic_frame_start_time.0) / 128) as usize;
                        if !mic_data.read_in_current_frame {
                            mic_data
                                .backend
                                .read_frame_samples(offset, &mut mic_data.samples[offset..]);
                        }
                        mic_data.samples[offset]
                    } else {
                        0
                    };
                    // TODO: How to apply the gain value? It can be assumed to be a value in decibel
                    //       (20 dB, 40 dB, 80 dB, 160 dB resulting in a 10x, 100x, 10^4x and
                    //       10^8x increase in amplitude), but the baseline amplitude should be
                    //       known.
                    // if power.mic_amplifier_enabled() {
                    //     sample = (sample as i32)
                    //         .saturating_mul(
                    //             [10, 100, 10_000, 100_000_000]
                    //                 [power.mic_amplifier_gain_control().gain_shift() as usize],
                    //         )
                    //         .clamp(-0x8000, 0x7FFF) as i16;
                    // }
                    (sample as u16).wrapping_add(0x8000) >> 4
                } else {
                    if !self.is_ds_lite {
                        #[cfg(feature = "log")]
                        slog::warn!(
                            self.logger,
                            "Reading from channel 6 (mic input) in differential mode"
                        );
                    }
                    0xFFF
                }
            }
            _ => {
                #[cfg(feature = "log")]
                if value.single_ended_mode() {
                    slog::debug!(
                        self.logger,
                        "Reading from unimplemented channel 7 (temperature 1)"
                    );
                } else {
                    slog::warn!(
                        self.logger,
                        "Reading from channel 7 (temperature 1) in differential mode"
                    );
                }
                0xFFF
            }
        };
        (if value.res_8_bit() {
            result & !0xF
        } else {
            result
        }) << 3
    }

    pub(super) fn handle_byte(
        &mut self,
        value: u8,
        is_first: bool,
        time: Timestamp,
        power: &Power,
        input_status: &mut input::Status,
    ) -> u8 {
        if is_first {
            self.pos = 0;
        }
        if self.pos == 0 {
            if ControlByte(value).start() {
                self.pos = 1;
                self.data_out =
                    self.handle_control_byte(ControlByte(value), time, power, input_status);
            }
            0
        } else {
            let result = (self.data_out >> 8) as u8;
            self.data_out <<= 8;
            if self.pos == 2 {
                if ControlByte(value).start() {
                    self.pos = 1;
                    self.data_out =
                        self.handle_control_byte(ControlByte(value), time, power, input_status);
                }
            } else {
                self.pos = 2;
            }
            result
        }
    }
}
