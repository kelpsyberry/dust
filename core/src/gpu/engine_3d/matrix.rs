use core::ops::Mul;
use packed_simd::{i32x4, i64x4, FromCast};

#[cfg(test)]
extern crate test;

#[derive(Clone, Copy, Debug)]
#[repr(align(16))]
pub struct MatrixBuffer<const LEN: usize>(pub [i32; LEN]);

#[derive(Clone, Copy, Debug)]
#[repr(align(64))]
pub struct Matrix(pub [i32x4; 4]);

impl Matrix {
    pub const fn zero() -> Self {
        Matrix([i32x4::splat(0); 4])
    }

    pub const fn identity() -> Self {
        Matrix([
            i32x4::new(0x1000, 0, 0, 0),
            i32x4::new(0, 0x1000, 0, 0),
            i32x4::new(0, 0, 0x1000, 0),
            i32x4::new(0, 0, 0, 0x1000),
        ])
    }

    pub fn new(arr: [i32; 16]) -> Self {
        Matrix([
            i32x4::from_slice_unaligned(&arr[..4]),
            i32x4::from_slice_unaligned(&arr[4..8]),
            i32x4::from_slice_unaligned(&arr[8..12]),
            i32x4::from_slice_unaligned(&arr[12..]),
        ])
    }

    pub fn get(&self, i: usize) -> i32 {
        self.0[i >> 2].extract(i & 3)
    }

    pub fn scale(&mut self, vec: [i32; 3]) {
        let vec = i64x4::from_cast(i32x4::new(vec[0], vec[1], vec[2], 0x1000));
        for row in &mut self.0 {
            *row = i32x4::from_cast((i64x4::from_cast(*row) * vec) >> 12);
        }
    }

    pub fn translate(&mut self, vec: [i32; 3]) {
        let mut last_row = i64x4::from_cast(self.0[3]) << 12;
        for (i, coeff) in vec.into_iter().enumerate() {
            let coeff = coeff as i64;
            last_row += i64x4::new(coeff, coeff, coeff, 0x1000) * i64x4::from_cast(self.0[i]);
        }
        self.0[3] = i32x4::from_cast(last_row >> 12);
    }

    pub fn mul_left_4x4(&mut self, other: MatrixBuffer<16>) {
        macro_rules! rows {
            ($($i: expr),*) => {
                [$(
                    i32x4::from_cast(
                        (i64x4::from_cast(self.0[0]) * other.0[$i * 4] as i64
                            + i64x4::from_cast(self.0[1]) * other.0[$i * 4 + 1] as i64
                            + i64x4::from_cast(self.0[2]) * other.0[$i * 4 + 2] as i64
                            + i64x4::from_cast(self.0[3]) * other.0[$i * 4 + 3] as i64)
                            >> 12,
                    )
                ),*]
            };
        }
        self.0 = rows!(0, 1, 2, 3);
    }

    pub fn mul_left_4x3(&mut self, other: MatrixBuffer<12>) {
        macro_rules! rows {
            ($($i: expr$(; $last_lhs_row_index: expr)?),*) => {
                [$(
                    i32x4::from_cast(
                        (
                            i64x4::from_cast(self.0[0]) * other.0[$i * 3] as i64
                                + i64x4::from_cast(self.0[1]) * other.0[$i * 3 + 1] as i64
                                + i64x4::from_cast(self.0[2]) * other.0[$i * 3 + 2] as i64
                               $( + (i64x4::from_cast(self.0[$last_lhs_row_index]) << 12))*
                        ) >> 12,
                    )
                ),*]
            };
        }
        self.0 = rows!(0, 1, 2, 3; 3);
    }

    pub fn mul_left_3x3(&mut self, other: MatrixBuffer<9>) {
        macro_rules! row {
            ($i: expr) => {
                i32x4::from_cast(
                    (i64x4::from_cast(self.0[0]) * other.0[$i * 3] as i64
                        + i64x4::from_cast(self.0[1]) * other.0[$i * 3 + 1] as i64
                        + i64x4::from_cast(self.0[2]) * other.0[$i * 3 + 2] as i64)
                        >> 12,
                )
            };
        }
        self.0 = [row!(0), row!(1), row!(2), self.0[3]];
    }

    pub fn mul_left_vec_i16(&self, vec: [i16; 3]) -> MatrixBuffer<4> {
        let mut result = MatrixBuffer([0; 4]);
        i32x4::from_cast(
            (i64x4::from_cast(self.0[0]) * vec[0] as i64
                + i64x4::from_cast(self.0[1]) * vec[1] as i64
                + i64x4::from_cast(self.0[2]) * vec[2] as i64
                + (i64x4::from_cast(self.0[3]) << 12))
                >> 12,
        )
        .write_to_slice_aligned(&mut result.0);
        result
    }
}

impl Mul for Matrix {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        macro_rules! rows {
            ($($i: expr),*) => {
                [$(
                    i32x4::from_cast(
                        (i64x4::from_cast(rhs.0[0]) * self.0[$i].extract(0) as i64
                            + i64x4::from_cast(rhs.0[1]) * self.0[$i].extract(1) as i64
                            + i64x4::from_cast(rhs.0[2]) * self.0[$i].extract(2) as i64
                            + i64x4::from_cast(rhs.0[3]) * self.0[$i].extract(3) as i64)
                            >> 12,
                    )
                ),*]
            };
        }
        Matrix(rows!(0, 1, 2, 3))
    }
}
