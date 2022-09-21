use super::BgObjPixel;
use dust_core::gpu::{
    engine_2d::{CaptureControl, Control},
    vram::Vram,
    Scanline,
};

pub fn run(
    line: u8,
    control: Control,
    capture_control: CaptureControl,
    bg_obj_scanline: &Scanline<BgObjPixel>,
    scanline_3d: Option<&Scanline<u32>>,
    vram: &mut Vram,
) {
    let dst_bank_index = capture_control.dst_bank();
    let dst_bank_control = vram.bank_control()[dst_bank_index as usize];
    if dst_bank_control.enabled() && dst_bank_control.mst() == 0 {
        let capture_width_shift = 7 + (capture_control.size() != 0) as u8;

        let dst_bank = match dst_bank_index {
            0 => vram.banks.a.as_ptr(),
            1 => vram.banks.b.as_ptr(),
            2 => vram.banks.c.as_ptr(),
            _ => vram.banks.d.as_ptr(),
        };

        let dst_offset = (((capture_control.dst_offset_raw() as usize) << 15)
            + ((line as usize) << (1 + capture_width_shift)))
            & 0x1_FFFE;

        let dst_line = unsafe { dst_bank.add(dst_offset) as *mut u16 };

        let capture_source = capture_control.src();
        let factor_a = capture_control.factor_a().min(16) as u16;
        let factor_b = capture_control.factor_b().min(16) as u16;

        let src_b_line = if capture_source != 0 && (factor_b != 0 || capture_source & 2 == 0) {
            if capture_control.src_b_display_fifo() {
                todo!("Display capture display FIFO source");
            } else {
                let src_bank_index = control.a_vram_bank();
                let src_bank_control = vram.bank_control()[src_bank_index as usize];
                if src_bank_control.enabled() && src_bank_control.mst() == 0 {
                    let src_bank = match src_bank_index {
                        0 => vram.banks.a.as_ptr(),
                        1 => vram.banks.b.as_ptr(),
                        2 => vram.banks.c.as_ptr(),
                        _ => vram.banks.d.as_ptr(),
                    };

                    let src_offset = if control.display_mode_a() == 2 {
                        (line as usize) << 9
                    } else {
                        (((capture_control.src_b_vram_offset_raw() as usize) << 15)
                            + ((line as usize) << 9))
                            & 0x1_FFFE
                    };

                    Some(unsafe { src_bank.add(src_offset) as *const u16 })
                } else {
                    None
                }
            }
        } else {
            None
        };

        unsafe {
            if capture_source == 1
                || (capture_source & 2 != 0 && factor_a == 0)
                || (capture_control.src_a_3d_only() && scanline_3d.is_none())
            {
                if let Some(src_b_line) = src_b_line {
                    if src_b_line != dst_line {
                        dst_line.copy_from_nonoverlapping(src_b_line, 1 << capture_width_shift);
                    }
                } else {
                    dst_line.write_bytes(0, 1 << capture_width_shift);
                }
            } else if capture_control.src_a_3d_only() {
                let scanline_3d = scanline_3d.unwrap_unchecked();
                if let Some(src_b_line) = src_b_line {
                    for x in 0..1 << capture_width_shift {
                        let a_pixel = scanline_3d.0[x];
                        let a_r = (a_pixel >> 1) as u16 & 0x1F;
                        let a_g = (a_pixel >> 7) as u16 & 0x1F;
                        let a_b = (a_pixel >> 13) as u16 & 0x1F;
                        let a_a = (a_pixel >> 18 & 0x1F != 0) as u16;

                        let b_pixel = src_b_line.add(x).read();
                        let b_r = b_pixel & 0x1F;
                        let b_g = (b_pixel >> 5) & 0x1F;
                        let b_b = (b_pixel >> 10) & 0x1F;
                        let b_a = b_pixel >> 15;

                        let r = (((a_r * a_a * factor_a) + (b_r * b_a * factor_b)) >> 4).min(0x1F);
                        let g = (((a_g * a_a * factor_a) + (b_g * b_a * factor_b)) >> 4).min(0x1F);
                        let b = (((a_b * a_a * factor_a) + (b_b * b_a * factor_b)) >> 4).min(0x1F);
                        let a = a_a | b_a;

                        dst_line.add(x).write(r | g << 5 | b << 10 | a << 15);
                    }
                } else {
                    for x in 0..1 << capture_width_shift {
                        let pixel = scanline_3d.0[x];
                        let r = (pixel >> 1) as u16 & 0x1F;
                        let g = (pixel >> 7) as u16 & 0x1F;
                        let b = (pixel >> 13) as u16 & 0x1F;
                        let a = (pixel >> 18 & 0x1F != 0) as u16;
                        dst_line.add(x).write(r | g << 5 | b << 10 | a << 15);
                    }
                }
            } else if let Some(src_b_line) = src_b_line {
                for x in 0..1 << capture_width_shift {
                    let a_pixel = bg_obj_scanline.0[x].0;
                    let a_r = (a_pixel >> 1) as u16 & 0x1F;
                    let a_g = (a_pixel >> 7) as u16 & 0x1F;
                    let a_b = (a_pixel >> 13) as u16 & 0x1F;

                    let b_pixel = src_b_line.add(x).read();
                    let b_r = b_pixel & 0x1F;
                    let b_g = (b_pixel >> 5) & 0x1F;
                    let b_b = (b_pixel >> 10) & 0x1F;
                    let b_a = b_pixel >> 15;

                    let r = (((a_r * factor_a) + (b_r * b_a * factor_b)) >> 4).min(0x1F);
                    let g = (((a_g * factor_a) + (b_g * b_a * factor_b)) >> 4).min(0x1F);
                    let b = (((a_b * factor_a) + (b_b * b_a * factor_b)) >> 4).min(0x1F);

                    dst_line.add(x).write(r | g << 5 | b << 10 | 0x8000);
                }
            } else {
                for x in 0..1 << capture_width_shift {
                    let pixel = bg_obj_scanline.0[x].0;
                    let r = (pixel >> 1) as u16 & 0x1F;
                    let g = (pixel >> 7) as u16 & 0x1F;
                    let b = (pixel >> 13) as u16 & 0x1F;
                    dst_line.add(x).write(r | g << 5 | b << 10 | 0x8000);
                }
            }
        }
    }
}
