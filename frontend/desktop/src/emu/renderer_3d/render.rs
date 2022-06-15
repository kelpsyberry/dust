mod utils;
use utils::{dec_poly_vert_index, inc_poly_vert_index, Edge, InterpLineData};

use super::{RenderingData, SharedData};
use dust_core::{
    gpu::{
        engine_3d::{InterpColor, PolyVertIndex, Polygon, PolygonAttrs, TexCoords},
        SCREEN_HEIGHT,
    },
    utils::{bitfield_debug, zeroed_box, Zero},
};
use std::sync::{atomic::Ordering, Arc};

#[derive(Clone, Copy)]
struct RenderingPolygon<'a> {
    poly: &'a Polygon,
    edges: [Edge<'a>; 2],
    height: u8,
    bot_i: PolyVertIndex,
    alpha: u8,
    id: u8,
    depth_test: fn(u32, u32, PixelAttrs) -> bool,
    process_pixel: fn(&RenderingData, &RenderingPolygon, TexCoords, InterpColor) -> InterpColor,
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct PixelAttrs(pub u32) {
        pub translucent: bool @ 13,
        pub back_facing: bool @ 14,

        pub fog_enabled: bool @ 15,

        pub edge_mask: u8 @ 16..=19,
        pub top_edge: bool @ 16,
        pub bottom_edge: bool @ 17,
        pub right_edge: bool @ 18,
        pub left_edge: bool @ 19,

        pub translucent_id: u8 @ 18..=23,
        pub opaque_id: u8 @ 24..=29,
    }
}

impl PixelAttrs {
    fn from_opaque_poly_attrs(attrs: PolygonAttrs) -> Self {
        PixelAttrs(attrs.0 & 0x3F00_8000)
    }
}

unsafe impl Zero for PixelAttrs {}

pub(super) struct RenderingState {
    pub shared_data: Arc<SharedData>,
    color_buffer: Box<[u64; 256]>,
    depth_buffer: Box<[u32; 256]>,
    attr_buffer: Box<[PixelAttrs; 256]>,
    polys: Vec<RenderingPolygon<'static>>,
}

fn decode_rgb_5(color: u16, alpha: u16) -> InterpColor {
    InterpColor::from_array([
        color & 0x1F,
        (color >> 5) & 0x1F,
        (color >> 10) & 0x1F,
        alpha,
    ])
}

fn rgb_5_to_6(color: InterpColor) -> InterpColor {
    (color << InterpColor::splat(1)) - color.lanes_ne(InterpColor::splat(0)).to_int().cast::<u16>()
}

fn encode_rgb_6(color: InterpColor) -> u32 {
    let [r, g, b, a] = color.to_array();
    r as u32 | (g as u32) << 6 | (b as u32) << 12 | (a as u32 >> 1) << 18
}

