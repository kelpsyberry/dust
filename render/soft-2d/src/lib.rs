#![feature(generic_const_exprs, new_uninit, portable_simd)]
#![allow(incomplete_features)]

mod common;
pub mod sync;
#[cfg(feature = "threaded")]
pub mod threaded;
