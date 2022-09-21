#![feature(
    portable_simd,
    maybe_uninit_uninit_array,
    maybe_uninit_slice,
    const_mut_refs,
    const_trait_impl,
    generic_const_exprs,
    new_uninit
)]
#![allow(incomplete_features)]

mod common;
pub mod sync;
#[cfg(feature = "threaded")]
pub mod threaded;
