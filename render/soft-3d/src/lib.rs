#![feature(portable_simd, new_zeroed_alloc)]
#![warn(clippy::all)]

mod data;
pub use data::RenderingData;
mod utils;

use core::simd::{cmp::SimdOrd, num::SimdUint};
use dust_core::{
    gpu::{
        engine_3d::{
            Color, InterpColor, PolyAddr, PolyVertIndex, RenderingPolygonAttrs, TexCoords,
            TextureParams,
        },
        Scanline,
    },
    utils::mem_prelude::*,
};
use utils::{
    clip_x_range, dec_poly_vert_index, decode_rgb5, expand_depth, inc_poly_vert_index,
    rgb5_to_rgb6, rgb5_to_rgb6_shift, DummyEdge, Edge, Edges, InterpLineData,
};

type DepthTestFn = fn(u32, u32, PixelAttrs) -> bool;
type ProcessPixelFn = fn(&RenderingData, &RenderingPolygon, TexCoords, InterpColor) -> InterpColor;

#[derive(Clone, Copy)]
struct RenderingPolygon {
    poly_addr: PolyAddr,
    attrs: RenderingPolygonAttrs,
    is_shadow: bool,
    tex_params: TextureParams,
    tex_palette_base: u16,
    top_y: u8,
    bot_y: u8,
    height: u8,
    edges: Edges,
    l_vert_i: PolyVertIndex,
    r_vert_i: PolyVertIndex,
    bot_i: PolyVertIndex,
    alpha: u8,
    id: u8,
    depth_test: DepthTestFn,
    process_pixel: ProcessPixelFn,
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct PixelAttrs(pub u32): Debug {
        // Edge flag for the topmost opaque polygon (used for edge marking)
        pub is_opaque_edge: bool @ 0,

        // Used so that a < depth test for a front-facing pixel over an opaque + back-facing one
        // becomes <=
        pub translucent: bool @ 13,
        pub front_facing: bool @ 14,

        // Pixel fog flag (replaced for opaque pixels, ANDed for translucent ones)
        pub fog_enabled: bool @ 15,

        // The topmost translucent polygon ID (used to avoid rendering translucent pixels over the
        // same translucent polygon ID)
        // 0 for no ID or 0x40..=0x7F for ID 0..=0x3F
        pub translucent_poly_id: u8 @ 16..=22,

        // Topmost opaque pixel polygon ID (used for edge marking)
        pub opaque_poly_id: u8 @ 24..=29,

        // Whether a shadow mask was drawn on top of this pixel
        pub stencil: bool @ 31,
    }
}

impl PixelAttrs {
    #[inline]
    fn from_opaque_poly_attrs(poly: &RenderingPolygon, is_edge: bool) -> Self {
        PixelAttrs((poly.attrs.raw() & 0x3F00_8000) | is_edge as u32)
            .with_front_facing(poly.attrs.is_front_facing())
    }

    #[inline]
    fn from_translucent_poly_attrs(poly: &RenderingPolygon, opaque: PixelAttrs) -> Self {
        PixelAttrs(opaque.0 & 0x3F00_800F & (poly.attrs.raw() | !0x0000_8000))
            .with_translucent(true)
            .with_front_facing(poly.attrs.is_front_facing())
            .with_translucent_poly_id(poly.id | 0x40)
    }
}

