use super::{capture, channel, Audio, Control};
use crate::cpu::bus::AccessType;

impl Audio {
    pub(crate) fn read_8<A: AccessType>(&mut self, addr: u32) -> u8 {
        if addr & 0x100 == 0 {
            let channel = &self.channels[addr as usize >> 4 & 0xF];
            match addr & 0xF {
                0 => return channel.control().0 as u8,
                1 => return (channel.control().0 >> 8) as u8,
                2 => return (channel.control().0 >> 16) as u8,
                3 => return (channel.control().0 >> 24) as u8,
                _ => {}
            }
        } else {
            match addr & 0x1F {
                0x00 => return self.control.0 as u8,
                0x01 => return (self.control.0 >> 8) as u8,

                0x04 => return self.bias as u8,
                0x05 => return (self.bias >> 8) as u8,

                0x08 => return self.capture[0].control().0,
                0x09 => return self.capture[1].control().0,

                0x10 => return self.capture[0].dst_addr() as u8,
                0x11 => return (self.capture[0].dst_addr() >> 8) as u8,
                0x12 => return (self.capture[0].dst_addr() >> 16) as u8,
                0x13 => return (self.capture[0].dst_addr() >> 24) as u8,

                0x14 => return self.capture[0].buffer_words() as u8,
                0x15 => return (self.capture[0].buffer_words() >> 8) as u8,

                0x18 => return self.capture[1].dst_addr() as u8,
                0x19 => return (self.capture[1].dst_addr() >> 8) as u8,
                0x1A => return (self.capture[1].dst_addr() >> 16) as u8,
                0x1B => return (self.capture[1].dst_addr() >> 24) as u8,

                0x1C => return self.capture[1].buffer_words() as u8,
                0x1D => return (self.capture[1].buffer_words() >> 8) as u8,

                0x02 | 0x03 | 0x06 | 0x07 | 0x16 | 0x17 | 0x1E | 0x1F => return 0,

                _ => {}
            }
        }
        #[cfg(feature = "log")]
        if !A::IS_DEBUG {
            slog::warn!(self.logger, "Unknown read8 @ {:#04X}", addr);
        }
        0
    }

    pub(crate) fn read_16<A: AccessType>(&mut self, addr: u32) -> u16 {
        if addr & 0x100 == 0 {
            let channel = &self.channels[addr as usize >> 4 & 0xF];
            match addr & 0xE {
                0 => return channel.control().0 as u16,
                2 => return (channel.control().0 >> 16) as u16,
                _ => {}
            }
        } else {
            match addr & 0x1E {
                0x00 => return self.control.0,
                0x04 => return self.bias,

                0x08 => {
                    return self.capture[0].control().0 as u16
                        | (self.capture[1].control().0 as u16) << 8
                }

                0x10 => return self.capture[0].dst_addr() as u16,
                0x12 => return (self.capture[0].dst_addr() >> 16) as u16,

                0x14 => return self.capture[0].buffer_words(),

                0x18 => return self.capture[1].dst_addr() as u16,
                0x1A => return (self.capture[1].dst_addr() >> 16) as u16,

                0x1C => return self.capture[1].buffer_words(),

                0x2 | 0x6 | 0x16 | 0x1E => return 0,

                _ => {}
            }
        }
        #[cfg(feature = "log")]
        if !A::IS_DEBUG {
            slog::warn!(self.logger, "Unknown read16 @ {:#04X}", addr);
        }
        0
    }

    pub(crate) fn read_32<A: AccessType>(&mut self, addr: u32) -> u32 {
        if addr & 0x100 == 0 {
            let channel = &self.channels[addr as usize >> 4 & 0xF];
            if addr & 0xC == 0 {
                return channel.control().0;
            }
        } else {
            match addr & 0x1C {
                0x00 => return self.control.0 as u32,
                0x04 => return self.bias as u32,

                0x08 => {
                    return self.capture[0].control().0 as u32
                        | (self.capture[1].control().0 as u32) << 8
                }

                0x10 => return self.capture[0].dst_addr(),
                0x14 => return self.capture[0].buffer_words() as u32,
                0x18 => return self.capture[1].dst_addr(),
                0x1C => return self.capture[1].buffer_words() as u32,

                _ => {}
            }
        }
        #[cfg(feature = "log")]
        if !A::IS_DEBUG {
            slog::warn!(self.logger, "Unknown read32 @ {:#04X}", addr);
        }
        0
    }

