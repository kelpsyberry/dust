use crate::common::{Buffers, ObjPixel, RenderingData, Vram};
use dust_core::gpu::engine_2d::{OamAttr0, OamAttr1, OamAttr2, Role};

pub fn prerender_objs<R: Role, B: Buffers, D: RenderingData, V: Vram<R>>(
    buffers: &mut B,
    line: u8,
    data: &D,
    vram: &V,
) where
    [(); R::OBJ_VRAM_LEN]: Sized,
{
    // Arisotura confirmed that shape 3 just forces 8 pixels of size
    #[rustfmt::skip]
    static OBJ_SIZE_SHIFT: [(u8, u8); 16] = [
        (0, 0), (1, 0), (0, 1), (0, 0),
        (1, 1), (2, 0), (0, 2), (0, 0),
        (2, 2), (2, 1), (1, 2), (0, 0),
        (3, 3), (3, 2), (2, 3), (0, 0),
    ];

    #[inline]
    fn obj_size_shift(attr_0: OamAttr0, attr_1: OamAttr1) -> (u8, u8) {
        OBJ_SIZE_SHIFT[((attr_1.0 >> 12 & 0xC) | attr_0.0 >> 14) as usize]
    }

    unsafe {
        buffers.obj_scanline().0.fill(ObjPixel(0).with_priority(4));
        buffers.obj_window().fill(0);
    }

    if !data.control().objs_enabled() {
        return;
    }

    let oam = vram.oam();

    for priority in (0..4).rev() {
        for obj_i in (0..128).rev() {
            let oam_start = obj_i << 3;
            let attrs = unsafe {
                let attr_2 = OamAttr2(oam.read_le_aligned_unchecked::<u16>(oam_start | 4));
                if attr_2.bg_priority() != priority {
                    continue;
                }
                (
                    OamAttr0(oam.read_le_aligned_unchecked::<u16>(oam_start)),
                    OamAttr1(oam.read_le_aligned_unchecked::<u16>(oam_start | 2)),
                    attr_2,
                )
            };
            if attrs.0.rot_scale() {
                let (width_shift, height_shift) = obj_size_shift(attrs.0, attrs.1);
                let y_in_obj = line.wrapping_sub(attrs.0.y_start()) as u32;
                let (bounds_width_shift, bounds_height_shift) = if attrs.0.double_size() {
                    (width_shift + 1, height_shift + 1)
                } else {
                    (width_shift, height_shift)
                };
                if y_in_obj as u32 >= 8 << bounds_height_shift {
                    continue;
                }
                let x_start = attrs.1.x_start() as i32;
                if x_start <= -(8 << bounds_width_shift) {
                    continue;
                }
                prerender_obj_rot_scale(
                    buffers,
                    attrs,
                    x_start,
                    y_in_obj as i32 - (4 << bounds_height_shift),
                    width_shift,
                    height_shift,
                    bounds_width_shift,
                    data,
                    vram,
                );
            } else {
                if attrs.0.disabled() {
                    continue;
                }
                let (width_shift, height_shift) = obj_size_shift(attrs.0, attrs.1);
                let y_in_obj = line.wrapping_sub(attrs.0.y_start()) as u32;
                if y_in_obj >= 8 << height_shift {
                    continue;
                }
                let x_start = attrs.1.x_start() as i32;
                if x_start <= -(8 << width_shift) {
                    continue;
                }
                let y_in_obj = if attrs.1.y_flip() {
                    y_in_obj ^ ((8 << height_shift) - 1)
                } else {
                    y_in_obj
                };
                (if attrs.1.x_flip() {
                    prerender_obj_normal::<_, _, _, _, true>
                } else {
                    prerender_obj_normal::<_, _, _, _, false>
                })(
                    buffers,
                    (attrs.0, (), attrs.2),
                    x_start,
                    y_in_obj,
                    width_shift,
                    data,
                    vram,
                );
            }
        }
    }
}

