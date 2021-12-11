use super::{channel, Audio, Control};
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
                0 => return self.control.0 as u8,
                1 => return (self.control.0 >> 8) as u8,
                4 => return self.bias as u8,
                5 => return (self.bias >> 8) as u8,
                2 | 3 | 6 | 7 => return 0,
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
                0 => return self.control.0,
                4 => return self.bias,
                2 | 6 => return 0,
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
                0 => return self.control.0 as u32,
                4 => return self.bias as u32,
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
            let channel = &mut self.channels[addr as usize >> 4 & 0xF];
            match addr & 0xF {
                0 => channel.set_control(channel::Control(
                    (channel.control().0 & 0xFFFF_FF00) | value as u32,
                )),
                1 => channel.set_control(channel::Control(
                    (channel.control().0 & 0xFFFF_00FF) | (value as u32) << 8,
                )),
                2 => channel.set_control(channel::Control(
                    (channel.control().0 & 0xFF00_FFFF) | (value as u32) << 16,
                )),
                3 => channel.set_control(channel::Control(
                    (channel.control().0 & 0xFF_FFFF) | (value as u32) << 24,
                )),
                4 => channel.set_src_addr((channel.src_addr() & 0xFFFF_FF00) | value as u32),
                5 => channel.set_src_addr((channel.src_addr() & 0xFFFF_00FF) | (value as u32) << 8),
                6 => {
                    channel.set_src_addr((channel.src_addr() & 0xFF00_FFFF) | (value as u32) << 16);
                }
                8 => channel
                    .set_timer_reload((channel.timer_reload() & 0xFF00) as u16 | value as u16),
                9 => channel
                    .set_timer_reload((channel.timer_reload() & 0xFF) as u16 | (value as u16) << 8),
                0xA => channel.set_loop_start((channel.loop_start() & 0xFF00) | value as u16),
                0xB => channel.set_loop_start((channel.loop_start() & 0xFF) | (value as u16) << 8),
                0xC => channel.set_loop_len((channel.loop_len() & 0xFFFF_FF00) | value as u32),
                0xD => {
                    channel.set_loop_len((channel.loop_len() & 0xFFFF_00FF) | (value as u32) << 8);
                }
                0xE => channel.set_loop_len((channel.loop_len() & 0xFFFF) | (value as u32) << 16),
                _ => {}
            }
        } else {
            match addr & 0x1F {
                0 => self.set_control(Control((self.control.0 & 0xFF00) | value as u16)),
                1 => self.set_control(Control((self.control.0 & 0xFF) | (value as u16) << 8)),
                4 => self.set_bias((self.bias & 0xFF00) | value as u16),
                5 => self.set_bias((self.bias & 0xFF) | (value as u16) << 8),
                2 | 3 | 6 | 7 => {}
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
            let channel = &mut self.channels[addr as usize >> 4 & 0xF];
            match addr & 0xE {
                0 => channel.set_control(channel::Control(
                    (channel.control().0 & 0xFFFF_0000) | value as u32,
                )),
                2 => channel.set_control(channel::Control(
                    (channel.control().0 & 0xFFFF) | (value as u32) << 16,
                )),
                4 => channel.set_src_addr((channel.src_addr() & 0xFFFF_0000) | value as u32),
                6 => channel.set_src_addr((channel.src_addr() & 0xFFFF) | (value as u32) << 16),
                8 => channel.set_timer_reload(value),
                0xA => channel.set_loop_start(value),
                0xC => channel.set_loop_len((channel.loop_len() & 0xFFFF_0000) | value as u32),
                _ => channel.set_loop_len((channel.loop_len() & 0xFFFF) | (value as u32) << 16),
            }
        } else {
            match addr & 0x1E {
                0 => self.set_control(Control(value)),
                4 => self.set_bias(value),
                2 | 6 => {}
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
            let channel = &mut self.channels[addr as usize >> 4 & 0xF];
            match addr & 0xC {
                0 => channel.set_control(channel::Control(value)),
                4 => channel.set_src_addr(value),
                8 => {
                    channel.set_timer_reload(value as u16);
                    channel.set_loop_start((value >> 16) as u16);
                }
                _ => channel.set_loop_len(value),
            }
        } else {
            match addr & 0x1C {
                0 => self.set_control(Control(value as u16)),
                4 => self.set_bias(value as u16),
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
