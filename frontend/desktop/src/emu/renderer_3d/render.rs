use super::SharedData;
use dust_core::{
    gpu::{
        engine_3d::{
            InterpColor, PolyVertIndex, PolyVertsLen, Polygon, PolygonAttrs, ScreenVertex,
            TexCoords,
        },
        SCREEN_HEIGHT,
    },
    utils::{bitfield_debug, zeroed_box, Zero},
};
use std::{
    simd::{i32x2, u32x4},
    sync::{atomic::Ordering, Arc},
};

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct Edge {
    a: ScreenVertex,
    b: ScreenVertex,
    x_ref: i32,
    x_incr: i32,
    x_min: u16,
    x_diff: u16,
    a_i: PolyVertIndex,
    b_i: PolyVertIndex,
    y_start: u8,
    y_end: u8,
    w_start: u16,
    w_end: u16,
    is_x_major: bool,
    is_negative: bool,
}

impl Edge {
    fn new(
        poly: &Polygon,
        a: ScreenVertex,
        a_i: PolyVertIndex,
        b: ScreenVertex,
        b_i: PolyVertIndex,
    ) -> Self {
        // Slope calculation based on https://github.com/StrikerX3/nds-interp

        let w_start = poly.w_values[a_i.get() as usize];
        let w_end = poly.w_values[b_i.get() as usize];

        let a_x = a.coords[0] as i32;
        let b_x = b.coords[0] as i32;
        let a_y = a.coords[1] as u8;
        let b_y = b.coords[1] as u8;
        let mut x_diff = b_x - a_x;
        let y_diff = b_y as i32 - a_y as i32;

        let mut x_ref = a_x << 18;

        let is_negative = x_diff < 0;
        if is_negative {
            x_ref -= 1;
            x_diff = -x_diff;
        }

        let is_x_major = x_diff > y_diff;
        if x_diff >= y_diff {
            if is_negative {
                x_ref -= 1 << 17;
            } else {
                x_ref += 1 << 17;
            }
        }

        let x_incr = if y_diff == 0 {
            x_diff << 18
        } else {
            x_diff * ((1 << 18) / y_diff)
        };

        Edge {
            a,
            a_i,
            b,
            b_i,
            x_ref,
            x_incr,
            x_min: a_x.min(b_x) as u16,
            x_diff: x_diff as u16,
            y_start: a_y,
            y_end: b_y,
            w_start,
            w_end,
            is_x_major,
            is_negative,
        }
    }

    fn line_x_range(&self, y: u8) -> (u16, u16) {
        let line_x_disp = self.x_incr * (y - self.y_start) as i32;
        let start_x = if self.is_negative {
            self.x_ref - line_x_disp
        } else {
            self.x_ref + line_x_disp
        };
        if self.is_x_major {
            if self.is_negative {
                (
                    (((start_x + (0x1FF - (start_x & 0x1FF)) - self.x_incr) >> 18) + 1)
                        .clamp(0, 256) as u16,
                    (start_x >> 18).clamp(0, 256) as u16,
                )
            } else {
                (
                    (start_x >> 18).clamp(0, 256) as u16,
                    ((((start_x & !0x1FF) + self.x_incr) >> 18).clamp(0, 256) as u16),
                )
            }
        } else {
            (
                (start_x >> 18).clamp(0, 256) as u16,
                ((start_x >> 18) + 1).clamp(0, 256) as u16,
            )
        }
    }

    fn edge_interp(&self, y: u8, x: u16) -> InterpData<true> {
        if self.is_x_major {
            let x = x - self.x_min;
            InterpData::new(
                self.x_diff,
                if self.is_negative { self.x_diff - x } else { x },
                self.w_start,
                self.w_end,
            )
        } else {
            InterpData::new(
                (self.y_end - self.y_start) as u16,
                (y - self.y_start) as u16,
                self.w_start,
                self.w_end,
            )
        }
    }
}

struct InterpData<const EDGE: bool> {
    l_factor: u16,
    p_factor: u16,
}

impl<const EDGE: bool> InterpData<EDGE> {
    const PRECISION: u8 = 8 + EDGE as u8;

    fn new(len: u16, x: u16, a_w: u16, b_w: u16) -> Self {
        let linear_test_w_mask = if EDGE { 0x7E } else { 0x7F };
        let force_linear = a_w == b_w && (a_w | b_w) & linear_test_w_mask == 0;
        let l_factor = {
            let numer = (x as u32) << Self::PRECISION;
            let denom = len as u32;
            if denom == 0 {
                // TODO: ???
                0
            } else {
                (numer / denom) as u16
            }
        };
        let p_factor = if force_linear {
            l_factor
        } else {
            let (w0_numer, w0_denom, w1_denom) = if EDGE {
                if a_w & 1 != 0 && b_w & 1 == 0 {
                    (a_w >> 1, a_w.wrapping_add(1) >> 1, b_w >> 1)
                } else {
                    (a_w >> 1, a_w >> 1, b_w >> 1)
                }
            } else {
                (a_w, a_w, b_w)
            };
            let numer = (x as u32 * w0_numer as u32) << Self::PRECISION;
            let denom = x as u32 * w0_denom as u32 + (len - x) as u32 * w1_denom as u32;
            if denom == 0 {
                // TODO: ???
                0
            } else {
                (numer / denom) as u16
            }
        };
        InterpData { l_factor, p_factor }
    }

