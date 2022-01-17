use crate::utils::Zero;
use packed_simd::{i16x2, i32x2, i32x4, i64x2, i64x4, i8x4, shuffle, u16x2, FromCast};

pub type TexCoords = i16x2;
pub type Color = i8x4;
pub type ConversionScreenCoords = i32x2;
pub type ScreenCoords = u16x2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Vertex {
    pub coords: i32x4,
    pub uv: TexCoords,
    pub color: i8x4,
}

unsafe impl Zero for Vertex {}

impl Vertex {
    pub const fn new() -> Self {
        Vertex {
            coords: i32x4::splat(0),
            uv: TexCoords::splat(0),
            color: i8x4::splat(0),
        }
    }

    pub(super) fn interpolate(&self, other: &Self, numer: i64, denom: i64) -> Self {
        macro_rules! interpolate_attr {
            ($ident: ident, $orig_ty: ty, $interp_ty: ty) => {
                self.$ident
                    + <$orig_ty>::from_cast(
                        (<$interp_ty>::from_cast(other.$ident)
                            - <$interp_ty>::from_cast(self.$ident))
                            * numer
                            / denom,
                    )
            };
        }
        Vertex {
            coords: interpolate_attr!(coords, i32x4, i64x4),
            uv: interpolate_attr!(uv, TexCoords, i64x2),
            color: interpolate_attr!(color, i8x4, i64x4),
        }
    }
}

fn cross_w_as_z(a: i64x4, b: i64x4) -> i64x4 {
    let a_ywxz: i64x4 = shuffle!(a, [1, 3, 0, 2]);
    let b_wxyz: i64x4 = shuffle!(b, [3, 0, 1, 2]);
    let a_wxyz: i64x4 = shuffle!(a, [3, 0, 1, 2]);
    let b_ywxz: i64x4 = shuffle!(b, [1, 3, 0, 2]);
    a_ywxz * b_wxyz - a_wxyz * b_ywxz
}

pub fn front_facing(v0: &Vertex, v1: &Vertex, v2: &Vertex) -> bool {
    // This is the same formula as used for back-face culling with a 3D pinhole camera; however,
    // since coordinates in clip space are divided by W, and not by Z (which could have no
    // meaning at all after projection), that must be reflected here; keeping that in mind,
    // the actual calculation for a front-facing polygon is just:
    // ((v2 - v1) × (v0 - v1)) · v1 >= 0
    let v1_64 = i64x4::from_cast(v1.coords);
    let normal = cross_w_as_z(
        i64x4::from_cast(v2.coords) - v1_64,
        i64x4::from_cast(v0.coords) - v1_64,
    );
    (normal * shuffle!(v1_64, [0, 1, 3, 2])).wrapping_sum() >= 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct ScreenVertex {
    pub coords: ScreenCoords,
    pub uv: TexCoords,
    pub color: i8x4,
}

unsafe impl Zero for ScreenVertex {}

impl ScreenVertex {
    pub const fn new() -> Self {
        ScreenVertex {
            coords: ScreenCoords::splat(0),
            uv: TexCoords::splat(0),
            color: i8x4::splat(0),
        }
    }
}
