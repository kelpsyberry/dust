use crate::utils::Zero;
use packed_simd::{i16x2, i32x2, i32x4, i64x2, i64x4, i8x4, u16x2, FromCast};

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
