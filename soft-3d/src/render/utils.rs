use core::simd::{i32x2, i64x2, u32x4, u64x4, SimdPartialOrd};
use dust_core::gpu::engine_3d::{
    InterpColor, PolyVertIndex, PolyVertsLen, Polygon, ScreenVertex, TexCoords, VertexAddr,
};

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
pub struct Edge {
    a_addr: VertexAddr,
    a_y: u8,
    a_z: u32,
    a_w: u16,

    b_addr: VertexAddr,
    b_y: u8,
    b_z: u32,
    b_w: u16,

    x_ref: i32,
    x_incr: i32,

    is_x_major: bool,
    is_negative: bool,

    interp_ref: u16,
    interp_len: u16,
    interp_data: InterpLineData<true>,
}

impl Edge {
    pub fn new(
        poly: &Polygon,
        a_i: PolyVertIndex,
        a_addr: VertexAddr,
        a: &ScreenVertex,
        b_i: PolyVertIndex,
        b_addr: VertexAddr,
        b: &ScreenVertex,
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
            a_addr,
            a_y,
            a_z: poly.depth_values[a_i.get() as usize],
            a_w,

            b_addr,
            b_y,
            b_z: poly.depth_values[b_i.get() as usize],
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

    pub fn a_addr(&self) -> VertexAddr {
        self.a_addr
    }

    pub fn a_z(&self) -> u32 {
        self.a_z
    }

    pub fn a_w(&self) -> u16 {
        self.a_w
    }

    pub fn b_addr(&self) -> VertexAddr {
        self.b_addr
    }

    pub fn b_y(&self) -> u8 {
        self.b_y
    }

    pub fn b_z(&self) -> u32 {
        self.b_z
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
        let start_frac_x = if self.is_negative {
            self.x_ref - line_x_disp
        } else {
            self.x_ref + line_x_disp
        };
        let start_x = (start_frac_x >> 18).clamp(0, 255) as u16;
        if self.is_x_major {
            if self.is_negative {
                (
                    (((start_frac_x + (0x1FF - (start_frac_x & 0x1FF)) - self.x_incr) >> 18) + 1)
                        .clamp(0, 255) as u16,
                    start_x,
                )
            } else {
                (
                    start_x,
                    (((((start_frac_x & !0x1FF) + self.x_incr) >> 18) - 1).clamp(0, 255) as u16),
                )
            }
        } else {
            (start_x, start_x)
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

const LINEAR_PRECISION: u8 = 30;

impl<const EDGE: bool> InterpLineData<EDGE> {
    const PERSP_PRECISION: u8 = 8 + EDGE as u8;

    pub fn new(a_w: u16, b_w: u16) -> Self {
        let linear_test_w_mask = if EDGE { 0x7E } else { 0x7F };
        let force_linear = a_w == b_w && (a_w | b_w) & linear_test_w_mask == 0;

        let (p_w0_numer, p_w0_denom, p_w1_denom) = if EDGE {
            if a_w & 1 != 0 && b_w & 1 == 0 {
                (a_w >> 1, ((a_w as u32 + 1) >> 1) as u16, b_w >> 1)
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
        let l_factor = (
            x,
            len,
            if len == 0 {
                0
            } else {
                (1 << LINEAR_PRECISION) / len as u32
            },
        );
        let p_factor = {
            let numer = (x as u32 * self.p_w0_numer as u32) << Self::PERSP_PRECISION;
            let denom =
                x as u32 * self.p_w0_denom as u32 + (len - x) as u32 * self.p_w1_denom as u32;
            if denom == 0 {
                // TODO: ???
                0
            } else {
                (numer / denom) as u16
            }
        };
        InterpData {
            l_factor,
            p_factor,
            force_linear: self.force_linear,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InterpData<const EDGE: bool> {
    l_factor: (u16, u16, u32),
    p_factor: u16,
    force_linear: bool,
}

impl<const EDGE: bool> InterpData<EDGE> {
    const PERSP_PRECISION: u8 = 8 + EDGE as u8;

    pub fn color(&self, a: InterpColor, b: InterpColor) -> InterpColor {
        if self.force_linear {
            let a = a.cast::<u64>();
            let b = b.cast::<u64>();
            let lower = a.simd_lt(b);
            let min = lower.select(a, b);
            let diff = lower.select(b, a) - min;
            let (x, len, denom) = self.l_factor;
            let factor = lower.select(
                u64x4::splat(x as u64 * denom as u64),
                u64x4::splat((len - x) as u64 * denom as u64),
            );
            (min + ((diff * factor) >> u64x4::splat(LINEAR_PRECISION as u64))).cast()
        } else {
            let a = a.cast::<u32>();
            let b = b.cast::<u32>();
            let lower = a.simd_lt(b);
            let min = lower.select(a, b);
            let diff = lower.select(b, a) - min;
            let factor = self.p_factor as u32;
            let factor = lower.select(
                u32x4::splat(factor),
                u32x4::splat((1 << Self::PERSP_PRECISION) - factor),
            );
            (min + ((diff * factor) >> u32x4::splat(Self::PERSP_PRECISION as u32))).cast()
        }
    }

    pub fn uv(&self, a: TexCoords, b: TexCoords) -> TexCoords {
        if self.force_linear {
            let a = a.cast::<i64>();
            let b = b.cast::<i64>();
            let lower = a.simd_lt(b);
            let min = lower.select(a, b);
            let diff = lower.select(b, a) - min;
            let (x, len, denom) = self.l_factor;
            let factor = lower.select(
                i64x2::splat(x as i64 * denom as i64),
                i64x2::splat((len - x) as i64 * denom as i64),
            );
            (min + ((diff * factor) >> i64x2::splat(LINEAR_PRECISION as i64))).cast()
        } else {
            let a = a.cast::<i32>();
            let b = b.cast::<i32>();
            let lower = a.simd_lt(b);
            let min = lower.select(a, b);
            let max = lower.select(b, a);
            let factor = self.p_factor as i32;
            let factor = lower.select(
                i32x2::splat(factor),
                i32x2::splat((1 << Self::PERSP_PRECISION) - factor),
            );
            (min + (((max - min) * factor) >> i32x2::splat(Self::PERSP_PRECISION as i32))).cast()
        }
    }

    pub fn depth(&self, a: u32, b: u32, w_buffering: bool) -> u32 {
        let a = a as i64;
        let b = b as i64;
        if w_buffering {
            let factor = self.p_factor as i64;
            (if b >= a {
                a + (((b - a) * factor) >> Self::PERSP_PRECISION)
            } else {
                b + (((a - b) * ((1 << Self::PERSP_PRECISION) - factor)) >> Self::PERSP_PRECISION)
            }) as u32
        } else {
            let (x, len, denom) = self.l_factor;
            (if b >= a {
                a + (((b - a) * x as i64 * denom as i64) >> LINEAR_PRECISION)
            } else {
                b + (((a - b) * (len - x) as i64 * denom as i64) >> LINEAR_PRECISION)
            }) as u32
        }
    }

    pub fn w(&self, a: u16, b: u16) -> u16 {
        if self.force_linear {
            let a = a as u64;
            let b = b as u64;
            let (x, len, denom) = self.l_factor;
            (if b >= a {
                a + (((b - a) * x as u64 * denom as u64) >> LINEAR_PRECISION)
            } else {
                b + (((a - b) * (len - x) as u64 * denom as u64) >> LINEAR_PRECISION)
            }) as u16
        } else {
            let a = a as u32;
            let b = b as u32;
            let factor = self.p_factor as u32;
            (if b >= a {
                a + (((b - a) * factor) >> Self::PERSP_PRECISION)
            } else {
                b + (((a - b) * ((1 << Self::PERSP_PRECISION) - factor)) >> Self::PERSP_PRECISION)
            }) as u16
        }
    }
}
