use super::{Engine3d, GxStatus};
use crate::{
    cpu::{self, bus::AccessType},
    emu::Emu,
};

impl Engine3d {
    pub(crate) fn read_8<A: AccessType>(&mut self, addr: u16) -> u8 {
        match addr & 0xFFF {
            0x320 => self.line_buffer_level(),

            0x600 => self.gx_status().0 as u8,
            0x601 => (self.gx_status().0 >> 8) as u8,
            0x602 => (self.gx_status().0 >> 16) as u8,
            0x603 => (self.gx_status().0 >> 24) as u8,

            0x604 => self.poly_vert_ram_level().0 as u8,
            0x605 => (self.poly_vert_ram_level().0 >> 8) as u8,
            0x606 => (self.poly_vert_ram_level().0 >> 16) as u8,
            0x607 => (self.poly_vert_ram_level().0 >> 24) as u8,

            0x640..=0x67F => {
                if self.clip_mtx_needs_recalculation {
                    self.update_clip_mtx();
                }
                (self.cur_clip_mtx.get(addr as usize >> 2 & 0xF) >> ((addr & 3) << 3)) as u8
            }

            0x680..=0x68B => {
                (self.cur_pos_vec_mtxs[1].get(addr as usize >> 2 & 0xF) >> ((addr & 3) << 3)) as u8
            }
            0x68C..=0x697 => {
                (self.cur_pos_vec_mtxs[1].get(1 + (addr as usize >> 2 & 0xF)) >> ((addr & 3) << 3))
                    as u8
            }
            0x698..=0x6A3 => {
                (self.cur_pos_vec_mtxs[1].get(2 + (addr as usize >> 2 & 0xF)) >> ((addr & 3) << 3))
                    as u8
            }

            _ => {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(self.logger, "Unknown read8 @ {:#06X}", addr);
                }
                0
            }
        }
    }

    pub(crate) fn read_16<A: AccessType>(&mut self, addr: u16) -> u16 {
        match addr & 0xFFE {
            0x320 => self.line_buffer_level() as u16,

            0x600 => self.gx_status().0 as u16,
            0x602 => (self.gx_status().0 >> 16) as u16,

            0x604 => self.poly_vert_ram_level().0 as u16,
            0x606 => (self.poly_vert_ram_level().0 >> 16) as u16,

            0x640..=0x67F => {
                if self.clip_mtx_needs_recalculation {
                    self.update_clip_mtx();
                }
                (self.cur_clip_mtx.get(addr as usize >> 2 & 0xF) >> ((addr & 2) << 3)) as u16
            }

            0x680..=0x68A => {
                (self.cur_pos_vec_mtxs[1].get(addr as usize >> 2 & 0xF) >> ((addr & 2) << 3)) as u16
            }
            0x68C..=0x696 => {
                (self.cur_pos_vec_mtxs[1].get(1 + (addr as usize >> 2 & 0xF)) >> ((addr & 2) << 3))
                    as u16
            }
            0x698..=0x6A2 => {
                (self.cur_pos_vec_mtxs[1].get(2 + (addr as usize >> 2 & 0xF)) >> ((addr & 2) << 3))
                    as u16
            }

            _ => {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(self.logger, "Unknown read16 @ {:#06X}", addr);
                }
                0
            }
        }
    }

    pub(crate) fn read_32<A: AccessType>(&mut self, addr: u16) -> u32 {
        match addr & 0xFFC {
            0x320 => self.line_buffer_level() as u32,

            0x600 => self.gx_status().0,
            0x604 => self.poly_vert_ram_level().0,

            0x640..=0x67F => {
                if self.clip_mtx_needs_recalculation {
                    self.update_clip_mtx();
                }
                self.cur_clip_mtx.get(addr as usize >> 2 & 0xF) as u32
            }

            0x680..=0x688 => self.cur_pos_vec_mtxs[1].get(addr as usize >> 2 & 0xF) as u32,
            0x68C..=0x694 => self.cur_pos_vec_mtxs[1].get(1 + (addr as usize >> 2 & 0xF)) as u32,
            0x698..=0x6A0 => self.cur_pos_vec_mtxs[1].get(2 + (addr as usize >> 2 & 0xF)) as u32,

            _ => {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(self.logger, "Unknown read32 @ {:#06X}", addr);
                }
                0
            }
        }
    }

    pub(crate) fn write_8<A: AccessType, E: cpu::Engine>(emu: &mut Emu<E>, addr: u16, value: u8) {
        match addr & 0xFFE {
            0x601 => {
                if emu.gpu.engine_3d.gx_enabled {
                    emu.gpu.engine_3d.write_gx_status(
                        GxStatus(
                            (emu.gpu.engine_3d.gx_status().0 & 0xFFFF_00FF) | (value as u32) << 8,
                        ),
                        &mut emu.arm9,
                    );
                }
            }
            0x603 => {
                if emu.gpu.engine_3d.gx_enabled {
                    emu.gpu.engine_3d.write_gx_status(
                        GxStatus(
                            (emu.gpu.engine_3d.gx_status().0 & 0x00FF_7FFF) | (value as u32) << 24,
                        ),
                        &mut emu.arm9,
                    );
                }
            }

            0x600 | 0x602 => {}

            _ =>
            {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        emu.gpu.engine_3d.logger,
                        "Unknown write8 @ {:#06X}: {:#04X}",
                        addr,
                        value
                    );
                }
            }
        }
    }

    pub(crate) fn write_16<A: AccessType, E: cpu::Engine>(emu: &mut Emu<E>, addr: u16, value: u16) {
        match addr & 0xFFE {
            0x600 => {
                if emu.gpu.engine_3d.gx_enabled {
                    emu.gpu.engine_3d.write_gx_status(
                        GxStatus((emu.gpu.engine_3d.gx_status().0 & 0xFFFF_0000) | value as u32),
                        &mut emu.arm9,
                    );
                }
            }
            0x602 => {
                if emu.gpu.engine_3d.gx_enabled {
                    emu.gpu.engine_3d.write_gx_status(
                        GxStatus(
                            (emu.gpu.engine_3d.gx_status().0 & 0x0000_7FFF) | (value as u32) << 16,
                        ),
                        &mut emu.arm9,
                    );
                }
            }

            _ =>
            {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        emu.gpu.engine_3d.logger,
                        "Unknown write16 @ {:#06X}: {:#06X}",
                        addr,
                        value
                    );
                }
            }
        }
    }

    pub(crate) fn write_32<A: AccessType, E: cpu::Engine>(emu: &mut Emu<E>, addr: u16, value: u32) {
        match addr & 0xFFC {
            0x400..=0x43C => {
                if emu.gpu.engine_3d.gx_enabled {
                    Self::write_packed_command(emu, value);
                }
            }

            0x440..=0x5FC => {
                if emu.gpu.engine_3d.gx_enabled {
                    Self::write_unpacked_command(emu, (addr >> 2) as u8, value);
                }
            }

            0x600 => {
                if emu.gpu.engine_3d.gx_enabled {
                    emu.gpu
                        .engine_3d
                        .write_gx_status(GxStatus(value), &mut emu.arm9);
                }
            }

            _ =>
            {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        emu.gpu.engine_3d.logger,
                        "Unknown write32 @ {:#06X}: {:#010X}",
                        addr,
                        value
                    );
                }
            }
        }
    }
}