#[allow(clippy::similar_names, clippy::too_many_arguments)]
fn prerender_obj_rot_scale<R: Role, B: Buffers, D: RenderingData, V: Vram<R>>(
    buffers: &mut B,
    attrs: (OamAttr0, OamAttr1, OamAttr2),
    bounds_x_start: i32,
    rel_y_in_square_obj: i32,
    width_shift: u8,
    height_shift: u8,
    bounds_width_shift: u8,
    data: &D,
    vram: &V,
) where
    [(); R::OBJ_VRAM_LEN]: Sized,
{
    let (start_x, end_x, start_rel_x_in_square_obj) = {
        let bounds_width = 8 << bounds_width_shift;
        if bounds_x_start < 0 {
            (
                0,
                (bounds_x_start + bounds_width) as usize,
                -(bounds_width >> 1) - bounds_x_start,
            )
        } else {
            (
                bounds_x_start as usize,
                (bounds_x_start + bounds_width).min(256) as usize,
                -(bounds_width >> 1),
            )
        }
    };

    let params = unsafe {
        let start = (attrs.1.rot_scale_params_index() as usize) << 5;
        let oam = vram.oam();
        [
            oam.read_le_aligned_unchecked::<i16>(start | 0x06),
            oam.read_le_aligned_unchecked::<i16>(start | 0x0E),
            oam.read_le_aligned_unchecked::<i16>(start | 0x16),
            oam.read_le_aligned_unchecked::<i16>(start | 0x1E),
        ]
    };

    let mut pos = [
        (0x400 << width_shift)
            + start_rel_x_in_square_obj * params[0] as i32
            + rel_y_in_square_obj * params[1] as i32,
        (0x400 << height_shift)
            + start_rel_x_in_square_obj * params[2] as i32
            + rel_y_in_square_obj * params[3] as i32,
    ];

    let obj_x_outside_mask = !((0x800 << width_shift) - 1);
    let obj_y_outside_mask = !((0x800 << height_shift) - 1);

    if attrs.0.mode() == 3 {
        let alpha = match attrs.2.palette_number() {
            0 => return,
            value => value + 1,
        };

        let tile_number = attrs.2.tile_number() as u32;

        let (tile_base, y_shift) = if data.control().obj_bitmap_1d_mapping() {
            if data.control().bitmap_objs_256x256() {
                return;
            }
            (
                tile_number
                    << if R::IS_A {
                        7 + data.control().a_obj_bitmap_1d_boundary()
                    } else {
                        7
                    },
                width_shift + 1,
            )
        } else if data.control().bitmap_objs_256x256() {
            (
                ((tile_number & 0x1F) << 4) + ((tile_number & !0x1F) << 7),
                9,
            )
        } else {
            (((tile_number & 0xF) << 4) + ((tile_number & !0xF) << 7), 8)
        };

        let pixel_attrs = ObjPixel(0)
            .with_use_raw_color(true)
            .with_alpha(alpha)
            .with_force_blending(true)
            .with_custom_alpha(true)
            .with_priority(attrs.2.bg_priority());

        let obj_vram = vram.obj();
        let scanline = unsafe { buffers.obj_scanline() };

        for x in start_x..end_x {
            if (pos[0] & obj_x_outside_mask) | (pos[1] & obj_y_outside_mask) == 0 {
                let pixel_addr = tile_base + (pos[0] as u32 >> 8) + (pos[1] as u32 >> 8 << y_shift);
                let color = unsafe {
                    obj_vram.read_le_aligned_unchecked::<u16>(
                        (pixel_addr & (R::OBJ_VRAM_MASK & !1)) as usize,
                    )
                };
                if color & 0x8000 != 0 {
                    unsafe {
                        *scanline.0.get_unchecked_mut(x) = pixel_attrs.with_raw_color(color);
                    }
                }
            }

            pos[0] = pos[0].wrapping_add(params[0] as i32);
            pos[1] = pos[1].wrapping_add(params[2] as i32);
        }
    } else {
        let tile_base = if R::IS_A {
            data.control().a_tile_base()
        } else {
            0
        } + {
            let tile_number = attrs.2.tile_number() as u32;
            if data.control().obj_tile_1d_mapping() {
                tile_number << (5 + data.control().obj_tile_1d_boundary())
            } else {
                tile_number << 5
            }
        };

        let mut pixel_attrs = ObjPixel(0)
            .with_priority(attrs.2.bg_priority())
            .with_force_blending(attrs.0.mode() == 1)
            .with_use_raw_color(false);

        let obj_vram = vram.obj();
        let scanline = unsafe { buffers.obj_scanline() };
        let obj_window = unsafe { buffers.obj_window() };

        if attrs.0.use_256_colors() {
            let pal_base = if data.control().obj_ext_pal_enabled() {
                pixel_attrs.set_use_ext_pal(true);
                (attrs.2.palette_number() as u16) << 8
            } else {
                0
            };

            macro_rules! render {
                ($window: expr, $y_off: expr) => {
                    for x in start_x..end_x {
                        if (pos[0] & obj_x_outside_mask) | (pos[1] & obj_y_outside_mask) == 0 {
                            let pixel_addr = {
                                let x_off = (pos[0] as u32 >> 11 << 6) | (pos[0] as u32 >> 8 & 7);
                                tile_base + ($y_off | x_off)
                            };
                            let color_index = unsafe {
                                obj_vram.read_unchecked((pixel_addr & R::OBJ_VRAM_MASK) as usize)
                            };
                            if color_index != 0 {
                                if $window {
                                    obj_window[x >> 3] |= 1 << (x & 7);
                                } else {
                                    unsafe {
                                        *scanline.0.get_unchecked_mut(x) = pixel_attrs
                                            .with_pal_color(pal_base | color_index as u16);
                                    }
                                }
                            }
                        }

                        pos[0] = pos[0].wrapping_add(params[0] as i32);
                        pos[1] = pos[1].wrapping_add(params[2] as i32);
                    }
                };
                ($window: expr) => {
                    if data.control().obj_tile_1d_mapping() {
                        render!(
                            $window,
                            (pos[1] as u32 >> 11 << (width_shift + 3) | (pos[1] as u32 >> 8 & 7))
                                << 3
                        );
                    } else {
                        render!(
                            $window,
                            (pos[1] as u32 >> 11 << 10) | (pos[1] as u32 >> 8 & 7) << 3
                        );
                    }
                };
            }

            if attrs.0.mode() == 2 {
                render!(true);
            } else {
                render!(false);
            }
        } else {
            let pal_base = (attrs.2.palette_number() as u16) << 4;

            macro_rules! render {
                ($window: expr, $y_off: expr) => {
                    for x in start_x..end_x {
                        if (pos[0] & obj_x_outside_mask) | (pos[1] & obj_y_outside_mask) == 0 {
                            let pixel_addr = {
                                let x_off = (pos[0] as u32 >> 11 << 5) | (pos[0] as u32 >> 9 & 3);
                                tile_base + ($y_off | x_off)
                            };
                            let color_index = unsafe {
                                obj_vram.read_unchecked((pixel_addr & R::OBJ_VRAM_MASK) as usize)
                            } >> (pos[0] as u32 >> 6 & 4)
                                & 0xF;
                            if color_index != 0 {
                                if $window {
                                    obj_window[x >> 3] |= 1 << (x & 7);
                                } else {
                                    unsafe {
                                        *scanline.0.get_unchecked_mut(x) = pixel_attrs
                                            .with_pal_color(pal_base | color_index as u16);
                                    }
                                }
                            }
                        }

                        pos[0] = pos[0].wrapping_add(params[0] as i32);
                        pos[1] = pos[1].wrapping_add(params[2] as i32);
                    }
                };
                ($window: expr) => {
                    if data.control().obj_tile_1d_mapping() {
                        render!(
                            $window,
                            (pos[1] as u32 >> 11 << (width_shift + 3) | (pos[1] as u32 >> 8 & 7))
                                << 2
                        );
                    } else {
                        render!(
                            $window,
                            (pos[1] as u32 >> 11 << 10) | (pos[1] as u32 >> 8 & 7) << 2
                        );
                    }
                };
            }

            if attrs.0.mode() == 2 {
                render!(true);
            } else {
                render!(false);
            }
        }
    }
}