    pub(crate) fn write_8<A: AccessType>(&mut self, addr: u32, value: u8) {
        if addr & 0x100 == 0 {
            let i = addr as usize >> 4 & 0xF;
            let channel = &mut self.channels[i];
            match addr & 0xF {
                0x0 => channel.write_control(channel::Control(
                    (channel.control().0 & 0xFFFF_FF00) | value as u32,
                )),
                0x1 => channel.write_control(channel::Control(
                    (channel.control().0 & 0xFFFF_00FF) | (value as u32) << 8,
                )),
                0x2 => channel.write_control(channel::Control(
                    (channel.control().0 & 0xFF00_FFFF) | (value as u32) << 16,
                )),
                0x3 => channel.write_control(channel::Control(
                    (channel.control().0 & 0x00FF_FFFF) | (value as u32) << 24,
                )),

                0x4 => channel.write_src_addr((channel.src_addr() & 0xFFFF_FF00) | value as u32),
                0x5 => {
                    channel
                        .write_src_addr((channel.src_addr() & 0xFFFF_00FF) | (value as u32) << 8);
                }
                0x6 => {
                    channel
                        .write_src_addr((channel.src_addr() & 0xFF00_FFFF) | (value as u32) << 16);
                }
                0x7 => {
                    channel
                        .write_src_addr((channel.src_addr() & 0x00FF_FFFF) | (value as u32) << 24);
                }

                0x8 => {
                    let new_timer_reload = (channel.timer_reload() & 0xFF00) as u16 | value as u16;
                    channel.write_timer_reload(new_timer_reload);
                    match i {
                        1 => self.capture[0].timer_reload = new_timer_reload,
                        3 => self.capture[1].timer_reload = new_timer_reload,
                        _ => {}
                    }
                }
                0x9 => {
                    let new_timer_reload =
                        (channel.timer_reload() & 0x00FF) as u16 | (value as u16) << 8;
                    channel.write_timer_reload(new_timer_reload);
                    match i {
                        1 => self.capture[0].timer_reload = new_timer_reload,
                        3 => self.capture[1].timer_reload = new_timer_reload,
                        _ => {}
                    }
                }
                0xA => channel.write_loop_start((channel.loop_start() & 0xFF00) | value as u16),
                0xB => {
                    channel.write_loop_start((channel.loop_start() & 0x00FF) | (value as u16) << 8);
                }

                0xC => channel.write_loop_len((channel.loop_len() & 0xFFFF_FF00) | value as u32),
                0xD => {
                    channel
                        .write_loop_len((channel.loop_len() & 0xFFFF_00FF) | (value as u32) << 8);
                }
                0xE => channel.write_loop_len((channel.loop_len() & 0xFFFF) | (value as u32) << 16),

                _ => {}
            }
        } else {
            match addr & 0x1F {
                0x00 => self.write_control(Control((self.control.0 & 0xFF00) | value as u16)),
                0x01 => {
                    self.write_control(Control((self.control.0 & 0x00FF) | (value as u16) << 8));
                }

                0x04 => self.write_bias((self.bias & 0xFF00) | value as u16),
                0x05 => self.write_bias((self.bias & 0x00FF) | (value as u16) << 8),

                0x08 => self.capture[0].write_control(capture::Control(value)),
                0x09 => self.capture[1].write_control(capture::Control(value)),

                0x10 => self.capture[0]
                    .write_dst_addr((self.capture[0].dst_addr() & 0xFFFF_FF00) | value as u32),
                0x11 => self.capture[0].write_dst_addr(
                    (self.capture[0].dst_addr() & 0xFFFF_00FF) | (value as u32) << 8,
                ),
                0x12 => self.capture[0].write_dst_addr(
                    (self.capture[0].dst_addr() & 0xFF00_FFFF) | (value as u32) << 16,
                ),
                0x13 => self.capture[0].write_dst_addr(
                    (self.capture[0].dst_addr() & 0x00FF_FFFF) | (value as u32) << 24,
                ),

                0x14 => self.capture[0]
                    .write_buffer_words((self.capture[0].buffer_words() & 0xFF00) | value as u16),
                0x15 => self.capture[0].write_buffer_words(
                    (self.capture[0].buffer_words() & 0x00FF) | (value as u16) << 8,
                ),

                0x18 => self.capture[1]
                    .write_dst_addr((self.capture[1].dst_addr() & 0xFFFF_FF00) | value as u32),
                0x19 => self.capture[1].write_dst_addr(
                    (self.capture[1].dst_addr() & 0xFFFF_00FF) | (value as u32) << 8,
                ),
                0x1A => self.capture[1].write_dst_addr(
                    (self.capture[1].dst_addr() & 0xFF00_FFFF) | (value as u32) << 16,
                ),
                0x1B => self.capture[1].write_dst_addr(
                    (self.capture[1].dst_addr() & 0x00FF_FFFF) | (value as u32) << 24,
                ),

                0x1C => self.capture[1]
                    .write_buffer_words((self.capture[1].buffer_words() & 0xFF00) | value as u16),
                0x1D => self.capture[1].write_buffer_words(
                    (self.capture[1].buffer_words() & 0x00FF) | (value as u16) << 8,
                ),

                0x02 | 0x03 | 0x06 | 0x07 | 0x16 | 0x17 | 0x1E | 0x1F => {}

                _ =>
                {
                    #[cfg(feature = "log")]
                    if !A::IS_DEBUG {
                        slog::warn!(
                            self.logger,
                            "Unknown write8 @ {:#05X}: {:#04X}",
                            addr,
                            value
                        );
                    }
                }
            }
        }
    }

