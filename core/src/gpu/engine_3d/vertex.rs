use crate::utils::Savestate;
use core::simd::{
    i16x2, i32x4, i64x2, i64x4, mask64x4, simd_swizzle, u16x2, u16x4, u8x4, SimdInt, SimdPartialEq,
};

pub type TexCoords = i16x2;
pub type Color = u8x4;
pub type InterpColor = u16x4;
pub type ScreenCoords = u16x2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Savestate)]
#[repr(C)]
pub struct Vertex {
    pub coords: i32x4,
    pub uv: TexCoords,
    pub color: Color,
}

impl Vertex {
    pub fn new() -> Self {
        Vertex {
            coords: i32x4::splat(0),
            uv: TexCoords::splat(0),
            color: Color::splat(0),
        }
    }

    pub(super) fn interpolate(&self, other: &Self, numer: i64, denom: i64) -> Self {
        let numer_4 = i64x4::splat(numer);
        let denom_4 = i64x4::splat(denom);
        let numer_2 = i64x2::splat(numer);
        let denom_2 = i64x2::splat(denom);
        macro_rules! interpolate_attr {
            ($ident: ident, $numer: expr, $denom: expr) => {
                self.$ident
                    + ((other.$ident.cast::<i64>() - self.$ident.cast::<i64>()) * $numer / $denom)
                        .cast()
            };
        }
        Vertex {
            coords: interpolate_attr!(coords, numer_4, denom_4),
            uv: interpolate_attr!(uv, numer_2, denom_2),
            color: interpolate_attr!(color, numer_4, denom_4),
        }
    }
}

impl Default for Vertex {
    fn default() -> Self {
        Self::new()
    }
}

fn cross_w_as_z(a: i64x4, b: i64x4) -> i64x4 {
    let a_ywxz: i64x4 = simd_swizzle!(a, [1, 3, 0, 2]);
    let b_wxyz: i64x4 = simd_swizzle!(b, [3, 0, 1, 2]);
    let a_wxyz: i64x4 = simd_swizzle!(a, [3, 0, 1, 2]);
    let b_ywxz: i64x4 = simd_swizzle!(b, [1, 3, 0, 2]);
    a_ywxz * b_wxyz - a_wxyz * b_ywxz
}

pub fn culled(
    v0: &Vertex,
    v1: &Vertex,
    v2: &Vertex,
    show_front: bool,
    show_back: bool,
) -> (bool, bool) {
    // This is the same formula as used for back-face culling with a 3D pinhole camera; however,
    // since coordinates in clip space are divided by W, and not by Z (which could have no meaning
    // at all after projection), that must be reflected here; keeping that in mind, the actual
    // calculation for a front-facing polygon is just:
    // ((v2 - v1) × (v0 - v1)) · v1 >= 0
    let v1_64 = v1.coords.cast::<i64>();
    let mut normal = cross_w_as_z(v2.coords.cast() - v1_64, v0.coords.cast() - v1_64);
    // Normalize the normal's components so that they fit in a 32-bit integer, to avoid overflows
    while ((normal >> i64x4::splat(31) ^ normal >> i64x4::splat(63)).simd_ne(i64x4::splat(0))
        & mask64x4::from_array([true, true, true, false]))
    .any()
    {
        normal >>= i64x4::splat(4);
    }
    let dot = (normal * simd_swizzle!(v1_64, [0, 1, 3, 2])).reduce_sum();
    let is_front_facing = dot > 0;
    (
        (is_front_facing && !show_front) || (dot < 0 && !show_back),
        is_front_facing,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Savestate)]
pub struct ScreenVertex {
    pub coords: ScreenCoords,
    #[cfg(feature = "3d-hi-res-coords")]
    pub hi_res_coords: ScreenCoords,
    pub uv: TexCoords,
    pub color: InterpColor,
}

impl ScreenVertex {
    pub fn new() -> Self {
        ScreenVertex {
            coords: ScreenCoords::splat(0),
            #[cfg(feature = "3d-hi-res-coords")]
            hi_res_coords: ScreenCoords::splat(0),
            uv: TexCoords::splat(0),
            color: InterpColor::splat(0),
        }
    }
}

impl Default for ScreenVertex {
    fn default() -> Self {
        Self::new()
    }
}