fn process_pixel<const FORMAT: u8, const MODE: u8>(
    rendering_data: &RenderingData,
    poly: &RenderingPolygon,
    uv: TexCoords,
    vert_color: InterpColor,
) -> InterpColor {
    let vert_color = vert_color >> 3;

    let mut vert_blend_color = match MODE {
        2 => rgb5_to_rgb6(rendering_data.toon_colors[(vert_color[0] as usize >> 1).min(31)].cast()),
        3 => InterpColor::splat(vert_color[0]),
        _ => vert_color,
    };
    vert_blend_color[3] = if poly.alpha == 0 {
        0x1F
    } else {
        poly.alpha as u16
    };

    let blended_color = if FORMAT == 0 {
        vert_blend_color
    } else {
        let tex_params = poly.tex_params;
        let tex_base = (tex_params.vram_off() as usize) << 3;
        let pal_base = if FORMAT == 2 {
            (poly.tex_palette_base as usize) << 3
        } else {
            (poly.tex_palette_base as usize) << 4
        };

        let tex_width_shift = tex_params.size_shift_s();
        let tex_width_mask = (8 << tex_width_shift) - 1;
        let tex_height_shift = tex_params.size_shift_t();
        let tex_height_mask = (8 << tex_height_shift) - 1;

        macro_rules! apply_tiling {
            ($coord: expr, $size_mask: expr, $size_shift: expr, $repeat: ident, $flip: ident) => {{
                let x = $coord >> 4;
                (if tex_params.$repeat() {
                    if tex_params.$flip() && x & 8 << $size_shift != 0 {
                        $size_mask - (x & $size_mask)
                    } else {
                        x & $size_mask
                    }
                } else {
                    x.clamp(0, $size_mask)
                }) as u16
            }};
        }

        let u = apply_tiling!(uv[0], tex_width_mask, tex_width_shift, repeat_s, flip_s) as usize;
        let v = apply_tiling!(uv[1], tex_height_mask, tex_height_shift, repeat_t, flip_t) as usize;

        let i = v << (tex_width_shift + 3) | u;

        let tex_color = match FORMAT {
            1 => {
                let pixel = rendering_data.texture[(tex_base + i) & 0x7_FFFF];
                let color_index = pixel as usize & 0x1F;
                let raw_alpha = pixel >> 5;
                rgb5_to_rgb6(decode_rgb5(
                    rendering_data
                        .tex_pal
                        .read_le::<u16>((pal_base + (color_index << 1)) & 0x1_FFFF),
                    (raw_alpha << 2 | raw_alpha >> 1) as u16,
                ))
            }

            2 => {
                let color_index = rendering_data.texture[(tex_base + (i >> 2)) & 0x7_FFFF]
                    .wrapping_shr((i << 1) as u32) as usize
                    & 3;
                rgb5_to_rgb6(decode_rgb5(
                    rendering_data
                        .tex_pal
                        .read_le::<u16>(pal_base | color_index << 1),
                    if tex_params.use_color_0_as_transparent() && color_index == 0 {
                        0
                    } else {
                        0x1F
                    },
                ))
            }

            3 => {
                let color_index = rendering_data.texture[(tex_base + (i >> 1)) & 0x7_FFFF]
                    .wrapping_shr((i << 2) as u32) as usize
                    & 0xF;
                rgb5_to_rgb6(decode_rgb5(
                    rendering_data
                        .tex_pal
                        .read_le::<u16>((pal_base + (color_index << 1)) & 0x1_FFFF),
                    if tex_params.use_color_0_as_transparent() && color_index == 0 {
                        0
                    } else {
                        0x1F
                    },
                ))
            }

            4 => {
                let color_index = rendering_data.texture[(tex_base + i) & 0x7_FFFF] as usize;
                rgb5_to_rgb6(decode_rgb5(
                    rendering_data
                        .tex_pal
                        .read_le::<u16>((pal_base + (color_index << 1)) & 0x1_FFFF),
                    if tex_params.use_color_0_as_transparent() && color_index == 0 {
                        0
                    } else {
                        0x1F
                    },
                ))
            }

            5 => {
                let texel_block_addr = (tex_base & 0x4_0000)
                    | ((tex_base + (v >> 2 << (tex_width_shift + 3) | (u & !3))) & 0x1_FFFF);
                let texel_value = rendering_data.texture[(texel_block_addr | (v & 3)) & 0x7_FFFF]
                    >> ((u & 3) << 1)
                    & 3;

                let pal_data_addr = 0x2_0000
                    | (texel_block_addr >> 1 & 0xFFFE)
                    | (texel_block_addr >> 2 & 0x1_0000);
                let pal_data = rendering_data.texture.read_le::<u16>(pal_data_addr);
                let pal_base = pal_base + (pal_data << 2) as usize;
                let mode = pal_data >> 14;

                macro_rules! color {
                    ($i: literal) => {
                        decode_rgb5(
                            rendering_data
                                .tex_pal
                                .read_le::<u16>((pal_base + ($i << 1)) & 0x1_FFFE),
                            0x1F,
                        )
                    };
                }

                let color = match texel_value {
                    0 => color!(0),

                    1 => color!(1),

                    2 => match mode {
                        0 | 2 => color!(2),

                        1 => {
                            let color_0 = color!(0);
                            let color_1 = color!(1);
                            (color_0 + color_1) >> 1
                        }

                        _ => {
                            let color_0 = color!(0);
                            let color_1 = color!(1);
                            (color_0 * InterpColor::splat(5) + color_1 * InterpColor::splat(3)) >> 3
                        }
                    },

                    _ => match mode {
                        0 | 1 => InterpColor::splat(0),

                        2 => color!(3),

                        _ => {
                            let color_0 = color!(0);
                            let color_1 = color!(1);
                            (color_0 * InterpColor::splat(3) + color_1 * InterpColor::splat(5)) >> 3
                        }
                    },
                };

                if mode & 1 != 0 {
                    rgb5_to_rgb6_shift(color)
                } else {
                    rgb5_to_rgb6(color)
                }
            }

            6 => {
                let pixel = rendering_data.texture[(tex_base + i) & 0x7_FFFF];
                let color_index = pixel as usize & 7;
                let alpha = pixel >> 3;
                rgb5_to_rgb6(decode_rgb5(
                    rendering_data
                        .tex_pal
                        .read_le::<u16>((pal_base | color_index << 1) & 0x1_FFFF),
                    alpha as u16,
                ))
            }

            _ => {
                let color = rendering_data
                    .texture
                    .read_le::<u16>((tex_base + (i << 1)) & 0x7_FFFE);
                rgb5_to_rgb6(decode_rgb5(
                    color,
                    if color & 1 << 15 != 0 { 0x1F } else { 0 },
                ))
            }
        };

        match MODE {
            1 => match tex_color[3] {
                0 => vert_blend_color,
                0x1F => {
                    let mut color = tex_color;
                    color[3] = vert_blend_color[3];
                    color
                }
                _ => {
                    let mut color = (tex_color * InterpColor::splat(tex_color[3])
                        + vert_blend_color * InterpColor::splat(31 - tex_color[3]))
                        >> 5;
                    color[3] = vert_blend_color[3];
                    color
                }
            },

            _ => {
                ((tex_color + InterpColor::splat(1)) * (vert_blend_color + InterpColor::splat(1))
                    - InterpColor::splat(1))
                    >> InterpColor::from_array([6, 6, 6, 5])
            }
        }
    };

    if MODE == 3 {
        let toon_color =
            rgb5_to_rgb6(rendering_data.toon_colors[(vert_color[0] as usize >> 1).min(31)].cast());
        (blended_color + toon_color).simd_min(InterpColor::from_array([0x3F, 0x3F, 0x3F, 0x1F]))
    } else {
        blended_color
    }
}