    pub(crate) fn write_16<A: AccessType>(&mut self, addr: u32, value: u16) {
        if addr & 0x100 == 0 {
            let i = addr as usize >> 4 & 0xF;
            let channel = &mut self.channels[i];
            match addr & 0xE {
                0x0 => channel.write_control(channel::Control(
                    (channel.control().0 & 0xFFFF_0000) | value as u32,
                )),
                0x2 => channel.write_control(channel::Control(
                    (channel.control().0 & 0x0000_FFFF) | (value as u32) << 16,
                )),

                0x4 => channel.write_src_addr((channel.src_addr() & 0xFFFF_0000) | value as u32),
                0x6 => {
                    channel
                        .write_src_addr((channel.src_addr() & 0x0000_FFFF) | (value as u32) << 16);
                }

                0x8 => {
                    channel.write_timer_reload(value);
                    match i {
                        1 => self.capture[0].timer_reload = value,
                        3 => self.capture[1].timer_reload = value,
                        _ => {}
                    }
                }

                0xA => channel.write_loop_start(value),

                0xC => channel.write_loop_len((channel.loop_len() & 0xFFFF_0000) | value as u32),
                _ => {
                    channel
                        .write_loop_len((channel.loop_len() & 0x0000_FFFF) | (value as u32) << 16);
                }
            }
        } else {
            match addr & 0x1E {
                0x00 => self.write_control(Control(value)),
                0x04 => self.write_bias(value),

                0x08 => {
                    self.capture[0].write_control(capture::Control(value as u8));
                    self.capture[1].write_control(capture::Control((value >> 8) as u8));
                }

                0x10 => self.capture[0]
                    .write_dst_addr((self.capture[0].dst_addr() & 0xFFFF_0000) | value as u32),
                0x12 => self.capture[0].write_dst_addr(
                    (self.capture[0].dst_addr() & 0x0000_FFFF) | (value as u32) << 16,
                ),

                0x14 => self.capture[0].write_buffer_words(value),

                0x18 => self.capture[1]
                    .write_dst_addr((self.capture[1].dst_addr() & 0xFFFF_0000) | value as u32),
                0x1A => self.capture[1].write_dst_addr(
                    (self.capture[1].dst_addr() & 0x0000_FFFF) | (value as u32) << 16,
                ),

                0x1C => self.capture[1].write_buffer_words(value),

                0x2 | 0x6 | 0x16 | 0x1E => {}

                _ =>
                {
                    #[cfg(feature = "log")]
                    if !A::IS_DEBUG {
                        slog::warn!(
                            self.logger,
                            "Unknown write16 @ {:#05X}: {:#06X}",
                            addr,
                            value
                        );
                    }
                }
            }
        }
    }

    pub(crate) fn write_32<A: AccessType>(&mut self, addr: u32, value: u32) {
        if addr & 0x100 == 0 {
            let i = addr as usize >> 4 & 0xF;
            let channel = &mut self.channels[i];
            match addr & 0xC {
                0 => channel.write_control(channel::Control(value)),
                4 => channel.write_src_addr(value),
                8 => {
                    channel.write_timer_reload(value as u16);
                    channel.write_loop_start((value >> 16) as u16);
                    match i {
                        1 => self.capture[0].timer_reload = value as u16,
                        3 => self.capture[1].timer_reload = value as u16,
                        _ => {}
                    }
                }
                _ => channel.write_loop_len(value),
            }
        } else {
            match addr & 0x1C {
                0x00 => self.write_control(Control(value as u16)),
                0x04 => self.write_bias(value as u16),

                0x08 => {
                    self.capture[0].write_control(capture::Control(value as u8));
                    self.capture[1].write_control(capture::Control((value >> 8) as u8));
                }

                0x10 => self.capture[0].write_dst_addr(value),
                0x14 => self.capture[0].write_buffer_words(value as u16),
                0x18 => self.capture[1].write_dst_addr(value),
                0x1C => self.capture[1].write_buffer_words(value as u16),

                _ =>
                {
                    #[cfg(feature = "log")]
                    if !A::IS_DEBUG {
                        slog::warn!(
                            self.logger,
                            "Unknown write32 @ {:#05X}: {:#010X}",
                            addr,
                            value
                        );
                    }
                }
            }
        }
    }
}