    fn interp_color(&self, a: InterpColor, b: InterpColor) -> InterpColor {
        let factor = self.p_factor as u32;
        let a = a.cast::<u32>();
        let b = b.cast::<u32>();
        let lower = a.lanes_le(b);
        let min = lower.select(a, b);
        let max = lower.select(b, a);
        let factor = lower.select(
            u32x4::splat(factor),
            u32x4::splat((1 << Self::PRECISION) - factor),
        );
        (min + (((max - min) * factor) >> u32x4::splat(Self::PRECISION as u32))).cast()
    }

    fn interp_uv(&self, a: TexCoords, b: TexCoords) -> TexCoords {
        let factor = self.p_factor as i32;
        let a = a.cast::<i32>();
        let b = b.cast::<i32>();
        let lower = a.lanes_le(b);
        let min = lower.select(a, b);
        let max = lower.select(b, a);
        let factor = lower.select(
            i32x2::splat(factor),
            i32x2::splat((1 << Self::PRECISION) - factor),
        );
        (min + (((max - min) * factor) >> i32x2::splat(Self::PRECISION as i32))).cast()
    }

    fn interp_depth(&self, a: i32, b: i32, w_buffering: bool) -> i32 {
        let a = a as i64;
        let b = b as i64;
        let factor = (if w_buffering {
            self.p_factor
        } else {
            self.l_factor
        }) as i64;
        (if b >= a {
            a + (((b - a) * factor) >> Self::PRECISION)
        } else {
            b + (((a - b) * ((1 << Self::PRECISION) - factor)) >> Self::PRECISION)
        }) as i32
    }

    fn interp_w(&self, a: u16, b: u16) -> u16 {
        let factor = self.p_factor as u32;
        let a = a as u32;
        let b = b as u32;
        (if b >= a {
            a + (((b - a) * factor) >> Self::PRECISION)
        } else {
            b + (((a - b) * ((1 << Self::PRECISION) - factor)) >> Self::PRECISION)
        }) as u16
    }
}

fn inc_poly_vert_index(i: PolyVertIndex, verts: PolyVertsLen) -> PolyVertIndex {
    let new = i.get() + 1;
    if new == verts.get() {
        PolyVertIndex::new(0)
    } else {
        PolyVertIndex::new(new)
    }
}

