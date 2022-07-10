use super::{decode_rgb_5, Color, Engine3d, GxStatus};
use crate::{
    cpu::{self, bus::AccessType},
    emu::Emu,
};

fn fog_density(mut value: u8) -> u8 {
    value &= 0x7F;
    if value == 0x7F {
        0x80
    } else {
        value
    }
}

fn set_rgb5_low(mut color: Color, value: u8) -> Color {
    color[0] = value & 0x1F;
    color[1] = (color[1] & 0x18) | value >> 5;
    color
}

fn set_rgb5_high(mut color: Color, value: u8) -> Color {
    color[1] = (color[1] & 7) | (value << 3 & 0x18);
    color[2] = value >> 2 & 0x1F;
    color
}

impl Engine3d {
    #[allow(clippy::match_same_arms)]
    pub(crate) fn read_8<A: AccessType>(&mut self, addr: u16) -> u8 {
        match addr & 0xFFF {
            0x320 => self.line_buffer_level(),
            0x321..=0x323 => 0,

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

    #[allow(clippy::match_same_arms)]
    pub(crate) fn read_16<A: AccessType>(&mut self, addr: u16) -> u16 {
        match addr & 0xFFE {
            0x320 => self.line_buffer_level() as u16,
            0x322 => 0,

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

    #[allow(clippy::match_same_arms)]
    pub(crate) fn write_8<A: AccessType, E: cpu::Engine>(emu: &mut Emu<E>, addr: u16, value: u8) {
        match addr & 0xFFF {
            0x330..=0x33F => {
                if emu.gpu.engine_3d.rendering_enabled {
                    let color = &mut emu.gpu.engine_3d.rendering_state.edge_colors
                        [(addr >> 1) as usize & 7];
                    *color = if addr & 1 == 0 {
                        set_rgb5_low(*color, value)
                    } else {
                        set_rgb5_high(*color, value)
                    };
                }
            }

            0x340 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.alpha_test_ref = value;
                }
            }

            0x350 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_color =
                        set_rgb5_low(emu.gpu.engine_3d.rendering_state.clear_color, value);
                }
            }
            0x351 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_color =
                        set_rgb5_high(emu.gpu.engine_3d.rendering_state.clear_color, value);
                }
            }
            0x352 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_color[3] = value & 0x1F;
                }
            }
            0x353 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_poly_id = value & 0x3F;
                }
            }

            0x354 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_depth =
                        (emu.gpu.engine_3d.rendering_state.clear_depth & 0x7F00) | value as u16;
                }
            }
            0x355 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_depth =
                        (emu.gpu.engine_3d.rendering_state.clear_depth & 0x00FF)
                            | (value as u16 & 0x7F) << 8;
                }
            }

            0x356 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_image_offset[0] = value;
                }
            }
            0x357 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_image_offset[1] = value;
                }
            }

            0x358 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.fog_color =
                        set_rgb5_low(emu.gpu.engine_3d.rendering_state.fog_color, value);
                }
            }
            0x359 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.fog_color =
                        set_rgb5_high(emu.gpu.engine_3d.rendering_state.fog_color, value);
                }
            }
            0x35A => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.fog_color[3] = value & 0x1F;
                }
            }
            0x35B => {}

            0x35C => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.fog_offset =
                        (emu.gpu.engine_3d.rendering_state.fog_offset & 0x7F00) | value as u16;
                }
            }
            0x35D => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.fog_offset =
                        (emu.gpu.engine_3d.rendering_state.fog_offset & 0x00FF)
                            | (value as u16 & 0x7F) << 8;
                }
            }
            0x35E | 0x35F => {}

            0x360..=0x37F => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.fog_densities[addr as usize & 0x1F] =
                        fog_density(value);
                }
            }

            0x380..=0x3BF => {
                if emu.gpu.engine_3d.rendering_enabled {
                    let color = &mut emu.gpu.engine_3d.rendering_state.toon_colors
                        [(addr >> 1) as usize & 0x1F];
                    *color = if addr & 1 == 0 {
                        set_rgb5_low(*color, value)
                    } else {
                        set_rgb5_high(*color, value)
                    };
                }
            }

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

    #[allow(clippy::match_same_arms)]
    pub(crate) fn write_16<A: AccessType, E: cpu::Engine>(
        emu: &mut Emu<E>,
        addr: u16,
        mut value: u16,
    ) {
        match addr & 0xFFE {
            0x330..=0x33E => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.edge_colors[(addr >> 1) as usize & 7] =
                        decode_rgb_5(value, 0);
                }
            }

            0x340 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.alpha_test_ref = value as u8;
                }
            }

            0x350 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_color =
                        decode_rgb_5(value, emu.gpu.engine_3d.rendering_state.clear_color[3]);
                    emu.gpu.engine_3d.rendering_state.rear_plane_fog_enabled = value & 1 << 15 != 0;
                }
            }
            0x352 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_color[3] = value as u8 & 0x1F;
                    emu.gpu.engine_3d.rendering_state.clear_poly_id = (value >> 8) as u8 & 0x3F;
                }
            }

            0x354 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_depth = value & 0x7FFF;
                }
            }

            0x356 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_image_offset =
                        [value as u8, (value >> 8) as u8];
                }
            }

            0x358 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.fog_color =
                        decode_rgb_5(value, emu.gpu.engine_3d.rendering_state.fog_color[3]);
                }
            }
            0x35A => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.fog_color[3] = value as u8 & 0x1F;
                }
            }

            0x35C => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.fog_offset = value & 0x7FFF;
                }
            }
            0x35E => {}

            0x360..=0x37E => {
                if emu.gpu.engine_3d.rendering_enabled {
                    let i = addr as usize & 0x1E;
                    for i in [i, i | 1] {
                        emu.gpu.engine_3d.rendering_state.fog_densities[i] =
                            fog_density(value as u8);
                        value >>= 8;
                    }
                }
            }

            0x380..=0x3BE => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.toon_colors[(addr >> 1) as usize & 0x1F] =
                        decode_rgb_5(value, 0);
                }
            }

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

    pub(crate) fn write_32<A: AccessType, E: cpu::Engine>(
        emu: &mut Emu<E>,
        addr: u16,
        mut value: u32,
    ) {
        match addr & 0xFFC {
            0x330..=0x33C => {
                if emu.gpu.engine_3d.rendering_enabled {
                    let i = (addr >> 1) as usize & 6;
                    emu.gpu.engine_3d.rendering_state.edge_colors[i] =
                        decode_rgb_5(value as u16, 0);
                    emu.gpu.engine_3d.rendering_state.edge_colors[i | 1] =
                        decode_rgb_5((value >> 16) as u16, 0);
                }
            }

            0x340 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.alpha_test_ref = value as u8;
                }
            }

            0x350 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_color =
                        decode_rgb_5(value as u16, (value >> 16) as u8 & 0x1F);
                    emu.gpu.engine_3d.rendering_state.rear_plane_fog_enabled = value & 1 << 15 != 0;
                    emu.gpu.engine_3d.rendering_state.clear_poly_id = (value >> 24) as u8 & 0x3F;
                }
            }

            0x354 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.clear_depth = value as u16 & 0x7FFF;
                    emu.gpu.engine_3d.rendering_state.clear_image_offset =
                        [(value >> 16) as u8, (value >> 24) as u8];
                }
            }

            0x358 => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.fog_color =
                        decode_rgb_5(value as u16, (value >> 16) as u8 & 0x1F);
                }
            }

            0x35C => {
                if emu.gpu.engine_3d.rendering_enabled {
                    emu.gpu.engine_3d.rendering_state.fog_offset = value as u16 & 0x7FFF;
                }
            }

            0x360..=0x37C => {
                if emu.gpu.engine_3d.rendering_enabled {
                    let i = addr as usize & 0x1C;
                    for i in i..=i | 3 {
                        emu.gpu.engine_3d.rendering_state.fog_densities[i] =
                            fog_density(value as u8);
                        value >>= 8;
                    }
                }
            }

            0x380..=0x3BC => {
                if emu.gpu.engine_3d.rendering_enabled {
                    let i = (addr >> 1) as usize & 0x1E;
                    emu.gpu.engine_3d.rendering_state.toon_colors[i] =
                        decode_rgb_5(value as u16, 0);
                    emu.gpu.engine_3d.rendering_state.toon_colors[i | 1] =
                        decode_rgb_5((value >> 16) as u16, 0);
                }
            }

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
