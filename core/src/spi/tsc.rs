use crate::emu::input;

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub const struct ControlByte(pub u8): Debug {
        pub power_down_mode: u8 @ 0..=1,
        pub single_ended_mode: bool @ 2,
        pub res_8_bit: bool @ 3,
        pub channel: u8 @ 4..=6,
        pub start: bool @ 7,
    }
}

pub struct Tsc {
    #[cfg(feature = "log")]
    logger: slog::Logger,
    is_ds_lite: bool,
    pen_down: bool,
    pos: u8,
    cur_control_byte: ControlByte,
    data_out: u16,
    x_pos: u16,
    y_pos: u16,
}

impl Tsc {
    pub(super) fn new(is_ds_lite: bool, #[cfg(feature = "log")] logger: slog::Logger) -> Self {
        Tsc {
            #[cfg(feature = "log")]
            logger,
            is_ds_lite,
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

    fn handle_control_byte(&mut self, value: ControlByte, input_status: &mut input::Status) -> u16 {
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
                    slog::warn!(
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
                slog::warn!(
                    self.logger,
                    "Reading from unimplemented channel 3 (Z1-position)"
                );
                0xFFF
            }
            4 => {
                #[cfg(feature = "log")]
                slog::warn!(
                    self.logger,
                    "Reading from unimplemented channel 4 (Z2-position)"
                );
                0xFFF
            }
            5 => self.x_pos,
            6 => {
                if value.single_ended_mode() {
                    #[cfg(feature = "log")]
                    slog::warn!(
                        self.logger,
                        "Reading from unimplemented channel 6 (mic input)"
                    );
                    0xFFF
                } else if self.is_ds_lite {
                    0xFFF
                } else {
                    #[cfg(feature = "log")]
                    slog::warn!(
                        self.logger,
                        "Reading from channel 6 (mic input) in differential mode"
                    );
                    0xFFF
                }
            }
            _ => {
                #[cfg(feature = "log")]
                if value.single_ended_mode() {
                    slog::warn!(
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
        input_status: &mut input::Status,
    ) -> u8 {
        if is_first {
            self.pos = 0;
        }
        if self.pos == 0 {
            if ControlByte(value).start() {
                self.pos = 1;
                self.data_out = self.handle_control_byte(ControlByte(value), input_status);
            }
            0
        } else {
            let result = (self.data_out >> 8) as u8;
            self.data_out <<= 8;
            if self.pos == 2 {
                if ControlByte(value).start() {
                    self.pos = 1;
                    self.data_out = self.handle_control_byte(ControlByte(value), input_status);
                }
            } else {
                self.pos = 2;
            }
            result
        }
    }
}