fn dec_poly_vert_index(i: PolyVertIndex, verts: PolyVertsLen) -> PolyVertIndex {
    if i.get() == 0 {
        PolyVertIndex::new(verts.get() - 1)
    } else {
        PolyVertIndex::new(i.get() - 1)
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct RenderingPolygon {
    poly: Polygon,
    height: u8,
    bot_i: PolyVertIndex,
    alpha: u8,
    id: u8,
    edges: [Edge; 2],
}

unsafe impl Zero for RenderingPolygon {}

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
    polys: Box<[RenderingPolygon; 2048]>,
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
            polys: zeroed_box(),
        }
    }

    pub fn run_frame(&mut self) {
        let rendering_data = unsafe { &*self.shared_data.rendering_data.get() };
        {
            let len = rendering_data.poly_ram_level as usize;
            for (dst, src) in self.polys[..len]
                .iter_mut()
                .zip(&rendering_data.poly_ram[..len])
            {
                if src.vertices_len.get() < 3 {
                    continue;
                }

                let top_y = src.top_y;
                let bot_y = src.bot_y;

                if top_y == bot_y {
                    let mut top_i = PolyVertIndex::new(0);
                    let mut bot_i = top_i;
                    let mut top_vert = &rendering_data.vert_ram[src.vertices[0].get() as usize];
                    let mut bot_vert = top_vert;
                    for i in [
                        PolyVertIndex::new(1),
                        PolyVertIndex::new(src.vertices_len.get() - 1),
                    ] {
                        let vert =
                            &rendering_data.vert_ram[src.vertices[i.get() as usize].get() as usize];
                        if vert.coords[0] < top_vert.coords[0] {
                            top_i = i;
                            top_vert = vert;
                        }
                        if vert.coords[0] > bot_vert.coords[0] {
                            bot_i = i;
                            bot_vert = vert;
                        }
                    }

                    *dst = RenderingPolygon {
                        poly: *src,
                        height: 1,
                        bot_i,
                        alpha: src.attrs.alpha(),
                        id: src.attrs.id(),
                        edges: [
                            Edge::new(src, *top_vert, top_i, *top_vert, top_i),
                            Edge::new(src, *bot_vert, bot_i, *bot_vert, bot_i),
                        ],
                    };
                } else {
                    let (top_i, top_vert, bot_i) = unsafe {
                        let mut top_i = PolyVertIndex::new(0);
                        let mut bot_i = top_i;
                        let mut top_vert = None;
                        for i in 0..src.vertices_len.get() as usize {
                            let i = PolyVertIndex::new(i as u8);
                            let vert = &rendering_data.vert_ram
                                [src.vertices[i.get() as usize].get() as usize];
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
                        inc_poly_vert_index(top_i, src.vertices_len),
                        dec_poly_vert_index(top_i, src.vertices_len),
                    ]
                    .map(|i| {
                        (
                            i,
                            &rendering_data.vert_ram[src.vertices[i.get() as usize].get() as usize],
                        )
                    });

                    if !src.is_front_facing {
                        other_verts.swap(0, 1);
                    }

                    *dst = RenderingPolygon {
                        poly: *src,
                        height: src.bot_y - src.top_y,
                        bot_i,
                        alpha: src.attrs.alpha(),
                        id: src.attrs.id(),
                        edges: [
                            Edge::new(src, *top_vert, top_i, *other_verts[0].1, other_verts[0].0),
                            Edge::new(src, *top_vert, top_i, *other_verts[1].1, other_verts[1].0),
                        ],
                    };
                }
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
                            if y >= $edge.y_end {
                                let mut i = $edge.b_i;
                                let mut start_vert = &$edge.b;
                                while i != poly.bot_i {
                                    i = if $increasing {
                                        inc_poly_vert_index(i, poly.poly.vertices_len)
                                    } else {
                                        dec_poly_vert_index(i, poly.poly.vertices_len)
                                    };
                                    let new_end_vert = &rendering_data.vert_ram
                                        [poly.poly.vertices[i.get() as usize].get() as usize];
                                    let new_y_end = new_end_vert.coords[1] as u8;

                                    if new_y_end > y || i == poly.bot_i {
                                        $edge = Edge::new(
                                            &poly.poly,
                                            *start_vert,
                                            $edge.b_i,
                                            *new_end_vert,
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
                    fill_all_edges || edges[0].is_negative || !edges[0].is_x_major,
                    fill_all_edges
                        || (!edges[1].is_negative && edges[1].is_x_major)
                        || edges[1].x_incr == 0,
                ];

                let edge_mask = (y == poly.poly.top_y) as u8 | (y == poly.poly.bot_y - 1) as u8;

                let [(l_color, l_uv, l_depth, l_w), (r_color, r_uv, r_depth, r_w)] =
                    [(edges[0], x_span_start), (edges[1], x_span_end - 1)].map(|(edge, x)| {
                        let interp = edge.edge_interp(y, x);
                        let color = interp.interp_color(edge.a.color, edge.b.color);
                        let uv = interp.interp_uv(edge.a.uv, edge.b.uv);
                        let depth = interp.interp_depth(
                            poly.poly.depth_values[edge.a_i.get() as usize],
                            poly.poly.depth_values[edge.b_i.get() as usize],
                            rendering_data.w_buffering,
                        );
                        let w = interp.interp_w(edge.w_start, edge.w_end);
                        (color, uv, depth, w)
                    });

                for i in 0..2 {
                    if fill_edges[i] {
                        for x in ranges[i].0..ranges[i].1 {
                            let span_interp =
                                InterpData::<false>::new(x_span_len, x - x_span_start, l_w, r_w);
                            let x = x as usize;
                            let depth = span_interp.interp_depth(
                                l_depth,
                                r_depth,
                                rendering_data.w_buffering,
                            ) as u32
                                & 0x00FF_FFFF;
                            if if poly.poly.attrs.depth_test_equal() {
                                depth == self.depth_buffer[x]
                            } else {
                                depth < self.depth_buffer[x]
                            } {
                                scanline.0[x] =
                                    encode_rgb6(span_interp.interp_color(l_color, r_color), alpha);
                                self.depth_buffer[x] = depth;
                                self.attr_buffer[x] =
                                    PixelAttrs::from_opaque_poly_attrs(poly.poly.attrs);
                            }
                        }
                    }
                }

                if !wireframe || edge_mask != 0 {
                    for x in ranges[0].1..ranges[1].0 {
                        let span_interp =
                            InterpData::<false>::new(x_span_len, x - x_span_start, l_w, r_w);
                        let x = x as usize;
                        let depth =
                            span_interp.interp_depth(l_depth, r_depth, rendering_data.w_buffering)
                                as u32
                                & 0x00FF_FFFF;
                        if if poly.poly.attrs.depth_test_equal() {
                            depth == self.depth_buffer[x]
                        } else {
                            depth < self.depth_buffer[x]
                        } {
                            scanline.0[x] =
                                encode_rgb6(span_interp.interp_color(l_color, r_color), alpha);
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
    }
}
