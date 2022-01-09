use packed_simd::{i16x2, i32x4, i64x2, i64x4, i8x4, FromCast};

pub type TexCoords = i16x2;

pub fn decode_rgb_5(value: u16) -> i8x4 {
    i8x4::new(
        value as i8 & 0x1F,
        (value >> 5) as i8 & 0x1F,
        (value >> 10) as i8 & 0x1F,
        1,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vertex {
    pub coords: i32x4,
    pub uv: i16x2,
    pub color: i8x4,
}

impl Vertex {
    pub const fn zero() -> Self {
        Vertex {
            coords: i32x4::splat(0),
            uv: i16x2::splat(0),
            color: i8x4::splat(0),
        }
    }

    pub(super) fn interpolate(&self, other: &Self, mut numer: i64, denom: i64) -> Self {
        numer <<= 12;
        macro_rules! interpolate_attr {
            ($ident: ident, $orig_ty: ty, $interp_ty: ty) => {
                <$orig_ty>::from_cast(
                    ((<$interp_ty>::from_cast(self.$ident) << 12)
                        + (<$interp_ty>::from_cast(other.$ident)
                            - <$interp_ty>::from_cast(self.$ident))
                            * numer
                            / denom)
                        >> 12,
                )
            };
        }
        Vertex {
            coords: interpolate_attr!(coords, i32x4, i64x4),
            uv: interpolate_attr!(uv, i16x2, i64x2),
            color: interpolate_attr!(color, i8x4, i64x4),
        }
    }
}