fn process_pixel<const FORMAT: u8, const MODE: u8>(
    rendering_data: &RenderingData,
    poly: &RenderingPolygon,
    uv: TexCoords,
    vert_color: InterpColor,
) -> InterpColor {
    let vert_color = vert_color >> InterpColor::splat(3);

    let mut vert_blend_color = match MODE {
        2 => {
            // TODO: Toon table
            rgb_5_to_6(InterpColor::from_array([0x1F, 0x1F, 0x1F, 0]))
        }
        3 => InterpColor::splat(vert_color[0]),
        _ => vert_color,
    };
    vert_blend_color[3] = if poly.alpha == 0 {
        0x3F
    } else {
        (poly.alpha << 1 | 1) as u16
    };

    let blended_color = if FORMAT == 0 {
        vert_blend_color
    } else {
        let tex_params = poly.poly.tex_params;
        let tex_base = (tex_params.vram_off() as usize) << 3;
        let pal_base = if FORMAT == 2 {
            (poly.poly.tex_palette_base as usize) << 3
        } else {
            (poly.poly.tex_palette_base as usize) << 4
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

        let tex_color = rgb_5_to_6(match FORMAT {
            1 => {
                let pixel = rendering_data.texture[(tex_base + i) & 0x7_FFFF];
                let color_index = pixel as usize & 0x1F;
                let raw_alpha = pixel >> 5;
                decode_rgb_5(
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
                decode_rgb_5(
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
                decode_rgb_5(
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
                decode_rgb_5(
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
                        decode_rgb_5(
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
                decode_rgb_5(
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
                decode_rgb_5(color, if color & 1 << 15 != 0 { 0x1F } else { 0 })
            }
        });

        match MODE {
            1 => match tex_color[3] {
                0 => vert_blend_color,
                0x3F => {
                    let mut color = tex_color;
                    color[3] = vert_blend_color[3];
                    color
                }
                _ => {
                    let mut color = (tex_color * InterpColor::splat(tex_color[3])
                        + vert_blend_color * InterpColor::splat(vert_blend_color[3]))
                        >> InterpColor::splat(6);
                    color[3] = vert_blend_color[3];
                    color
                }
            },

            _ => {
                ((tex_color + InterpColor::splat(1)) * (vert_blend_color + InterpColor::splat(1))
                    - InterpColor::splat(1))
                    >> InterpColor::splat(6)
            }
        }
    };

    if MODE == 3 {
        // TODO: Toon table
        let toon_color = rgb_5_to_6(InterpColor::from_array([0x1F, 0x1F, 0x1F, 0]));
        (blended_color + toon_color).min(InterpColor::from_array([0x3F, 0x3F, 0x3F, 0x3F]))
    } else {
        blended_color
    }
}

impl RenderingState {
    pub fn new(shared_data: Arc<SharedData>) -> Self {
        RenderingState {
            shared_data,
            color_buffer: zeroed_box(),
            depth_buffer: zeroed_box(),
            attr_buffer: zeroed_box(),
            polys: Vec::new(),
        }
    }

    pub fn run_frame(&mut self) {
        let rendering_data = unsafe { &*self.shared_data.rendering_data.get() };

        for poly in &rendering_data.poly_ram[..rendering_data.poly_ram_level as usize] {
            if poly.vertices_len.get() < 3 {
                continue;
            }

            let depth_test: fn(u32, u32, PixelAttrs) -> bool = if poly.attrs.depth_test_equal() {
                if rendering_data.w_buffering {
                    |a, b: u32, _b_attrs| b.wrapping_sub(a).wrapping_add(0xFF) <= 0x1FE
                } else {
                    |a, b, _b_attrs| b.wrapping_sub(a).wrapping_add(0x200) <= 0x400
                }
            } else if poly.is_front_facing {
                |a, b, b_attrs| {
                    if b_attrs.0
                        & PixelAttrs(0)
                            .with_translucent(true)
                            .with_back_facing(true)
                            .0
                        == PixelAttrs(0)
                            .with_translucent(false)
                            .with_back_facing(true)
                            .0
                    {
                        a <= b
                    } else {
                        a < b
                    }
                }
            } else {
                |a, b, _b_attrs| a < b
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
                    [
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
                    ][(mode << 3 | poly.tex_params.format()) as usize]
                } else {
                    [
                        process_pixel::<0, 0>,
                        process_pixel::<0, 1>,
                        process_pixel::<0, 2>,
                        process_pixel::<0, 3>,
                    ][mode as usize]
                }
            };

            let top_y = poly.top_y;
            let bot_y = poly.bot_y;

            if top_y == bot_y {
                let mut top_i = PolyVertIndex::new(0);
                let mut bot_i = top_i;
                let mut top_vert = &rendering_data.vert_ram[poly.vertices[0].get() as usize];
                let mut bot_vert = top_vert;
                for i in [
                    PolyVertIndex::new(1),
                    PolyVertIndex::new(poly.vertices_len.get() - 1),
                ] {
                    let vert =
                        &rendering_data.vert_ram[poly.vertices[i.get() as usize].get() as usize];
                    if vert.coords[0] < top_vert.coords[0] {
                        top_i = i;
                        top_vert = vert;
                    }
                    if vert.coords[0] > bot_vert.coords[0] {
                        bot_i = i;
                        bot_vert = vert;
                    }
                }

                self.polys.push(RenderingPolygon {
                    poly,
                    height: 1,
                    bot_i,
                    alpha: poly.attrs.alpha(),
                    id: poly.attrs.id(),
                    edges: [
                        Edge::new(poly, top_vert, top_i, top_vert, top_i),
                        Edge::new(poly, bot_vert, bot_i, bot_vert, bot_i),
                    ],
                    depth_test,
                    process_pixel,
                });
            } else {
                let (top_i, top_vert, bot_i) = unsafe {
                    let mut top_i = PolyVertIndex::new(0);
                    let mut bot_i = top_i;
                    let mut top_vert = None;
                    for i in 0..poly.vertices_len.get() as usize {
                        let i = PolyVertIndex::new(i as u8);
                        let vert = &rendering_data.vert_ram
                            [poly.vertices[i.get() as usize].get() as usize];
                        if vert.coords[1] as u8 == top_y && top_vert.is_none() {
                            top_i = i;
                            top_vert = Some(vert);
                        }
                        if vert.coords[1] as u8 == bot_y {
                            bot_i = i;
                        }
                    }
                    (top_i, top_vert.unwrap_unchecked(), bot_i)
                };

                let mut other_verts = [
                    inc_poly_vert_index(top_i, poly.vertices_len),
                    dec_poly_vert_index(top_i, poly.vertices_len),
                ]
                .map(|i| {
                    (
                        i,
                        &rendering_data.vert_ram[poly.vertices[i.get() as usize].get() as usize],
                    )
                });

                if !poly.is_front_facing {
                    other_verts.swap(0, 1);
                }

                self.polys.push(RenderingPolygon {
                    poly,
                    height: poly.bot_y - poly.top_y,
                    bot_i,
                    alpha: poly.attrs.alpha(),
                    id: poly.attrs.id(),
                    edges: [
                        Edge::new(poly, top_vert, top_i, other_verts[0].1, other_verts[0].0),
                        Edge::new(poly, top_vert, top_i, other_verts[1].1, other_verts[1].0),
                    ],
                    depth_test,
                    process_pixel,
                });
            }
        }

        for y in 0..SCREEN_HEIGHT as u8 {
            let scanline = &mut unsafe { &mut *self.shared_data.scanline_buffer.get() }[y as usize];
            scanline.0.fill(0);

            self.color_buffer.fill(0);
            self.depth_buffer.fill(0xFF_FFFF);
            self.attr_buffer.fill(PixelAttrs(0));

            for poly in self.polys.iter_mut() {
                if y.wrapping_sub(poly.poly.top_y) >= poly.height {
                    continue;
                }

                if poly.poly.top_y != poly.poly.bot_y {
                    macro_rules! process_edge {
                        ($edge: expr, $increasing: expr) => {{
                            if y >= $edge.b_y() {
                                let mut i = $edge.b_i();
                                let mut start_vert = $edge.b();
                                while i != poly.bot_i {
                                    i = if $increasing {
                                        inc_poly_vert_index(i, poly.poly.vertices_len)
                                    } else {
                                        dec_poly_vert_index(i, poly.poly.vertices_len)
                                    };
                                    let new_end_vert = &rendering_data.vert_ram
                                        [poly.poly.vertices[i.get() as usize].get() as usize];
                                    let new_b_y = new_end_vert.coords[1] as u8;

                                    if new_b_y > y || i == poly.bot_i {
                                        $edge = Edge::new(
                                            &poly.poly,
                                            start_vert,
                                            $edge.b_i(),
                                            new_end_vert,
                                            i,
                                        );
                                        break;
                                    }

                                    start_vert = new_end_vert;
                                }
                            }
                        }};
                    }

                    process_edge!(poly.edges[0], poly.poly.is_front_facing);
                    process_edge!(poly.edges[1], !poly.poly.is_front_facing);
                }

                let mut edges = [&poly.edges[0], &poly.edges[1]];
                let mut ranges = edges.map(|edge| edge.line_x_range(y));

                // Breaks EoS...?
                // if edges[1].x_incr == 0 {
                //     ranges[1].0 = ranges[1].0.saturating_sub(1);
                //     ranges[1].1 = ranges[1].1.saturating_sub(1);
                // }

                if ranges[1].1 <= ranges[0].0 {
                    edges.swap(0, 1);
                    ranges.swap(0, 1);
                }

                ranges[0].0 = ranges[0].0.max(0);
                ranges[0].1 = ranges[0].1.max(1);

                let x_span_start = ranges[0].0;
                let x_span_end = ranges[1].1;
                let x_span_len = x_span_end - x_span_start;
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

                let edge_mask = (y == poly.poly.top_y) as u8 | (y == poly.poly.bot_y - 1) as u8;

                let [(l_vert_color, l_uv, l_depth, l_w), (r_vert_color, r_uv, r_depth, r_w)] =
                    [(edges[0], x_span_start), (edges[1], x_span_end - 1)].map(|(edge, x)| {
                        let interp = edge.edge_interp(y, x);
                        let vert_color = interp.color(edge.a().color, edge.b().color);
                        let uv = interp.uv(edge.a().uv, edge.b().uv);
                        let depth = interp.depth(
                            poly.poly.depth_values[edge.a_i().get() as usize],
                            poly.poly.depth_values[edge.b_i().get() as usize],
                            rendering_data.w_buffering,
                        );
                        let w = interp.w(edge.a_w(), edge.b_w());
                        (vert_color, uv, depth, w)
                    });

                let x_interp = InterpLineData::<false>::new(l_w, r_w);

                for i in 0..2 {
                    if fill_edges[i] {
                        for x in ranges[i].0..ranges[i].1 {
                            let interp = x_interp.set_x(x - x_span_start, x_span_len);
                            let x = x as usize;
                            let depth = interp.depth(l_depth, r_depth, rendering_data.w_buffering)
                                as u32
                                & 0x00FF_FFFF;
                            if (poly.depth_test)(depth, self.depth_buffer[x], self.attr_buffer[x]) {
                                let vert_color = interp.color(l_vert_color, r_vert_color);
                                let uv = interp.uv(l_uv, r_uv);
                                let color =
                                    (poly.process_pixel)(rendering_data, poly, uv, vert_color);
                                scanline.0[x] = encode_rgb_6(color);
                                self.depth_buffer[x] = depth;
                                self.attr_buffer[x] =
                                    PixelAttrs::from_opaque_poly_attrs(poly.poly.attrs);
                            }
                        }
                    }
                }

                if !wireframe || edge_mask != 0 {
                    for x in ranges[0].1..ranges[1].0 {
                        let interp = x_interp.set_x(x - x_span_start, x_span_len);
                        let x = x as usize;
                        let depth = interp.depth(l_depth, r_depth, rendering_data.w_buffering)
                            as u32
                            & 0x00FF_FFFF;
                        if (poly.depth_test)(depth, self.depth_buffer[x], self.attr_buffer[x]) {
                            let vert_color = interp.color(l_vert_color, r_vert_color);
                            let uv = interp.uv(l_uv, r_uv);
                            let color = (poly.process_pixel)(rendering_data, poly, uv, vert_color);
                            scanline.0[x] = encode_rgb_6(color);
                            self.depth_buffer[x] = depth;
                            self.attr_buffer[x] =
                                PixelAttrs::from_opaque_poly_attrs(poly.poly.attrs);
                        }
                    }
                }
            }

            if self
                .shared_data
                .processing_scanline
                .compare_exchange(y, y + 1, Ordering::Release, Ordering::Relaxed)
                .is_err()
            {
                return;
            }
        }

        self.polys.clear();
    }
}
