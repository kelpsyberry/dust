pub use super::super::engines_common::*;

use core::marker::ConstParamTy;

pub static COND_STRINGS: [&str; 16] = [
    "eq", "ne", "cs", "cc", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le", "", "nv",
];

#[derive(Clone, Copy, PartialEq, Eq, ConstParamTy, Debug)]
pub enum DpOpSpecialTy {
    Add,
    Cmp,
    Mov,
}

#[derive(Clone, Copy, PartialEq, Eq, ConstParamTy, Debug)]
pub enum LoadStoreMiscTy {
    Half,
    Double,
    SignedByte,
    SignedHalf,
}

#[derive(Clone, Copy, PartialEq, Eq, ConstParamTy, Debug)]
pub enum ThumbLoadStoreTy {
    Str,
    Strh,
    Strb,
    Ldrsb,
    Ldr,
    Ldrh,
    Ldrb,
    Ldrsh,
}
