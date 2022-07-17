use crate::utils::Savestate;
use core::ops::Mul;
use core::simd::{i32x4, i64x4, Simd, SimdElement};

#[derive(Clone, Copy, Debug)]
#[repr(align(16))]
pub struct MatrixBuffer<const LEN: usize>(pub [i32; LEN]);

#[derive(Clone, Copy, Debug, Savestate)]
#[repr(align(64))]
pub struct Matrix(pub [i32x4; 4]);

impl Matrix {
    pub const fn zero() -> Self {
        Matrix([i32x4::splat(0); 4])
    }

    pub const fn identity() -> Self {
        Matrix([
            i32x4::from_array([0x1000, 0, 0, 0]),
            i32x4::from_array([0, 0x1000, 0, 0]),
            i32x4::from_array([0, 0, 0x1000, 0]),
            i32x4::from_array([0, 0, 0, 0x1000]),
        ])
    }

    pub fn new(arr: MatrixBuffer<16>) -> Self {
        Matrix([
            i32x4::from_slice(&arr.0[..4]),
            i32x4::from_slice(&arr.0[4..8]),
            i32x4::from_slice(&arr.0[8..12]),
            i32x4::from_slice(&arr.0[12..]),
        ])
    }

    pub fn get(&self, i: usize) -> i32 {
        self.0[i >> 2][i & 3]
    }

    pub fn scale(&mut self, vec: [i32; 3]) {
        self.0[0] =
            ((i64x4::splat(vec[0] as i64) * self.0[0].cast::<i64>()) >> i64x4::splat(12)).cast();
        self.0[1] =
            ((i64x4::splat(vec[1] as i64) * self.0[1].cast::<i64>()) >> i64x4::splat(12)).cast();
        self.0[2] =
            ((i64x4::splat(vec[2] as i64) * self.0[2].cast::<i64>()) >> i64x4::splat(12)).cast();
    }

    pub fn translate(&mut self, vec: [i32; 3]) {
        self.0[3] = (((self.0[3].cast::<i64>() << i64x4::splat(12))
            + i64x4::splat(vec[0] as i64) * self.0[0].cast::<i64>()
            + i64x4::splat(vec[1] as i64) * self.0[1].cast::<i64>()
            + i64x4::splat(vec[2] as i64) * self.0[2].cast::<i64>())
            >> i64x4::splat(12))
        .cast();
    }

    pub fn mul_left_4x4(&mut self, other: MatrixBuffer<16>) {
        macro_rules! rows {
            ($($i: expr),*) => {
                [$(
                    ((self.0[0].cast::<i64>() * i64x4::splat(other.0[$i * 4] as i64)
                        + self.0[1].cast::<i64>() * i64x4::splat(other.0[$i * 4 + 1] as i64)
                        + self.0[2].cast::<i64>() * i64x4::splat(other.0[$i * 4 + 2] as i64)
                        + self.0[3].cast::<i64>() * i64x4::splat(other.0[$i * 4 + 3] as i64))
                        >> i64x4::splat(12))
                    .cast()
                ),*]
            };
        }
        self.0 = rows!(0, 1, 2, 3);
    }

    pub fn mul_left_4x3(&mut self, other: MatrixBuffer<12>) {
        macro_rules! rows {
            ($($i: expr$(; $last_lhs_row_index: expr)?),*) => {
                [$(
                    ((self.0[0].cast::<i64>() * i64x4::splat(other.0[$i * 3] as i64)
                        + self.0[1].cast::<i64>() * i64x4::splat(other.0[$i * 3 + 1] as i64)
                        + self.0[2].cast::<i64>() * i64x4::splat(other.0[$i * 3 + 2] as i64)
                        $( + (self.0[$last_lhs_row_index].cast::<i64>() << i64x4::splat(12)))*)
                        >> i64x4::splat(12))
                    .cast()
                ),*]
            };
        }
        self.0 = rows!(0, 1, 2, 3; 3);
    }

    pub fn mul_left_3x3(&mut self, other: MatrixBuffer<9>) {
        macro_rules! row {
            ($i: expr) => {
                ((self.0[0].cast::<i64>() * i64x4::splat(other.0[$i * 3] as i64)
                    + self.0[1].cast::<i64>() * i64x4::splat(other.0[$i * 3 + 1] as i64)
                    + self.0[2].cast::<i64>() * i64x4::splat(other.0[$i * 3 + 2] as i64))
                    >> i64x4::splat(12))
                .cast()
            };
        }
        self.0 = [row!(0), row!(1), row!(2), self.0[3]];
    }

    pub fn mul_left_vec3<T: Into<i64> + Copy, U: SimdElement>(&self, vec: [T; 3]) -> Simd<U, 4> {
        ((self.0[0].cast::<i64>() * i64x4::splat(vec[0].into())
            + self.0[1].cast::<i64>() * i64x4::splat(vec[1].into())
            + self.0[2].cast::<i64>() * i64x4::splat(vec[2].into())
            + (self.0[3].cast::<i64>() << i64x4::splat(12)))
            >> i64x4::splat(12))
        .cast()
    }

    pub fn mul_left_vec2_one_one<T: Into<i64> + SimdElement, U: SimdElement>(
        &self,
        vec: Simd<T, 2>,
    ) -> Simd<U, 4> {
        ((self.0[0].cast::<i64>() * i64x4::splat(vec[0].into())
            + self.0[1].cast::<i64>() * i64x4::splat(vec[1].into())
            + self.0[2].cast::<i64>()
            + self.0[3].cast::<i64>())
            >> i64x4::splat(12))
        .cast()
    }

    pub fn mul_left_vec3_zero<T: Into<i64> + Copy, U: SimdElement, const SHIFT: u8>(
        &self,
        vec: [T; 3],
    ) -> Simd<U, 4> {
        ((self.0[0].cast::<i64>() * i64x4::splat(vec[0].into())
            + self.0[1].cast::<i64>() * i64x4::splat(vec[1].into())
            + self.0[2].cast::<i64>() * i64x4::splat(vec[2].into()))
            >> i64x4::splat(SHIFT as i64))
        .cast()
    }
}

impl Mul for Matrix {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        macro_rules! rows {
            ($($i: expr),*) => {
                [$({
                    let [x, y, z, w] = self.0[$i].cast::<i64>().to_array();
                    ((rhs.0[0].cast::<i64>() * i64x4::splat(x)
                        + rhs.0[1].cast::<i64>() * i64x4::splat(y)
                        + rhs.0[2].cast::<i64>() * i64x4::splat(z)
                        + rhs.0[3].cast::<i64>() * i64x4::splat(w))
                        >> i64x4::splat(12))
                    .cast()
                }),*]
            };
        }
        Matrix(rows!(0, 1, 2, 3))
    }
}
