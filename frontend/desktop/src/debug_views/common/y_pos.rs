use core::ops::{Add, Div, Mul, Rem, Sub};

pub type YPosRaw = u128;
pub type SignedYPosRaw = i128;

const FRACT_BITS: u32 = 8;

macro_rules! impl_bin_ops {
    ($ty: ty, $raw: ty) => {
        impl_bin_ops!(
            $ty, $raw;
            Add, add;
            Sub, sub;
            Mul<shifted_res>, mul;
            Div<shifted_lhs>, div;
            Rem, rem;
            Add<raw shifted>, add;
            Sub<raw shifted>, sub;
            Mul<raw>, mul;
            Div<raw>, div;
            Rem<raw shifted>, rem;
        );
    };
    ($ty: ty, $raw: ty;) => {};
    ($ty: ty, $raw: ty; $trait: ident, $fn: ident; $($remaining: tt)*) => {
        impl $trait for $ty {
            type Output = Self;
            #[inline]
            fn $fn(self, rhs: Self) -> Self::Output {
                Self(self.0.$fn(rhs.0))
            }
        }
        impl_bin_ops!($ty, $raw; $trait<f32>, $fn; $($remaining)*);
    };
    ($ty: ty, $raw: ty; $trait: ident<shifted_lhs>, $fn: ident; $($remaining: tt)*) => {
        impl $trait for $ty {
            type Output = Self;
            #[inline]
            #[allow(clippy::suspicious_arithmetic_impl)]
            fn $fn(self, rhs: Self) -> Self::Output {
                Self((self.0 << FRACT_BITS).$fn(rhs.0))
            }
        }
        impl_bin_ops!($ty, $raw; $trait<f32>, $fn; $($remaining)*);
    };
    ($ty: ty, $raw: ty; $trait: ident<shifted_res>, $fn: ident; $($remaining: tt)*) => {
        impl $trait for $ty {
            type Output = Self;
            #[inline]
            #[allow(clippy::suspicious_arithmetic_impl)]
            fn $fn(self, rhs: Self) -> Self::Output {
                Self(self.0.$fn(rhs.0) >> FRACT_BITS)
            }
        }
        impl_bin_ops!($ty, $raw; $trait<f32>, $fn; $($remaining)*);
    };
    ($ty: ty, $raw: ty; $trait: ident<raw>, $fn: ident; $($remaining: tt)*) => {
        impl $trait<$raw> for $ty {
            type Output = Self;
            #[inline]
            fn $fn(self, rhs: $raw) -> Self::Output {
                Self(self.0.$fn(rhs))
            }
        }
        impl_bin_ops!($ty, $raw; $($remaining)*);
    };
    ($ty: ty, $raw: ty; $trait: ident<raw shifted>, $fn: ident; $($remaining: tt)*) => {
        impl $trait<$raw> for $ty {
            type Output = Self;
            #[inline]
            #[allow(clippy::suspicious_arithmetic_impl)]
            fn $fn(self, rhs: $raw) -> Self::Output {
                Self(self.0.$fn(rhs << FRACT_BITS))
            }
        }
        impl_bin_ops!($ty, $raw; $($remaining)*);
    };
    ($ty: ty, $raw: ty; $trait: ident<f32>, $fn: ident; $($remaining: tt)*) => {
        impl $trait<f32> for $ty {
            type Output = Self;
            #[inline]
            fn $fn(self, rhs: f32) -> Self::Output {
                self.$fn(Self::from(rhs))
            }
        }
        impl_bin_ops!($ty, $raw; $($remaining)*);
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct YPos(pub YPosRaw);

impl YPos {
    #[inline]
    pub fn as_signed(self) -> SignedYPos {
        SignedYPos(self.0 as SignedYPosRaw)
    }

    #[inline]
    pub fn saturating_sub(self, other: Self) -> Self {
        YPos(self.0.saturating_sub(other.0))
    }

    #[inline]
    pub fn div_into_int(self, other: Self) -> YPosRaw {
        self.0 / other.0
    }

    #[inline]
    pub fn div_into_f32(self, other: Self) -> f32 {
        YPos((self.0 << 8) / other.0).into()
    }
}

impl From<YPosRaw> for YPos {
    #[inline]
    fn from(int: YPosRaw) -> Self {
        YPos(int << FRACT_BITS)
    }
}

impl From<YPos> for YPosRaw {
    #[inline]
    fn from(value: YPos) -> Self {
        value.0 >> FRACT_BITS
    }
}

impl From<f32> for YPos {
    #[inline]
    fn from(value: f32) -> Self {
        YPos((value as f64 * (1 << FRACT_BITS) as f64) as YPosRaw)
    }
}

impl From<YPos> for f32 {
    #[inline]
    fn from(value: YPos) -> Self {
        ((value.0 as f64) / (1 << FRACT_BITS) as f64) as f32
    }
}

impl_bin_ops!(YPos, YPosRaw);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SignedYPos(pub SignedYPosRaw);

impl SignedYPos {
    #[inline]
    pub fn as_unsigned(self) -> YPos {
        YPos(self.0 as YPosRaw)
    }
}

impl From<SignedYPosRaw> for SignedYPos {
    #[inline]
    fn from(int: SignedYPosRaw) -> Self {
        SignedYPos(int << FRACT_BITS)
    }
}

impl From<SignedYPos> for SignedYPosRaw {
    #[inline]
    fn from(value: SignedYPos) -> Self {
        value.0 >> FRACT_BITS
    }
}

impl From<f32> for SignedYPos {
    #[inline]
    fn from(value: f32) -> Self {
        SignedYPos((value as f64 * (1 << FRACT_BITS) as f64) as SignedYPosRaw)
    }
}

impl From<SignedYPos> for f32 {
    #[inline]
    fn from(value: SignedYPos) -> Self {
        ((value.0 as f64) / (1 << FRACT_BITS) as f64) as f32
    }
}

impl_bin_ops!(SignedYPos, SignedYPosRaw);
