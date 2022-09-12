use super::{
    BgControl, BlendCoeffsRaw, BrightnessControl, CaptureControl, ColorEffectsControl, Control,
    Engine2d, Role, WindowControl, WindowControlIndex,
};
use crate::cpu::bus::AccessType;

impl<R: Role> Engine2d<R> {
    pub(crate) fn read_8<A: AccessType>(&mut self, addr: u32) -> u8 {
        let addr = addr & 0x7F;
        match addr {
            0x00 => self.data.control.0 as u8,
            0x01 => (self.data.control.0 >> 8) as u8,
            0x02 => (self.data.control.0 >> 16) as u8,
            0x03 => (self.data.control.0 >> 24) as u8,
            0x08 => self.data.bgs[0].control.0 as u8,
            0x09 => (self.data.bgs[0].control.0 >> 8) as u8,
            0x0A => self.data.bgs[1].control.0 as u8,
            0x0B => (self.data.bgs[1].control.0 >> 8) as u8,
            0x0C => self.data.bgs[2].control.0 as u8,
            0x0D => (self.data.bgs[2].control.0 >> 8) as u8,
            0x0E => self.data.bgs[3].control.0 as u8,
            0x0F => (self.data.bgs[3].control.0 >> 8) as u8,
            0x64 => self.data.capture_control.0 as u8,
            0x65 => (self.data.capture_control.0 >> 8) as u8,
            0x66 => (self.data.capture_control.0 >> 16) as u8,
            0x67 => (self.data.capture_control.0 >> 24) as u8,
            _ => {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(self.logger, "Unknown read8 @ {:#04X}", addr);
                }
                0
            }
        }
    }

    pub(crate) fn read_16<A: AccessType>(&mut self, addr: u32) -> u16 {
        let addr = addr & 0x7E;
        match addr {
            0x00 => self.data.control.0 as u16,
            0x02 => (self.data.control.0 >> 16) as u16,
            0x08 => self.data.bgs[0].control.0,
            0x0A => self.data.bgs[1].control.0,
            0x0C => self.data.bgs[2].control.0,
            0x0E => self.data.bgs[3].control.0,
            0x48 => {
                self.data.window_control[0].0 as u16 | (self.data.window_control[1].0 as u16) << 8
            }
            0x4A => {
                self.data.window_control[2].0 as u16 | (self.data.window_control[3].0 as u16) << 8
            }
            0x50 => self.data.color_effects_control.0,
            0x52 => self.data.blend_coeffs_raw.0,
            0x64 => self.data.capture_control.0 as u16,
            0x66 => (self.data.capture_control.0 >> 16) as u16,
            0x6C => self.data.master_brightness_control.0,
            _ => {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(self.logger, "Unknown read16 @ {:#04X}", addr);
                }
                0
            }
        }
    }

    pub(crate) fn read_32<A: AccessType>(&mut self, addr: u32) -> u32 {
        let addr = addr & 0x7C;
        match addr {
            0x00 => self.data.control.0,
            0x08 => self.data.bgs[0].control.0 as u32 | (self.data.bgs[1].control.0 as u32) << 16,
            0x0C => self.data.bgs[2].control.0 as u32 | (self.data.bgs[3].control.0 as u32) << 16,
            0x48 => {
                self.data.window_control[0].0 as u32
                    | (self.data.window_control[1].0 as u32) << 8
                    | (self.data.window_control[2].0 as u32) << 16
                    | (self.data.window_control[3].0 as u32) << 24
            }
            0x50 => {
                self.data.color_effects_control.0 as u32
                    | (self.data.blend_coeffs_raw.0 as u32) << 16
            }
            0x64 => self.data.capture_control.0,
            0x6C => self.data.master_brightness_control.0 as u32,
            _ => {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(self.logger, "Unknown read32 @ {:#04X}", addr);
                }
                0
            }
        }
    }

    pub(crate) fn write_8<A: AccessType>(&mut self, addr: u32, value: u8) {
        let addr = addr & 0x7F;
        match addr {
            0x00 => self.write_control(Control((self.data.control.0 & 0xFFFF_FF00) | value as u32)),
            0x01 => self.write_control(Control(
                (self.data.control.0 & 0xFFFF_00FF) | (value as u32) << 8,
            )),
            0x02 => self.write_control(Control(
                (self.data.control.0 & 0xFF00_FFFF) | (value as u32) << 16,
            )),
            0x03 => self.write_control(Control(
                (self.data.control.0 & 0x00FF_FFFF) | (value as u32) << 24,
            )),
            0x08 => self.data.bgs[0].write_control(BgControl(
                (self.data.bgs[0].control.0 & 0xFF00) | value as u16,
            )),
            0x09 => self.data.bgs[0].write_control(BgControl(
                (self.data.bgs[0].control.0 & 0x00FF) | (value as u16) << 8,
            )),
            0x0A => self.data.bgs[1].write_control(BgControl(
                (self.data.bgs[1].control.0 & 0xFF00) | value as u16,
            )),
            0x0B => self.data.bgs[1].write_control(BgControl(
                (self.data.bgs[1].control.0 & 0x00FF) | (value as u16) << 8,
            )),
            0x0C => self.data.bgs[2].write_control(BgControl(
                (self.data.bgs[2].control.0 & 0xFF00) | value as u16,
            )),
            0x0D => self.data.bgs[2].write_control(BgControl(
                (self.data.bgs[2].control.0 & 0x00FF) | (value as u16) << 8,
            )),
            0x0E => self.data.bgs[3].write_control(BgControl(
                (self.data.bgs[3].control.0 & 0xFF00) | value as u16,
            )),
            0x0F => self.data.bgs[3].write_control(BgControl(
                (self.data.bgs[3].control.0 & 0x00FF) | (value as u16) << 8,
            )),
            0x10 => {
                self.data.bgs[0].scroll[0] = (self.data.bgs[0].scroll[0] & 0xFF00) | value as u16;
            }
            0x11 => {
                self.data.bgs[0].scroll[0] =
                    (self.data.bgs[0].scroll[0] & 0x00FF) | (value as u16 & 1) << 8;
            }
            0x12 => {
                self.data.bgs[0].scroll[1] = (self.data.bgs[0].scroll[1] & 0xFF00) | value as u16;
            }
            0x13 => {
                self.data.bgs[0].scroll[1] =
                    (self.data.bgs[0].scroll[1] & 0x00FF) | (value as u16 & 1) << 8;
            }
            0x14 => {
                self.data.bgs[1].scroll[0] = (self.data.bgs[1].scroll[0] & 0xFF00) | value as u16;
            }
            0x15 => {
                self.data.bgs[1].scroll[0] =
                    (self.data.bgs[1].scroll[0] & 0x00FF) | (value as u16 & 1) << 8;
            }
            0x16 => {
                self.data.bgs[1].scroll[1] = (self.data.bgs[1].scroll[1] & 0xFF00) | value as u16;
            }
            0x17 => {
                self.data.bgs[1].scroll[1] =
                    (self.data.bgs[1].scroll[1] & 0x00FF) | (value as u16 & 1) << 8;
            }
            0x18 => {
                self.data.bgs[2].scroll[0] = (self.data.bgs[2].scroll[0] & 0xFF00) | value as u16;
            }
            0x19 => {
                self.data.bgs[2].scroll[0] =
                    (self.data.bgs[2].scroll[0] & 0x00FF) | (value as u16 & 1) << 8;
            }
            0x1A => {
                self.data.bgs[2].scroll[1] = (self.data.bgs[2].scroll[1] & 0xFF00) | value as u16;
            }
            0x1B => {
                self.data.bgs[2].scroll[1] =
                    (self.data.bgs[2].scroll[1] & 0x00FF) | (value as u16 & 1) << 8;
            }
            0x1C => {
                self.data.bgs[3].scroll[0] = (self.data.bgs[3].scroll[0] & 0xFF00) | value as u16;
            }
            0x1D => {
                self.data.bgs[3].scroll[0] =
                    (self.data.bgs[3].scroll[0] & 0x00FF) | (value as u16 & 1) << 8;
            }
            0x1E => {
                self.data.bgs[3].scroll[1] = (self.data.bgs[3].scroll[1] & 0xFF00) | value as u16;
            }
            0x1F => {
                self.data.bgs[3].scroll[1] =
                    (self.data.bgs[3].scroll[1] & 0x00FF) | (value as u16 & 1) << 8;
            }
            0x40 => self.data.window_ranges[0].x.1 = value,
            0x41 => self.data.window_ranges[0].x.0 = value,
            0x42 => self.data.window_ranges[1].x.1 = value,
            0x43 => self.data.window_ranges[1].x.0 = value,
            0x44 => self.data.window_ranges[0].y.1 = value,
            0x45 => self.data.window_ranges[0].y.0 = value,
            0x46 => self.data.window_ranges[1].y.1 = value,
            0x47 => self.data.window_ranges[1].y.0 = value,
            0x64 => self.write_capture_control(CaptureControl(
                (self.data.capture_control.0 & 0xFFFF_FF00) | value as u32,
            )),
            0x65 => self.write_capture_control(CaptureControl(
                (self.data.capture_control.0 & 0xFFFF_00FF) | (value as u32) << 8,
            )),
            0x66 => self.write_capture_control(CaptureControl(
                (self.data.capture_control.0 & 0xFF00_FFFF) | (value as u32) << 16,
            )),
            0x67 => self.write_capture_control(CaptureControl(
                (self.data.capture_control.0 & 0x00FF_FFFF) | (value as u32) << 24,
            )),
            _ =>
            {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        self.logger,
                        "Unknown write8 @ {:#04X}: {:#04X}",
                        addr,
                        value
                    );
                }
            }
        }
    }

    pub(crate) fn write_16<A: AccessType>(&mut self, addr: u32, value: u16) {
        let addr = addr & 0x7E;
        match addr {
            0x00 => self.write_control(Control((self.data.control.0 & 0xFFFF_0000) | value as u32)),
            0x02 => self.write_control(Control(
                (self.data.control.0 & 0x0000_FFFF) | (value as u32) << 16,
            )),
            0x08 => self.data.bgs[0].write_control(BgControl(value)),
            0x0A => self.data.bgs[1].write_control(BgControl(value)),
            0x0C => self.data.bgs[2].write_control(BgControl(value)),
            0x0E => self.data.bgs[3].write_control(BgControl(value)),
            0x10 => self.data.bgs[0].scroll[0] = value & 0x1FF,
            0x12 => self.data.bgs[0].scroll[1] = value & 0x1FF,
            0x14 => self.data.bgs[1].scroll[0] = value & 0x1FF,
            0x16 => self.data.bgs[1].scroll[1] = value & 0x1FF,
            0x18 => self.data.bgs[2].scroll[0] = value & 0x1FF,
            0x1A => self.data.bgs[2].scroll[1] = value & 0x1FF,
            0x1C => self.data.bgs[3].scroll[0] = value & 0x1FF,
            0x1E => self.data.bgs[3].scroll[1] = value & 0x1FF,
            0x20 => self.data.affine_bg_data[0].params[0] = value as i16,
            0x22 => self.data.affine_bg_data[0].params[1] = value as i16,
            0x24 => self.data.affine_bg_data[0].params[2] = value as i16,
            0x26 => self.data.affine_bg_data[0].params[3] = value as i16,
            0x28 => {
                self.data.affine_bg_data[0].ref_points[0] =
                    (self.data.affine_bg_data[0].ref_points[0] & !0xFFFF) | value as i32;
                self.data.affine_bg_data[0].pos[0] = self.data.affine_bg_data[0].ref_points[0];
            }
            0x2A => {
                self.data.affine_bg_data[0].ref_points[0] =
                    (self.data.affine_bg_data[0].ref_points[0] & 0xFFFF)
                        | (value as i32) << 20 >> 4;
                self.data.affine_bg_data[0].pos[0] = self.data.affine_bg_data[0].ref_points[0];
            }
            0x2C => {
                self.data.affine_bg_data[0].ref_points[1] =
                    (self.data.affine_bg_data[0].ref_points[1] & !0xFFFF) | value as i32;
                self.data.affine_bg_data[0].pos[1] = self.data.affine_bg_data[0].ref_points[1];
            }
            0x2E => {
                self.data.affine_bg_data[0].ref_points[1] =
                    (self.data.affine_bg_data[0].ref_points[1] & 0xFFFF)
                        | (value as i32) << 20 >> 4;
                self.data.affine_bg_data[0].pos[1] = self.data.affine_bg_data[0].ref_points[1];
            }
            0x30 => self.data.affine_bg_data[1].params[0] = value as i16,
            0x32 => self.data.affine_bg_data[1].params[1] = value as i16,
            0x34 => self.data.affine_bg_data[1].params[2] = value as i16,
            0x36 => self.data.affine_bg_data[1].params[3] = value as i16,
            0x38 => {
                self.data.affine_bg_data[1].ref_points[0] =
                    (self.data.affine_bg_data[1].ref_points[0] & !0xFFFF) | value as i32;
                self.data.affine_bg_data[1].pos[0] = self.data.affine_bg_data[1].ref_points[0];
            }
            0x3A => {
                self.data.affine_bg_data[1].ref_points[0] =
                    (self.data.affine_bg_data[1].ref_points[0] & 0xFFFF)
                        | (value as i32) << 20 >> 4;
                self.data.affine_bg_data[1].pos[0] = self.data.affine_bg_data[1].ref_points[0];
            }
            0x3C => {
                self.data.affine_bg_data[1].ref_points[1] =
                    (self.data.affine_bg_data[1].ref_points[1] & !0xFFFF) | value as i32;
                self.data.affine_bg_data[1].pos[1] = self.data.affine_bg_data[1].ref_points[1];
            }
            0x3E => {
                self.data.affine_bg_data[1].ref_points[1] =
                    (self.data.affine_bg_data[1].ref_points[1] & 0xFFFF)
                        | (value as i32) << 20 >> 4;
                self.data.affine_bg_data[1].pos[1] = self.data.affine_bg_data[1].ref_points[1];
            }
            0x40 => self.data.window_ranges[0].x = ((value >> 8) as u8, value as u8),
            0x42 => self.data.window_ranges[1].x = ((value >> 8) as u8, value as u8),
            0x44 => self.data.window_ranges[0].y = ((value >> 8) as u8, value as u8),
            0x46 => self.data.window_ranges[1].y = ((value >> 8) as u8, value as u8),
            0x48 => {
                self.write_window_control(WindowControlIndex::new(0), WindowControl(value as u8));
                self.write_window_control(
                    WindowControlIndex::new(1),
                    WindowControl((value >> 8) as u8),
                );
            }
            0x4A => {
                self.write_window_control(WindowControlIndex::new(2), WindowControl(value as u8));
                self.write_window_control(
                    WindowControlIndex::new(3),
                    WindowControl((value >> 8) as u8),
                );
            }
            0x50 => self.write_color_effects_control(ColorEffectsControl(value)),
            0x52 => self.write_blend_coeffs_raw(BlendCoeffsRaw(value)),
            0x54 => self.write_brightness_coeff(value as u8),
            0x64 => self.write_capture_control(CaptureControl(
                (self.data.capture_control.0 & 0xFFFF_0000) | value as u32,
            )),
            0x66 => self.write_capture_control(CaptureControl(
                (self.data.capture_control.0 & 0x0000_FFFF) | (value as u32) << 16,
            )),
            0x6C => self.write_master_brightness_control(BrightnessControl(value)),
            _ =>
            {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        self.logger,
                        "Unknown write16 @ {:#04X}: {:#06X}",
                        addr,
                        value
                    );
                }
            }
        }
    }

    pub(crate) fn write_32<A: AccessType>(&mut self, addr: u32, value: u32) {
        let addr = addr & 0x7C;
        match addr {
            0x00 => self.write_control(Control(value)),
            0x08 => {
                self.data.bgs[0].write_control(BgControl(value as u16));
                self.data.bgs[1].write_control(BgControl((value >> 16) as u16));
            }
            0x0C => {
                self.data.bgs[2].write_control(BgControl(value as u16));
                self.data.bgs[3].write_control(BgControl((value >> 16) as u16));
            }
            0x10 => {
                self.data.bgs[0].scroll[0] = value as u16 & 0x1FF;
                self.data.bgs[0].scroll[1] = (value >> 16) as u16 & 0x1FF;
            }
            0x14 => {
                self.data.bgs[1].scroll[0] = value as u16 & 0x1FF;
                self.data.bgs[1].scroll[1] = (value >> 16) as u16 & 0x1FF;
            }
            0x18 => {
                self.data.bgs[2].scroll[0] = value as u16 & 0x1FF;
                self.data.bgs[2].scroll[1] = (value >> 16) as u16 & 0x1FF;
            }
            0x1C => {
                self.data.bgs[3].scroll[0] = value as u16 & 0x1FF;
                self.data.bgs[3].scroll[1] = (value >> 16) as u16 & 0x1FF;
            }
            0x20 => {
                self.data.affine_bg_data[0].params[0] = value as i16;
                self.data.affine_bg_data[0].params[1] = (value >> 16) as i16;
            }
            0x24 => {
                self.data.affine_bg_data[0].params[2] = value as i16;
                self.data.affine_bg_data[0].params[3] = (value >> 16) as i16;
            }
            0x28 => {
                self.data.affine_bg_data[0].ref_points[0] = (value as i32) << 4 >> 4;
                self.data.affine_bg_data[0].pos[0] = self.data.affine_bg_data[0].ref_points[0];
            }
            0x2C => {
                self.data.affine_bg_data[0].ref_points[1] = (value as i32) << 4 >> 4;
                self.data.affine_bg_data[0].pos[1] = self.data.affine_bg_data[0].ref_points[1];
            }
            0x30 => {
                self.data.affine_bg_data[1].params[0] = value as i16;
                self.data.affine_bg_data[1].params[1] = (value >> 16) as i16;
            }
            0x34 => {
                self.data.affine_bg_data[1].params[2] = value as i16;
                self.data.affine_bg_data[1].params[3] = (value >> 16) as i16;
            }
            0x38 => {
                self.data.affine_bg_data[1].ref_points[0] = (value as i32) << 4 >> 4;
                self.data.affine_bg_data[1].pos[0] = self.data.affine_bg_data[1].ref_points[0];
            }
            0x3C => {
                self.data.affine_bg_data[1].ref_points[1] = (value as i32) << 4 >> 4;
                self.data.affine_bg_data[1].pos[1] = self.data.affine_bg_data[1].ref_points[1];
            }
            0x40 => {
                self.data.window_ranges[0].x = ((value >> 8) as u8, value as u8);
                self.data.window_ranges[1].x = ((value >> 24) as u8, (value >> 16) as u8);
            }
            0x44 => {
                self.data.window_ranges[0].y = ((value >> 8) as u8, value as u8);
                self.data.window_ranges[1].y = ((value >> 24) as u8, (value >> 16) as u8);
            }
            0x48 => {
                self.write_window_control(WindowControlIndex::new(0), WindowControl(value as u8));
                self.write_window_control(
                    WindowControlIndex::new(1),
                    WindowControl((value >> 8) as u8),
                );
                self.write_window_control(
                    WindowControlIndex::new(2),
                    WindowControl((value >> 16) as u8),
                );
                self.write_window_control(
                    WindowControlIndex::new(3),
                    WindowControl((value >> 24) as u8),
                );
            }
            0x50 => {
                self.write_color_effects_control(ColorEffectsControl(value as u16));
                self.write_blend_coeffs_raw(BlendCoeffsRaw((value >> 16) as u16));
            }
            0x54 => self.write_brightness_coeff(value as u8),
            0x64 => self.write_capture_control(CaptureControl(value)),
            0x6C => self.write_master_brightness_control(BrightnessControl(value as u16)),
            _ =>
            {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        self.logger,
                        "Unknown write32 @ {:#04X}: {:#010X}",
                        addr,
                        value
                    );
                }
            }
        }
    }
}