static PROCESS_PIXEL_TEXTURES_ENABLED: [ProcessPixelFn; 32] = [
    process_pixel::<0, 0>,
    process_pixel::<1, 0>,
    process_pixel::<2, 0>,
    process_pixel::<3, 0>,
    process_pixel::<4, 0>,
    process_pixel::<5, 0>,
    process_pixel::<6, 0>,
    process_pixel::<7, 0>,
    process_pixel::<0, 1>,
    process_pixel::<1, 1>,
    process_pixel::<2, 1>,
    process_pixel::<3, 1>,
    process_pixel::<4, 1>,
    process_pixel::<5, 1>,
    process_pixel::<6, 1>,
    process_pixel::<7, 1>,
    process_pixel::<0, 2>,
    process_pixel::<1, 2>,
    process_pixel::<2, 2>,
    process_pixel::<3, 2>,
    process_pixel::<4, 2>,
    process_pixel::<5, 2>,
    process_pixel::<6, 2>,
    process_pixel::<7, 2>,
    process_pixel::<0, 3>,
    process_pixel::<1, 3>,
    process_pixel::<2, 3>,
    process_pixel::<3, 3>,
    process_pixel::<4, 3>,
    process_pixel::<5, 3>,
    process_pixel::<6, 3>,
    process_pixel::<7, 3>,
];

static PROCESS_PIXEL_TEXTURES_DISABLED: [ProcessPixelFn; 4] = [
    process_pixel::<0, 0>,
    process_pixel::<0, 1>,
    process_pixel::<0, 2>,
    process_pixel::<0, 3>,
];

pub struct Renderer {
    color_buffer: Box<[Scanline<Color>; 192]>,
    depth_buffer: Box<[Scanline<u32, 258>; 194]>,
    attr_buffer: Box<[Scanline<PixelAttrs, 258>; 194]>,
    polys: Vec<RenderingPolygon>,
}

