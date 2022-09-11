mod utils;
use utils::{dec_poly_vert_index, inc_poly_vert_index, Edge, InterpLineData};

use super::RenderingData;
use core::simd::{SimdOrd, SimdPartialEq};
use dust_core::{
    gpu::{
        engine_3d::{
            Color, InterpColor, PolyAddr, PolyVertIndex, PolygonAttrs, TexCoords, TextureParams,
        },
        Scanline,
    },
    utils::{zeroed_box, Zero},
};

type DepthTestFn = fn(u32, u32, PixelAttrs) -> bool;
type ProcessPixelFn = fn(&RenderingData, &RenderingPolygon, TexCoords, InterpColor) -> InterpColor;

#[derive(Clone, Copy)]
struct RenderingPolygon {
    poly_addr: PolyAddr,
    attrs: PolygonAttrs,
    tex_params: TextureParams,
    tex_palette_base: u16,
    top_y: u8,
    bot_y: u8,
    height: u8,
    edges: [Edge; 2],
    l_vert_i: PolyVertIndex,
    r_vert_i: PolyVertIndex,
    bot_i: PolyVertIndex,
    alpha: u8,
    id: u8,
    is_front_facing: bool,
    depth_test: DepthTestFn,
    process_pixel: ProcessPixelFn,
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    const struct PixelAttrs(pub u32): Debug {
        pub edge_mask: u8 @ 0..=3,
        pub top_edge: bool @ 0,
        pub bottom_edge: bool @ 1,
        pub right_edge: bool @ 2,
        pub left_edge: bool @ 3,

        pub translucent: bool @ 13,
        pub back_facing: bool @ 14,

        pub fog_enabled: bool @ 15,

        pub translucent_id: u8 @ 16..=22,
        pub opaque_id: u8 @ 24..=29,
    }
}

impl PixelAttrs {
    #[inline]
    fn from_opaque_poly_attrs(poly: &RenderingPolygon) -> Self {
        PixelAttrs(poly.attrs.0 & 0x3F00_8000).with_back_facing(!poly.is_front_facing)
    }

    #[inline]
    fn from_translucent_poly_attrs(poly: &RenderingPolygon, opaque: PixelAttrs) -> Self {
        PixelAttrs(opaque.0 & 0x3F00_8000)
            .with_translucent(true)
            .with_back_facing(!poly.is_front_facing)
            .with_translucent_id(poly.id | 0x40)
    }
}

unsafe impl Zero for PixelAttrs {}

fn decode_rgb5(color: u16, alpha: u16) -> InterpColor {
    InterpColor::from_array([
        color & 0x1F,
        (color >> 5) & 0x1F,
        (color >> 10) & 0x1F,
        alpha,
    ])
}

#[inline]
fn rgb5_to_rgb6(color: InterpColor) -> InterpColor {
    let mut result = (color << InterpColor::splat(1))
        - color.simd_ne(InterpColor::splat(0)).to_int().cast::<u16>();
    result[3] >>= 1;
    result
}

fn expand_depth(depth: u16) -> u32 {
    let depth = depth as u32;
    depth << 9 | ((depth.wrapping_add(1) as i32) << 16 >> 31 & 0x1FF) as u32
}

