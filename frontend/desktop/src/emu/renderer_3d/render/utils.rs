use dust_core::gpu::engine_3d::{
    InterpColor, PolyVertIndex, PolyVertsLen, Polygon, ScreenVertex, TexCoords,
};
use std::simd::{i32x2, u32x4};

pub fn inc_poly_vert_index(i: PolyVertIndex, verts: PolyVertsLen) -> PolyVertIndex {
    let new = i.get() + 1;
    if new == verts.get() {
        PolyVertIndex::new(0)
    } else {
        PolyVertIndex::new(new)
    }
}

pub fn dec_poly_vert_index(i: PolyVertIndex, verts: PolyVertsLen) -> PolyVertIndex {
    if i.get() == 0 {
        PolyVertIndex::new(verts.get() - 1)
    } else {
        PolyVertIndex::new(i.get() - 1)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Edge<'a> {
    a: &'a ScreenVertex,
    a_i: PolyVertIndex,
    a_y: u8,
    a_w: u16,

    b: &'a ScreenVertex,
    b_i: PolyVertIndex,
    b_y: u8,
    b_w: u16,

    x_ref: i32,
    x_incr: i32,

    is_x_major: bool,
    is_negative: bool,

    interp_ref: u16,
    interp_len: u16,
    interp_data: InterpLineData<true>,
}

impl<'a> Edge<'a> {
    pub fn new(
        poly: &Polygon,
        a: &'a ScreenVertex,
        a_i: PolyVertIndex,
        b: &'a ScreenVertex,
        b_i: PolyVertIndex,
    ) -> Self {
        // Slope calculation based on https://github.com/StrikerX3/nds-interp

        let a_w = poly.w_values[a_i.get() as usize];
        let b_w = poly.w_values[b_i.get() as usize];

        let a_x = a.coords[0];
        let b_x = b.coords[0];
        let a_y = a.coords[1] as u8;
        let b_y = b.coords[1] as u8;
        let x_diff = b_x as i16 - a_x as i16;
        let y_len = (b_y - a_y) as u16;

        let mut x_ref = (a_x as i32) << 18;

        let is_negative = x_diff < 0;
        let x_len = if is_negative {
            x_ref -= 1;
            -x_diff
        } else {
            x_diff
        } as u16;

        let is_x_major = x_len > y_len;
        if x_len >= y_len {
            if is_negative {
                x_ref -= 1 << 17;
            } else {
                x_ref += 1 << 17;
            }
        }

        let x_incr = if y_len == 0 {
            (x_len as i32) << 18
        } else {
            x_len as i32 * ((1 << 18) / y_len as i32)
        };

        Edge {
            a,
            a_i,
            a_y,
            a_w,

            b,
            b_i,
            b_y,
            b_w,

            x_ref,
            x_incr,

            is_x_major,
            is_negative,

            interp_ref: if is_x_major {
                a_x.min(b_x) as u16
            } else {
                a_y as u16
            },
            interp_len: if is_x_major { x_len } else { y_len },
            interp_data: InterpLineData::new(a_w, b_w),
        }
    }

    pub fn a(&self) -> &'a ScreenVertex {
        self.a
    }

    pub fn a_i(&self) -> PolyVertIndex {
        self.a_i
    }

    pub fn a_w(&self) -> u16 {
        self.a_w
    }

    pub fn b(&self) -> &'a ScreenVertex {
        self.b
    }

    pub fn b_i(&self) -> PolyVertIndex {
        self.b_i
    }

    pub fn b_y(&self) -> u8 {
        self.b_y
    }

    pub fn b_w(&self) -> u16 {
        self.b_w
    }

    pub fn x_incr(&self) -> i32 {
        self.x_incr
    }

    pub fn is_negative(&self) -> bool {
        self.is_negative
    }

    pub fn is_x_major(&self) -> bool {
        self.is_x_major
    }

    pub fn line_x_range(&self, y: u8) -> (u16, u16) {
        let line_x_disp = self.x_incr * (y - self.a_y) as i32;
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

    pub fn edge_interp(&self, y: u8, x: u16) -> InterpData<true> {
        self.interp_data.set_x(
            if self.is_x_major {
                let rel = x - self.interp_ref;
                if self.is_negative {
                    self.interp_len - rel
                } else {
                    rel
                }
            } else {
                y as u16 - self.interp_ref
            },
            self.interp_len,
        )
    }
}

trait InterpDir {}

pub enum YDir {}
impl InterpDir for YDir {}

pub enum XDir {}
impl InterpDir for XDir {}

#[derive(Clone, Copy, Debug)]
pub struct InterpLineData<const EDGE: bool> {
    force_linear: bool,
    p_w0_numer: u16,
    p_w0_denom: u16,
    p_w1_denom: u16,
}

impl<const EDGE: bool> InterpLineData<EDGE> {
    const PRECISION: u8 = 8 + EDGE as u8;

    pub fn new(a_w: u16, b_w: u16) -> Self {
        let linear_test_w_mask = if EDGE { 0x7E } else { 0x7F };
        let force_linear = a_w == b_w && (a_w | b_w) & linear_test_w_mask == 0;

        let (p_w0_numer, p_w0_denom, p_w1_denom) = if EDGE {
            if a_w & 1 != 0 && b_w & 1 == 0 {
                (a_w >> 1, a_w.wrapping_add(1) >> 1, b_w >> 1)
            } else {
                (a_w >> 1, a_w >> 1, b_w >> 1)
            }
        } else {
            (a_w, a_w, b_w)
        };

        InterpLineData {
            force_linear,
            p_w0_numer,
            p_w0_denom,
            p_w1_denom,
        }
    }

    pub fn set_x(&self, x: u16, len: u16) -> InterpData<EDGE> {
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
        let p_factor = if self.force_linear {
            l_factor
        } else {
            let numer = (x as u32 * self.p_w0_numer as u32) << Self::PRECISION;
            let denom =
                x as u32 * self.p_w0_denom as u32 + (len - x) as u32 * self.p_w1_denom as u32;
            if denom == 0 {
                // TODO: ???
                0
            } else {
                (numer / denom) as u16
            }
        };
        InterpData { l_factor, p_factor }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InterpData<const EDGE: bool> {
    l_factor: u16,
    p_factor: u16,
}

impl<const EDGE: bool> InterpData<EDGE> {
    const PRECISION: u8 = 8 + EDGE as u8;

    pub fn color(&self, a: InterpColor, b: InterpColor) -> InterpColor {
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

    pub fn uv(&self, a: TexCoords, b: TexCoords) -> TexCoords {
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

    pub fn depth(&self, a: i32, b: i32, w_buffering: bool) -> i32 {
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

    pub fn w(&self, a: u16, b: u16) -> u16 {
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
