#![feature(generic_const_exprs, new_uninit)]
#![allow(incomplete_features)]

mod common;
pub mod sync;
#[cfg(feature = "threaded")]
pub mod threaded;