fn process_pixel<const FORMAT: u8, const MODE: u8>(
    rendering_data: &RenderingData,
    poly: &RenderingPolygon,
    uv: TexCoords,
    vert_color: InterpColor,
) -> InterpColor {
    let vert_color = vert_color >> InterpColor::splat(3);

    let mut vert_blend_color = match MODE {
        2 => rgb5_to_rgb6(rendering_data.toon_colors[vert_color[0] as usize >> 1].cast()),
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

        let tex_color = rgb5_to_rgb6(match FORMAT {
            1 => {
                let pixel = rendering_data.texture[(tex_base + i) & 0x7_FFFF];
                let color_index = pixel as usize & 0x1F;
                let raw_alpha = pixel >> 5;
                decode_rgb5(
                    rendering_data
                        .tex_pal
                        .read_le::<u16>((pal_base + (color_index << 1)) & 0x1_FFFF),
                    (raw_alpha << 2 | raw_alpha >> 1) as u16,
                )
            }

            2 => {
                let color_index = rendering_data.texture[(tex_base + (i >> 2)) & 0x7_FFFF]
                    .wrapping_shr((i << 1) as u32) as usize
                    & 3;
                decode_rgb5(
                    rendering_data
                        .tex_pal
                        .read_le::<u16>(pal_base | color_index << 1),
                    if tex_params.use_color_0_as_transparent() && color_index == 0 {
                        0
                    } else {
                        0x1F
                    },
                )
            }

            3 => {
                let color_index = rendering_data.texture[(tex_base + (i >> 1)) & 0x7_FFFF]
                    .wrapping_shr((i << 2) as u32) as usize
                    & 0xF;
                decode_rgb5(
                    rendering_data
                        .tex_pal
                        .read_le::<u16>((pal_base + (color_index << 1)) & 0x1_FFFF),
                    if tex_params.use_color_0_as_transparent() && color_index == 0 {
                        0
                    } else {
                        0x1F
                    },
                )
            }

            4 => {
                let color_index = rendering_data.texture[(tex_base + i) & 0x7_FFFF] as usize;
                decode_rgb5(
                    rendering_data
                        .tex_pal
                        .read_le::<u16>((pal_base + (color_index << 1)) & 0x1_FFFF),
                    if tex_params.use_color_0_as_transparent() && color_index == 0 {
                        0
                    } else {
                        0x1F
                    },
                )
            }

            5 => {
                let texel_block_addr = tex_base + (v >> 2 << (tex_width_shift + 3) | (u & !3));
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

                match texel_value {
                    0 => color!(0),

                    1 => color!(1),

                    2 => match mode {
                        0 | 2 => color!(2),

                        1 => {
                            let color_0 = color!(0);
                            let color_1 = color!(1);
                            (color_0 + color_1) >> InterpColor::splat(1)
                        }

                        _ => {
                            let color_0 = color!(0);
                            let color_1 = color!(1);
                            (color_0 * InterpColor::splat(5) + color_1 * InterpColor::splat(3))
                                >> InterpColor::splat(3)
                        }
                    },

                    _ => match mode {
                        0 | 1 => InterpColor::splat(0),

                        2 => color!(3),

                        _ => {
                            let color_0 = color!(0);
                            let color_1 = color!(1);
                            (color_0 * InterpColor::splat(5) + color_1 * InterpColor::splat(3))
                                >> InterpColor::splat(3)
                        }
                    },
                }
            }

            6 => {
                let pixel = rendering_data.texture[(tex_base + i) & 0x7_FFFF];
                let color_index = pixel as usize & 7;
                let alpha = pixel >> 3;
                decode_rgb5(
                    rendering_data
                        .tex_pal
                        .read_le::<u16>((pal_base | color_index << 1) & 0x1_FFFF),
                    alpha as u16,
                )
            }

            _ => {
                let color = rendering_data
                    .texture
                    .read_le::<u16>((tex_base + (i << 1)) & 0x7_FFFE);
                decode_rgb5(color, if color & 1 << 15 != 0 { 0x1F } else { 0 })
            }
        });

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
                        >> InterpColor::splat(5);
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
        let toon_color = rgb5_to_rgb6(rendering_data.toon_colors[vert_color[0] as usize >> 1].cast());
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
    color_buffer: Box<Scanline<Color>>,
    depth_buffer: Box<Scanline<u32>>,
    attr_buffer: Box<Scanline<PixelAttrs>>,
    polys: Vec<RenderingPolygon>,
}

fn depth_test_equal_w(a: u32, b: u32, _: PixelAttrs) -> bool {
    a.wrapping_sub(b).wrapping_add(0xFF) <= 0x1FE
}

fn depth_test_equal_z(a: u32, b: u32, _: PixelAttrs) -> bool {
    a.wrapping_sub(b).wrapping_add(0x200) <= 0x400
}

fn depth_test_less_front_facing(a: u32, b: u32, b_attrs: PixelAttrs) -> bool {
    const MASK: u32 = PixelAttrs(0)
        .with_translucent(true)
        .with_back_facing(true)
        .0;
    const VALUE: u32 = PixelAttrs(0)
        .with_translucent(false)
        .with_back_facing(true)
        .0;
    if b_attrs.0 & MASK == VALUE {
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
            color_buffer: Box::new(Scanline([Color::splat(0); 256])),
            depth_buffer: zeroed_box(),
            attr_buffer: zeroed_box(),
            polys: Vec::with_capacity(2048),
        }
    }

    pub fn start_frame(&mut self, rendering_data: &RenderingData) {
        self.polys.clear();

        for poly_addr in 0..rendering_data.poly_ram_level {
            let poly_addr = unsafe { PolyAddr::new_unchecked(poly_addr) };
            let poly = &rendering_data.poly_ram[poly_addr.get() as usize];

            if poly.vertices_len.get() < 3 {
                continue;
            }

            let depth_test: fn(u32, u32, PixelAttrs) -> bool = if poly.attrs.depth_test_equal() {
                if rendering_data.w_buffering {
                    depth_test_equal_w
                } else {
                    depth_test_equal_z
                }
            } else if poly.is_front_facing {
                depth_test_less_front_facing
            } else {
                depth_test_less_back_facing
            };

            let process_pixel = if poly.attrs.mode() == 3 {
                // TODO: Shadow polygons
                process_pixel::<0, 0>
            } else {
                let mode = match poly.attrs.mode() {
                    2 => 2 + rendering_data.control.highlight_shading_enabled() as u8,
                    mode => mode,
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
                let mut top_vert_addr = poly.vertices[0];
                let mut top_vert = &rendering_data.vert_ram[top_vert_addr.get() as usize];
                let mut bot_vert_addr = top_vert_addr;
                let mut bot_vert = top_vert;

                macro_rules! vert {
                    ($i: expr) => {{
                        let i = $i;
                        let vert_addr = poly.vertices[i.get() as usize];
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
                vert!(PolyVertIndex::new(poly.vertices_len.get() - 1));

                self.polys.push(RenderingPolygon {
                    poly_addr,
                    attrs: poly.attrs,
                    tex_params: poly.tex_params,
                    tex_palette_base: poly.tex_palette_base,
                    top_y: poly.top_y,
                    bot_y: poly.bot_y,
                    height: 1,
                    alpha: poly.attrs.alpha(),
                    id: poly.attrs.id(),
                    is_front_facing: poly.is_front_facing,
                    edges: [
                        Edge::new(
                            poly,
                            top_i,
                            top_vert_addr,
                            top_vert,
                            top_i,
                            top_vert_addr,
                            top_vert,
                        ),
                        Edge::new(
                            poly,
                            bot_i,
                            bot_vert_addr,
                            bot_vert,
                            bot_i,
                            bot_vert_addr,
                            bot_vert,
                        ),
                    ],
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
                    for i in 0..poly.vertices_len.get() as usize {
                        let i = PolyVertIndex::new(i as u8);
                        let vert_addr = poly.vertices[i.get() as usize];
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
                        let addr = poly.vertices[i.get() as usize];
                        (i, addr, &rendering_data.vert_ram[addr.get() as usize])
                    }};
                }

                let mut other_verts = [
                    vert!(inc_poly_vert_index(top_i, poly.vertices_len)),
                    vert!(dec_poly_vert_index(top_i, poly.vertices_len)),
                ];

                if !poly.is_front_facing {
                    other_verts.swap(0, 1);
                }

                self.polys.push(RenderingPolygon {
                    poly_addr,
                    attrs: poly.attrs,
                    tex_params: poly.tex_params,
                    tex_palette_base: poly.tex_palette_base,
                    top_y: poly.top_y,
                    bot_y: poly.bot_y,
                    height: poly.bot_y - poly.top_y,
                    alpha: poly.attrs.alpha(),
                    id: poly.attrs.id(),
                    is_front_facing: poly.is_front_facing,
                    edges: [
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
                    ],
                    l_vert_i: other_verts[0].0,
                    r_vert_i: other_verts[1].0,
                    bot_i,
                    depth_test,
                    process_pixel,
                });
            }
        }
    }

    pub fn render_line(
        &mut self,
        y: u8,
        scanline: &mut Scanline<u32, 256>,
        rendering_data: &RenderingData,
    ) {
        scanline.0.fill(0);

        if rendering_data.control.rear_plane_bitmap_enabled() {
            let line_base = (y.wrapping_add(rendering_data.clear_image_offset[1]) as usize) << 9;
            let mut x_in_image = rendering_data.clear_image_offset[0];

            let color_line_base = 0x4_0000 | line_base;
            for x in 0..256 {
                let raw_color = rendering_data
                    .texture
                    .read_le(color_line_base | (x_in_image as usize) << 1);
                self.color_buffer.0[x] = rgb5_to_rgb6(decode_rgb5(
                    raw_color,
                    if raw_color >> 15 != 0 { 31 } else { 0 },
                ))
                .cast();
                x_in_image = x_in_image.wrapping_add(1);
            }

            let depth_line_base = 0x4_0000 | line_base;
            let pixel_attrs = PixelAttrs(0).with_opaque_id(rendering_data.clear_poly_id);
            for x in 0..256 {
                let raw_depth = rendering_data
                    .texture
                    .read_le(depth_line_base | (x_in_image as usize) << 1);
                self.depth_buffer.0[x] = expand_depth(raw_depth);
                self.attr_buffer.0[x] = pixel_attrs.with_fog_enabled(raw_depth >> 15 != 0);
                x_in_image = x_in_image.wrapping_add(1);
            }
        } else {
            self.color_buffer
                .0
                .fill(rgb5_to_rgb6(rendering_data.clear_color.cast()).cast());
            self.depth_buffer
                .0
                .fill(expand_depth(rendering_data.clear_depth));
            self.attr_buffer.0.fill(
                PixelAttrs(0)
                    .with_opaque_id(rendering_data.clear_poly_id)
                    .with_fog_enabled(rendering_data.rear_plane_fog_enabled),
            );
        }

        for poly in self.polys.iter_mut() {
            if y.wrapping_sub(poly.top_y) >= poly.height {
                continue;
            }

            if poly.top_y != poly.bot_y {
                let raw_poly = rendering_data.poly_ram[poly.poly_addr.get() as usize];

                macro_rules! process_edge {
                    ($vert_i: expr, $edge: expr, $increasing: expr) => {{
                        if y >= $edge.b_y() {
                            let mut i = *$vert_i;
                            let mut start_vert_addr = $edge.b_addr();
                            let mut start_vert =
                                &rendering_data.vert_ram[start_vert_addr.get() as usize];
                            while i != poly.bot_i {
                                i = if $increasing {
                                    inc_poly_vert_index(i, raw_poly.vertices_len)
                                } else {
                                    dec_poly_vert_index(i, raw_poly.vertices_len)
                                };
                                let new_end_vert_addr = raw_poly.vertices[i.get() as usize];
                                let new_end_vert =
                                    &rendering_data.vert_ram[new_end_vert_addr.get() as usize];
                                let new_b_y = new_end_vert.coords[1] as u8;

                                if new_b_y > y || i == poly.bot_i {
                                    $edge = Edge::new(
                                        &raw_poly,
                                        *$vert_i,
                                        start_vert_addr,
                                        start_vert,
                                        i,
                                        new_end_vert_addr,
                                        new_end_vert,
                                    );
                                    *$vert_i = i;
                                    break;
                                }

                                start_vert = new_end_vert;
                                start_vert_addr = new_end_vert_addr;
                            }
                        }
                    }};
                }

                process_edge!(&mut poly.l_vert_i, poly.edges[0], raw_poly.is_front_facing);
                process_edge!(&mut poly.r_vert_i, poly.edges[1], !raw_poly.is_front_facing);
            }

            let mut edges = [&poly.edges[0], &poly.edges[1]];
            let mut ranges = [edges[0].line_x_range(y), edges[1].line_x_range(y)];

            if ranges[1].1 <= ranges[0].0 {
                edges.swap(0, 1);
                ranges.swap(0, 1);
            }

            let x_span_start = ranges[0].0;
            let x_span_end = ranges[1].1;
            let x_span_len = x_span_end + 1 - x_span_start;
            let wireframe = poly.alpha == 0;

            let fill_all_edges = wireframe
                || rendering_data.control.antialiasing_enabled()
                || rendering_data.control.edge_marking_enabled();
            let fill_edges = [
                fill_all_edges || edges[0].is_negative() || !edges[0].is_x_major(),
                fill_all_edges
                    || (!edges[1].is_negative() && edges[1].is_x_major())
                    || edges[1].x_incr() == 0,
            ];

            let edge_mask = (y == poly.top_y) as u8 | (y == poly.bot_y - 1) as u8;

            macro_rules! interp_edge {
                ($i: expr, $x: expr) => {{
                    let edge = edges[$i];
                    let a = &rendering_data.vert_ram[edge.a_addr().get() as usize];
                    let b = &rendering_data.vert_ram[edge.b_addr().get() as usize];
                    let interp = edge.edge_interp(y, $x);
                    let vert_color = interp.color(a.color, b.color);
                    let uv = interp.uv(a.uv, b.uv);
                    let depth = interp.depth(edge.a_z(), edge.b_z(), rendering_data.w_buffering);
                    let w = interp.w(edge.a_w(), edge.b_w());
                    (vert_color, uv, depth, w)
                }};
            }

            let [(l_vert_color, l_uv, l_depth, l_w), (r_vert_color, r_uv, r_depth, r_w)] =
                [interp_edge!(0, x_span_start), interp_edge!(1, x_span_end)];

            let x_interp = InterpLineData::<false>::new(l_w, r_w);

            for i in 0..2 {
                if fill_edges[i] {
                    for x in ranges[i].0..=ranges[i].1 {
                        let interp = x_interp.set_x(x - x_span_start, x_span_len);
                        let x = x as usize;
                        let depth = interp.depth(l_depth, r_depth, rendering_data.w_buffering);
                        if (poly.depth_test)(depth, self.depth_buffer.0[x], self.attr_buffer.0[x]) {
                            let vert_color = interp.color(l_vert_color, r_vert_color);
                            let uv = interp.uv(l_uv, r_uv);
                            let mut color =
                                (poly.process_pixel)(rendering_data, poly, uv, vert_color);
                            let alpha = color[3];
                            if alpha > rendering_data.alpha_test_ref as u16 {
                                if alpha == 0x1F {
                                    self.color_buffer.0[x] = color.cast();
                                    self.depth_buffer.0[x] = depth;
                                    self.attr_buffer.0[x] =
                                        PixelAttrs::from_opaque_poly_attrs(poly);
                                } else {
                                    let prev_attrs = self.attr_buffer.0[x];
                                    if prev_attrs.translucent_id() != poly.id | 0x40 {
                                        if rendering_data.control.alpha_blending_enabled() {
                                            let prev_color = self.color_buffer.0[x].cast();
                                            let prev_alpha = prev_color[3];
                                            if prev_alpha != 0 {
                                                color = ((color * InterpColor::splat(alpha + 1))
                                                    + (prev_color
                                                        * InterpColor::splat(31 - alpha)))
                                                    >> InterpColor::splat(5);
                                                color[3] = alpha.max(prev_alpha);
                                            }
                                        }
                                        self.color_buffer.0[x] = color.cast();
                                        if poly.attrs.update_depth_for_translucent() {
                                            self.depth_buffer.0[x] = depth;
                                        }
                                        self.attr_buffer.0[x] =
                                            PixelAttrs::from_translucent_poly_attrs(
                                                poly, prev_attrs,
                                            );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !wireframe || edge_mask != 0 {
                for x in ranges[0].1 + 1..ranges[1].0 {
                    let interp = x_interp.set_x(x - x_span_start, x_span_len);
                    let x = x as usize;
                    let depth = interp.depth(l_depth, r_depth, rendering_data.w_buffering) as u32
                        & 0x00FF_FFFF;
                    if (poly.depth_test)(depth, self.depth_buffer.0[x], self.attr_buffer.0[x]) {
                        let vert_color = interp.color(l_vert_color, r_vert_color);
                        let uv = interp.uv(l_uv, r_uv);
                        let mut color = (poly.process_pixel)(rendering_data, poly, uv, vert_color);
                        let alpha = color[3];
                        if alpha > rendering_data.alpha_test_ref as u16 {
                            if alpha == 0x1F {
                                self.color_buffer.0[x] = color.cast();
                                self.depth_buffer.0[x] = depth;
                                self.attr_buffer.0[x] = PixelAttrs::from_opaque_poly_attrs(poly);
                            } else {
                                let prev_attrs = self.attr_buffer.0[x];
                                if prev_attrs.translucent_id() != poly.id | 0x40 {
                                    if rendering_data.control.alpha_blending_enabled() {
                                        let prev_color = self.color_buffer.0[x].cast();
                                        let prev_alpha = prev_color[3];
                                        if prev_alpha != 0 {
                                            color = ((color * InterpColor::splat(alpha + 1))
                                                + (prev_color * InterpColor::splat(31 - alpha)))
                                                >> InterpColor::splat(5);
                                            color[3] = alpha.max(prev_alpha);
                                        }
                                    }
                                    self.color_buffer.0[x] = color.cast();
                                    if poly.attrs.update_depth_for_translucent() {
                                        self.depth_buffer.0[x] = depth;
                                    }
                                    self.attr_buffer.0[x] =
                                        PixelAttrs::from_translucent_poly_attrs(poly, prev_attrs);
                                }
                            }
                        }
                    }
                }
            }
        }

        for x in 0..256 {
            let [r, g, b, a] = self.color_buffer.0[x].to_array();
            scanline.0[x] = r as u32 | (g as u32) << 6 | (b as u32) << 12 | (a as u32) << 18
        }
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}
