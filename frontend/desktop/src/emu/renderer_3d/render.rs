mod utils;
use utils::{dec_poly_vert_index, inc_poly_vert_index, Edge, InterpLineData};

use super::SharedData;
use dust_core::{
    gpu::{
        engine_3d::{InterpColor, PolyVertIndex, Polygon, PolygonAttrs},
        SCREEN_HEIGHT,
    },
    utils::{bitfield_debug, zeroed_box, Zero},
};
use std::sync::{atomic::Ordering, Arc};

#[derive(Clone, Copy, Debug)]
struct RenderingPolygon<'a> {
    poly: &'a Polygon,
    edges: [Edge<'a>; 2],
    height: u8,
    bot_i: PolyVertIndex,
    alpha: u8,
    id: u8,
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

fn encode_rgb6(color: InterpColor, alpha: u8) -> u32 {
    let [r, g, b, _] = color.to_array();
    (r as u32 >> 3) | (g as u32 >> 3) << 6 | (b as u32 >> 3) << 12 | (alpha as u32) << 18
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
                });
            }
        }

        for y in 0..SCREEN_HEIGHT as u8 {
            let scanline = &mut unsafe { &mut *self.shared_data.scanline_buffer.get() }[y as usize];
            scanline.0.fill(0);

            self.color_buffer.fill(0);
            self.depth_buffer.fill(0xFF_FFFF);
            self.attr_buffer.fill(PixelAttrs(0));

            for poly in self.polys[..rendering_data.poly_ram_level as usize].iter_mut() {
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
                let alpha = if wireframe { 31 } else { poly.alpha };

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

                let [(l_color, l_uv, l_depth, l_w), (r_color, r_uv, r_depth, r_w)] =
                    [(edges[0], x_span_start), (edges[1], x_span_end - 1)].map(|(edge, x)| {
                        let interp = edge.edge_interp(y, x);
                        let color = interp.color(edge.a().color, edge.b().color);
                        let uv = interp.uv(edge.a().uv, edge.b().uv);
                        let depth = interp.depth(
                            poly.poly.depth_values[edge.a_i().get() as usize],
                            poly.poly.depth_values[edge.b_i().get() as usize],
                            rendering_data.w_buffering,
                        );
                        let w = interp.w(edge.a_w(), edge.b_w());
                        (color, uv, depth, w)
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
                            if if poly.poly.attrs.depth_test_equal() {
                                depth == self.depth_buffer[x]
                            } else {
                                depth < self.depth_buffer[x]
                            } {
                                scanline.0[x] = encode_rgb6(interp.color(l_color, r_color), alpha);
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
                        if if poly.poly.attrs.depth_test_equal() {
                            depth == self.depth_buffer[x]
                        } else {
                            depth < self.depth_buffer[x]
                        } {
                            scanline.0[x] = encode_rgb6(interp.color(l_color, r_color), alpha);
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