fn depth_test_equal_w(a: u32, b: u32, _: PixelAttrs) -> bool {
    a.wrapping_sub(b).wrapping_add(0xFF) <= 0x1FE
}

fn depth_test_equal_z(a: u32, b: u32, _: PixelAttrs) -> bool {
    a.wrapping_sub(b).wrapping_add(0x200) <= 0x400
}

fn depth_test_less_front_facing(a: u32, b: u32, b_attrs: PixelAttrs) -> bool {
    let mask = PixelAttrs(0)
        .with_translucent(true)
        .with_front_facing(true)
        .0;
    let value = PixelAttrs(0)
        .with_translucent(false)
        .with_front_facing(false)
        .0;
    if b_attrs.0 & mask == value {
        a <= b
    } else {
        a < b
    }
}

fn depth_test_less_back_facing(a: u32, b: u32, _: PixelAttrs) -> bool {
    a < b
}

impl Renderer {
    pub fn new() -> Self {
        Renderer {
            color_buffer: unsafe { Box::new_zeroed().assume_init() },
            depth_buffer: unsafe { Box::new_zeroed().assume_init() },
            attr_buffer: unsafe { Box::new_zeroed().assume_init() },
            polys: Vec::with_capacity(2048),
        }
    }

    pub fn start_frame(&mut self, rendering_data: &RenderingData) {
        self.polys.clear();

        for poly_addr in 0..rendering_data.poly_ram_level {
            let poly_addr = unsafe { PolyAddr::new_unchecked(poly_addr) };
            let poly = &rendering_data.poly_ram[poly_addr.get() as usize];
            let verts_len = unsafe { poly.attrs.verts_len() };

            if verts_len.get() < 3 {
                continue;
            }

            let depth_test: fn(u32, u32, PixelAttrs) -> bool = if poly.attrs.depth_test_equal() {
                if rendering_data.w_buffering {
                    depth_test_equal_w
                } else {
                    depth_test_equal_z
                }
            } else if poly.attrs.is_front_facing() {
                depth_test_less_front_facing
            } else {
                depth_test_less_back_facing
            };

            let is_shadow = poly.attrs.mode() == 3;
            let process_pixel = {
                let mode = if is_shadow {
                    // TODO: Do process shadow/shadow mask polygons
                    continue;
                    // if poly.attrs.id() == 0 {
                    //     // TODO: Shadow mask polygons
                    // }
                    // 1
                } else {
                    match poly.attrs.mode() {
                        2 => 2 + rendering_data.control.highlight_shading_enabled() as u8,
                        mode => mode,
                    }
                };
                if rendering_data.control.texture_mapping_enabled() {
                    PROCESS_PIXEL_TEXTURES_ENABLED[(mode << 3 | poly.tex_params.format()) as usize]
                } else {
                    PROCESS_PIXEL_TEXTURES_DISABLED[mode as usize]
                }
            };

            let top_y = poly.top_y;
            let bot_y = poly.bot_y;

            if top_y == bot_y {
                let mut top_i = PolyVertIndex::new(0);
                let mut bot_i = top_i;
                let mut top_vert_addr = poly.verts[0];
                let mut top_vert = &rendering_data.vert_ram[top_vert_addr.get() as usize];
                let mut bot_vert_addr = top_vert_addr;
                let mut bot_vert = top_vert;

                macro_rules! vert {
                    ($i: expr) => {{
                        let i = $i;
                        let vert_addr = poly.verts[i.get() as usize];
                        let vert = &rendering_data.vert_ram[vert_addr.get() as usize];
                        if vert.coords[0] < top_vert.coords[0] {
                            top_i = i;
                            top_vert_addr = vert_addr;
                            top_vert = vert;
                        }
                        if vert.coords[0] > bot_vert.coords[0] {
                            bot_i = i;
                            bot_vert_addr = vert_addr;
                            bot_vert = vert;
                        }
                    }};
                }

                vert!(PolyVertIndex::new(1));
                vert!(PolyVertIndex::new(verts_len.get() - 1));

                self.polys.push(RenderingPolygon {
                    poly_addr,
                    attrs: poly.attrs,
                    is_shadow,
                    tex_params: poly.tex_params,
                    tex_palette_base: poly.tex_palette_base,
                    top_y: poly.top_y,
                    bot_y: poly.bot_y,
                    height: 1,
                    alpha: poly.attrs.alpha(),
                    id: poly.attrs.id(),
                    edges: Edges::Dummy([
                        DummyEdge::new(poly, top_i, top_vert_addr, top_vert),
                        DummyEdge::new(poly, bot_i, bot_vert_addr, bot_vert),
                    ]),
                    l_vert_i: top_i,
                    r_vert_i: bot_i,
                    bot_i,
                    depth_test,
                    process_pixel,
                });
            } else {
                let (top_i, top_vert_addr, top_vert, bot_i) = unsafe {
                    let mut top_i = PolyVertIndex::new(0);
                    let mut bot_i = top_i;
                    let mut top_vert = None;
                    for i in 0..verts_len.get() as usize {
                        let i = PolyVertIndex::new(i as u8);
                        let vert_addr = poly.verts[i.get() as usize];
                        let vert = &rendering_data.vert_ram[vert_addr.get() as usize];
                        if vert.coords[1] as u8 == top_y && top_vert.is_none() {
                            top_i = i;
                            top_vert = Some((vert_addr, vert));
                        }
                        if vert.coords[1] as u8 == bot_y {
                            bot_i = i;
                        }
                    }
                    let (top_vert_addr, top_vert) = top_vert.unwrap_unchecked();
                    (top_i, top_vert_addr, top_vert, bot_i)
                };

                macro_rules! vert {
                    ($i: expr) => {{
                        let i = $i;
                        let addr = poly.verts[i.get() as usize];
                        (i, addr, &rendering_data.vert_ram[addr.get() as usize])
                    }};
                }

                let mut other_verts = [
                    vert!(inc_poly_vert_index(top_i, verts_len)),
                    vert!(dec_poly_vert_index(top_i, verts_len)),
                ];

                if !poly.attrs.is_front_facing() {
                    other_verts.swap(0, 1);
                }

                self.polys.push(RenderingPolygon {
                    poly_addr,
                    attrs: poly.attrs,
                    is_shadow,
                    tex_params: poly.tex_params,
                    tex_palette_base: poly.tex_palette_base,
                    top_y: poly.top_y,
                    bot_y: poly.bot_y,
                    height: poly.bot_y - poly.top_y,
                    alpha: poly.attrs.alpha(),
                    id: poly.attrs.id(),
                    edges: Edges::Normal([
                        Edge::new(
                            poly,
                            top_i,
                            top_vert_addr,
                            top_vert,
                            other_verts[0].0,
                            other_verts[0].1,
                            other_verts[0].2,
                        ),
                        Edge::new(
                            poly,
                            top_i,
                            top_vert_addr,
                            top_vert,
                            other_verts[1].0,
                            other_verts[1].1,
                            other_verts[1].2,
                        ),
                    ]),
                    l_vert_i: other_verts[0].0,
                    r_vert_i: other_verts[1].0,
                    bot_i,
                    depth_test,
                    process_pixel,
                });
            }
        }

        // The bitmap rear plane's out-of-screen pixels get the same depth and attributes as a
        // non-bitmap rear plane (but the fog flag isn't copied since it's unneeded)
        let outside_pixel_attrs = PixelAttrs(0).with_opaque_poly_id(rendering_data.clear_poly_id);
        self.depth_buffer[0].0.fill(rendering_data.clear_depth);
        self.depth_buffer[193].0.fill(rendering_data.clear_depth);
        self.attr_buffer[0].0.fill(outside_pixel_attrs);
        self.attr_buffer[193].0.fill(outside_pixel_attrs);
    }