fn prerender_obj_normal<R: Role, B: Buffers, D: RenderingData, V: Vram<R>, const X_FLIP: bool>(
    buffers: &mut B,
    attrs: (OamAttr0, (), OamAttr2),
    x_start: i32,
    y_in_obj: u32,
    width_shift: u8,
    data: &D,
    vram: &V,
) where
    [(); R::OBJ_VRAM_LEN]: Sized,
{
    let (start_x, end_x, mut x_in_obj, x_in_obj_incr) = {
        let width = 8 << width_shift;
        let (start_x, end_x, mut x_in_obj) = if x_start < 0 {
            (0, (width + x_start) as usize, -x_start as u32)
        } else {
            (x_start as usize, (x_start + width).min(256) as usize, 0)
        };
        let x_in_obj_incr = if X_FLIP {
            x_in_obj = width as u32 - 1 - x_in_obj;
            -1_i32
        } else {
            1
        };
        (start_x, end_x, x_in_obj, x_in_obj_incr)
    };

    if attrs.0.mode() == 3 {
        let alpha = match attrs.2.palette_number() {
            0 => return,
            value => value + 1,
        };

        let tile_number = attrs.2.tile_number() as u32;

        let mut tile_base = if data.control().obj_bitmap_1d_mapping() {
            if data.control().bitmap_objs_256x256() {
                return;
            }
            (tile_number
                << if R::IS_A {
                    7 + data.control().a_obj_bitmap_1d_boundary()
                } else {
                    7
                })
                + (y_in_obj << (width_shift + 1))
        } else if data.control().bitmap_objs_256x256() {
            ((tile_number & 0x1F) << 4) + ((tile_number & !0x1F) << 7) + (y_in_obj << 9)
        } else {
            ((tile_number & 0xF) << 4) + ((tile_number & !0xF) << 7) + (y_in_obj << 8)
        };

        let pixel_attrs = ObjPixel(0)
            .with_use_raw_color(true)
            .with_alpha(alpha)
            .with_force_blending(true)
            .with_custom_alpha(true)
            .with_priority(attrs.2.bg_priority());

        let obj_vram = vram.obj();
        let scanline = unsafe { buffers.obj_scanline() };

        let x_in_obj_new_tile_compare = if X_FLIP { 3 } else { 0 };

        let tile_base_incr = if X_FLIP { -8_i32 } else { 8 };
        tile_base += (x_in_obj >> 3) << 4;
        let mut pixels = 0;

        macro_rules! read_pixels {
            () => {
                pixels = unsafe {
                    obj_vram.read_le_aligned_unchecked::<u64>(
                        (tile_base & (R::OBJ_VRAM_MASK & !7)) as usize,
                    )
                };
                tile_base = tile_base.wrapping_add(tile_base_incr as u32);
            };
        }

        if x_in_obj & 3 != x_in_obj_new_tile_compare {
            read_pixels!();
        }

        for x in start_x..end_x {
            if x_in_obj & 3 == x_in_obj_new_tile_compare {
                read_pixels!();
            }
            let color = pixels.wrapping_shr(x_in_obj << 4) as u16;
            if color & 0x8000 != 0 {
                unsafe {
                    *scanline.0.get_unchecked_mut(x) = pixel_attrs.with_raw_color(color);
                }
            }
            x_in_obj = x_in_obj.wrapping_add(x_in_obj_incr as u32);
        }
    } else {
        let mut tile_base = if R::IS_A {
            data.control().a_tile_base()
        } else {
            0
        } + {
            let tile_number = attrs.2.tile_number() as u32;
            if data.control().obj_tile_1d_mapping() {
                let tile_number_off = tile_number << (5 + data.control().obj_tile_1d_boundary());
                let y_off = ((y_in_obj & !7) << width_shift | (y_in_obj & 7))
                    << (2 | attrs.0.use_256_colors() as u8);
                tile_number_off + y_off
            } else {
                let tile_number_off = tile_number << 5;
                let y_off = (y_in_obj >> 3 << 10)
                    | ((y_in_obj & 7) << (2 | attrs.0.use_256_colors() as u8));
                tile_number_off + y_off
            }
        };

        let mut pixel_attrs = ObjPixel(0)
            .with_priority(attrs.2.bg_priority())
            .with_force_blending(attrs.0.mode() == 1)
            .with_use_raw_color(false);

        let obj_vram = vram.obj();
        let scanline = unsafe { buffers.obj_scanline() };
        let obj_window = unsafe { buffers.obj_window() };

        let x_in_obj_new_tile_compare = if X_FLIP { 7 } else { 0 };

        if attrs.0.use_256_colors() {
            let pal_base = if data.control().obj_ext_pal_enabled() {
                pixel_attrs.set_use_ext_pal(true);
                (attrs.2.palette_number() as u16) << 8
            } else {
                0
            };

            let tile_base_incr = if X_FLIP { -64_i32 } else { 64 };
            tile_base += x_in_obj >> 3 << 6;
            let mut pixels = 0;

            macro_rules! read_pixels {
                () => {
                    pixels = unsafe {
                        obj_vram.read_le_aligned_unchecked::<u64>(
                            (tile_base & (R::OBJ_VRAM_MASK & !7)) as usize,
                        )
                    };
                    tile_base = tile_base.wrapping_add(tile_base_incr as u32);
                };
            }

            if x_in_obj & 7 != x_in_obj_new_tile_compare {
                read_pixels!();
            }

            macro_rules! render {
                ($window: expr) => {
                    for x in start_x..end_x {
                        if x_in_obj & 7 == x_in_obj_new_tile_compare {
                            read_pixels!();
                        }
                        let color_index = pixels.wrapping_shr(x_in_obj << 3) as u16 & 0xFF;
                        if color_index != 0 {
                            if $window {
                                obj_window[x >> 3] |= 1 << (x & 7);
                            } else {
                                unsafe {
                                    *scanline.0.get_unchecked_mut(x) =
                                        pixel_attrs.with_pal_color(pal_base | color_index);
                                }
                            }
                        }
                        x_in_obj = x_in_obj.wrapping_add(x_in_obj_incr as u32);
                    }
                };
            }

            if attrs.0.mode() == 2 {
                render!(true);
            } else {
                render!(false);
            }
        } else {
            let pal_base = (attrs.2.palette_number() as u16) << 4;
            let tile_base_incr = if X_FLIP { -32_i32 } else { 32 };
            tile_base += x_in_obj >> 3 << 5;
            let mut pixels = 0;

            macro_rules! read_pixels {
                () => {
                    pixels = unsafe {
                        obj_vram.read_le_aligned_unchecked::<u32>(
                            (tile_base & (R::OBJ_VRAM_MASK & !3)) as usize,
                        )
                    };
                    tile_base = tile_base.wrapping_add(tile_base_incr as u32);
                };
            }

            if x_in_obj & 7 != x_in_obj_new_tile_compare {
                read_pixels!();
            }

            macro_rules! render {
                ($window: expr) => {
                    for x in start_x..end_x {
                        if x_in_obj & 7 == x_in_obj_new_tile_compare {
                            read_pixels!();
                        }
                        let color_index = pixels.wrapping_shr(x_in_obj << 2) as u16 & 0xF;
                        if color_index != 0 {
                            if $window {
                                obj_window[x >> 3] |= 1 << (x & 7);
                            } else {
                                unsafe {
                                    *scanline.0.get_unchecked_mut(x) =
                                        pixel_attrs.with_pal_color(pal_base | color_index);
                                }
                            }
                        }
                        x_in_obj = x_in_obj.wrapping_add(x_in_obj_incr as u32);
                    }
                };
            }

            if attrs.0.mode() == 2 {
                render!(true);
            } else {
                render!(false);
            }
        }
    }
}