    pub fn render_line(&mut self, y: u8, rendering_data: &RenderingData) {
        let color_line = &mut self.color_buffer[y as usize].0;
        let depth_full_line = &mut self.depth_buffer[y as usize + 1].0;
        let attr_full_line = &mut self.attr_buffer[y as usize + 1].0;

        if rendering_data.control.rear_plane_bitmap_enabled() {
            let line_base = (y.wrapping_add(rendering_data.clear_image_offset[1]) as usize) << 9;
            let mut x_in_image = rendering_data.clear_image_offset[0];

            let color_line_base = 0x4_0000 | line_base;
            for dst in &mut *color_line {
                let raw_color = rendering_data
                    .texture
                    .read_le(color_line_base | (x_in_image as usize) << 1);
                *dst = rgb5_to_rgb6(decode_rgb5(
                    raw_color,
                    if raw_color >> 15 != 0 { 31 } else { 0 },
                ))
                .cast();
                x_in_image = x_in_image.wrapping_add(1);
            }

            let depth_line_base = 0x4_0000 | line_base;
            let pixel_attrs = PixelAttrs(0).with_opaque_poly_id(rendering_data.clear_poly_id);
            for (dst_depth, dst_attrs) in depth_full_line[1..257]
                .iter_mut()
                .zip(&mut attr_full_line[1..257])
            {
                let raw_depth = rendering_data
                    .texture
                    .read_le(depth_line_base | (x_in_image as usize) << 1);
                *dst_depth = expand_depth(raw_depth);
                *dst_attrs = pixel_attrs.with_fog_enabled(raw_depth >> 15 != 0);
                x_in_image = x_in_image.wrapping_add(1);
            }

            // The bitmap rear plane's out-of-screen pixels get the same depth and attributes as a
            // non-bitmap rear plane (but the fog flag isn't copied since it's unneeded)
            depth_full_line[0] = rendering_data.clear_depth;
            depth_full_line[257] = rendering_data.clear_depth;
            attr_full_line[0] = pixel_attrs;
            attr_full_line[257] = pixel_attrs;
        } else {
            color_line.fill(rgb5_to_rgb6(rendering_data.clear_color.cast()).cast());
            depth_full_line.fill(rendering_data.clear_depth);
            attr_full_line.fill(
                PixelAttrs(0)
                    .with_opaque_poly_id(rendering_data.clear_poly_id)
                    .with_fog_enabled(rendering_data.rear_plane_fog_enabled),
            );
        }

        let depth_line = <&mut [_; 256]>::try_from(&mut depth_full_line[1..257]).unwrap();
        let attr_line = <&mut [_; 256]>::try_from(&mut attr_full_line[1..257]).unwrap();

        for poly in self.polys.iter_mut() {
            if y.wrapping_sub(poly.top_y) >= poly.height {
                continue;
            }

            let (
                ranges,
                fill_edges,
                [(l_vert_color, l_uv, l_depth, l_w), (r_vert_color, r_uv, r_depth, r_w)],
            ) = match &mut poly.edges {
                Edges::Normal(edges) => {
                    let raw_poly = rendering_data.poly_ram[poly.poly_addr.get() as usize];
                    let verts_len = unsafe { raw_poly.attrs.verts_len() };

                    macro_rules! process_edge {
                        ($vert_i: expr, $edge: expr, $increasing: expr) => {{
                            let increasing = $increasing;
                            if y >= $edge.b_y() && $vert_i != poly.bot_i {
                                let mut prev_i = $vert_i;
                                let mut prev_vert_addr = $edge.b_addr();
                                let mut prev_vert =
                                    &rendering_data.vert_ram[prev_vert_addr.get() as usize];

                                while true {
                                    let i = if increasing {
                                        inc_poly_vert_index(prev_i, verts_len)
                                    } else {
                                        dec_poly_vert_index(prev_i, verts_len)
                                    };
                                    let vert_addr = raw_poly.verts[i.get() as usize];
                                    let vert = &rendering_data.vert_ram[vert_addr.get() as usize];

                                    if vert.coords[1] as u8 > y || i == poly.bot_i {
                                        $edge = Edge::new(
                                            &raw_poly,
                                            prev_i,
                                            prev_vert_addr,
                                            prev_vert,
                                            i,
                                            vert_addr,
                                            vert,
                                        );
                                        $vert_i = i;
                                        break;
                                    }

                                    prev_i = i;
                                    prev_vert = vert;
                                    prev_vert_addr = vert_addr;
                                }
                            }
                        }};
                    }

                    process_edge!(poly.l_vert_i, edges[0], raw_poly.attrs.is_front_facing());
                    process_edge!(poly.r_vert_i, edges[1], !raw_poly.attrs.is_front_facing());

                    let mut edges = [&edges[0], &edges[1]];
                    let mut ranges = [edges[0].line_x_range(y), edges[1].line_x_range(y)];

                    if ranges[1].0 < ranges[0].0 {
                        edges.swap(0, 1);
                        ranges.swap(0, 1);
                    }
                    // The left edge cannot extend further right than the end of the right edge
                    ranges[0].1 = ranges[0].1.min(ranges[1].1);

                    macro_rules! interp_edge {
                        ($i: expr, $x: expr) => {{
                            let edge = edges[$i];
                            let a = &rendering_data.vert_ram[edge.a_addr().get() as usize];
                            let b = &rendering_data.vert_ram[edge.b_addr().get() as usize];
                            let interp = edge.edge_interp(y, $x);
                            let vert_color = interp.color(a.color, b.color);
                            let uv = interp.uv(a.uv, b.uv);
                            let depth =
                                interp.depth(edge.a_z(), edge.b_z(), rendering_data.w_buffering);
                            let w = interp.w(edge.a_w(), edge.b_w());
                            (vert_color, uv, depth, w)
                        }};
                    }

                    let next_is_horiz = edges[0].b_y() == edges[1].b_y();

                    (
                        ranges,
                        [
                            edges[0].is_negative()
                                || !edges[0].is_x_major()
                                || (y + 1 == poly.bot_y && edges[0].is_x_major() && next_is_horiz),
                            (!edges[1].is_negative() && edges[1].is_x_major())
                                || edges[1].x_incr() == 0
                                || (y + 1 == poly.bot_y && edges[1].is_x_major() && next_is_horiz),
                        ],
                        [interp_edge!(0, ranges[0].0), interp_edge!(1, ranges[1].1)],
                    )
                }
                Edges::Dummy(edges) => {
                    let l_v = rendering_data.vert_ram[edges[0].addr().get() as usize];
                    let r_v = rendering_data.vert_ram[edges[1].addr().get() as usize];
                    (
                        [edges[0].line_x_range(), edges[1].line_x_range()],
                        [true, true],
                        [
                            (l_v.color, l_v.uv, edges[0].z(), edges[0].w()),
                            (r_v.color, r_v.uv, edges[1].z(), edges[1].w()),
                        ],
                    )
                }
            };

            let x_span_start = ranges[0].0;
            let x_span_len = ranges[1].1 + 1 - x_span_start;
            let is_wireframe = poly.alpha == 0;

            let fill_all_edges = rendering_data.control.antialiasing_enabled()
                || rendering_data.control.edge_marking_enabled()
                || is_wireframe
                || (poly.attrs.is_translucent() && rendering_data.control.alpha_blending_enabled());
            let fill_edges = if fill_all_edges {
                [true; 2]
            } else {
                fill_edges
            };

            let is_at_y_boundary = y == poly.top_y || y + 1 == poly.bot_y;

            let x_interp = InterpLineData::<false>::new(l_w, r_w);

            macro_rules! render_pixel {
                ($x: expr, $is_edge: expr) => {{
                    let x = $x;
                    let is_edge = $is_edge;

                    if poly.is_shadow && !attr_line[x as usize].stencil() {
                        continue;
                    }

                    let interp = x_interp.set_x(x - x_span_start, x_span_len);
                    let x = x as usize;
                    let depth =
                        interp.depth(l_depth, r_depth, rendering_data.w_buffering) & 0x00FF_FFFF;
                    if (poly.depth_test)(depth, depth_line[x], attr_line[x]) {
                        let vert_color = interp.color(l_vert_color, r_vert_color);
                        let uv = interp.uv(l_uv, r_uv);
                        let mut color = (poly.process_pixel)(rendering_data, poly, uv, vert_color);
                        let alpha = color[3];
                        if alpha > rendering_data.alpha_test_ref as u16 {
                            if alpha == 0x1F {
                                color_line[x] = color.cast();
                                depth_line[x] = depth;
                                attr_line[x] = PixelAttrs::from_opaque_poly_attrs(poly, is_edge);
                            } else {
                                let prev_attrs = attr_line[x];
                                if prev_attrs.translucent_poly_id() != poly.id | 0x40 {
                                    if rendering_data.control.alpha_blending_enabled() {
                                        let prev_color = color_line[x].cast();
                                        let prev_alpha = prev_color[3];
                                        if prev_alpha != 0 {
                                            color = ((color * InterpColor::splat(alpha + 1))
                                                + (prev_color * InterpColor::splat(31 - alpha)))
                                                >> 5;
                                            color[3] = alpha.max(prev_alpha);
                                        }
                                    }
                                    color_line[x] = color.cast();
                                    if poly.attrs.update_depth_for_translucent() {
                                        depth_line[x] = depth;
                                    }
                                    attr_line[x] =
                                        PixelAttrs::from_translucent_poly_attrs(poly, prev_attrs);
                                }
                            }
                        }
                    }
                }};
            }

            for i in 0..2 {
                if fill_edges[i] {
                    // If the range is out-of-screen don't render it
                    let (start, end) = clip_x_range(ranges[i]);
                    for x in start..=end {
                        render_pixel!(x as u16, true);
                    }
                }
            }

            if !is_wireframe || is_at_y_boundary {
                for x in ranges[0].1 + 1..ranges[1].0 {
                    render_pixel!(x, is_at_y_boundary);
                }
            }
        }
    }

    pub fn postprocess_line(
        &mut self,
        y: u8,
        scanline: &mut Scanline<u32>,
        rendering_data: &RenderingData,
    ) {
        let color_line = &mut self.color_buffer[y as usize].0;

        if rendering_data.control.edge_marking_enabled() {
            let y_ = y as usize + 1;

            for (x, color_dst) in color_line.iter_mut().enumerate() {
                let x_ = x + 1;

                let attrs = self.attr_buffer[y_].0[x_];
                if !attrs.is_opaque_edge() {
                    continue;
                }

                let opaque_poly_id = attrs.opaque_poly_id();
                let depth = self.depth_buffer[y_].0[x_];

                macro_rules! has_edge {
                    ($x: expr, $y: expr) => {
                        (depth < self.depth_buffer[$y].0[$x]
                            && self.attr_buffer[$y].0[$x].opaque_poly_id() != opaque_poly_id)
                    };
                }

                if has_edge!(x_, y_ - 1)
                    || has_edge!(x_, y_ + 1)
                    || has_edge!(x_ - 1, y_)
                    || has_edge!(x_ + 1, y_)
                {
                    let edge_color = rendering_data.edge_colors[(opaque_poly_id >> 3) as usize];
                    if rendering_data.control.antialiasing_enabled() {
                        *color_dst = (*color_dst + edge_color) >> 1;
                    } else {
                        *color_dst = edge_color;
                    }
                }
            }
        }

        let depth_line =
            <&mut [_; 256]>::try_from(&mut self.depth_buffer[y as usize + 1].0[1..257]).unwrap();
        let attr_line =
            <&mut [_; 256]>::try_from(&mut self.attr_buffer[y as usize + 1].0[1..257]).unwrap();

        if rendering_data.control.fog_enabled() {
            macro_rules! fog_density {
                ($x: expr) => {{
                    let z = depth_line[$x];
                    let offset = if z < rendering_data.fog_offset {
                        0
                    } else {
                        ((z - rendering_data.fog_offset) >> 2
                            << rendering_data.control.fog_depth_shift())
                        .min(32 << 17)
                    };
                    let index = (offset >> 17) as usize;
                    let fract = offset & 0x1_FFFF;
                    ((rendering_data.fog_densities[index] as u32 * (0x2_0000 - fract)
                        + rendering_data.fog_densities[index + 1] as u32 * fract)
                        >> 17) as u16
                }};
            }

            if rendering_data.control.fog_only_alpha() {
                let fog_alpha = rendering_data.fog_color[3] as u16;
                for x in 0..256 {
                    let attrs = attr_line[x];
                    if !attrs.fog_enabled() {
                        continue;
                    }
                    let density = fog_density!(x);
                    let alpha = color_line[x][3] as u16;
                    color_line[x][3] =
                        ((fog_alpha * density + alpha * (0x80 - density)) >> 7) as u8;
                }
            } else {
                let fog_color = rgb5_to_rgb6(rendering_data.fog_color.cast());
                for x in 0..256 {
                    let attrs = attr_line[x];
                    if !attrs.fog_enabled() {
                        continue;
                    }
                    let density = fog_density!(x);
                    let color = color_line[x].cast::<u16>();
                    color_line[x] = ((fog_color * InterpColor::splat(density)
                        + color * InterpColor::splat(0x80 - density))
                        >> 7)
                        .cast();
                }
            }
        }

        for (dst, src) in scanline.0.iter_mut().zip(&*color_line) {
            let [r, g, b, a] = src.to_array();
            *dst = r as u32 | (g as u32) << 6 | (b as u32) << 12 | (a as u32) << 18
        }
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}
