#![feature(
    new_uninit,
    generic_const_exprs,
    const_mut_refs,
    const_trait_impl,
    portable_simd
)]
#![allow(incomplete_features)]

mod common;
pub mod threaded;
pub use common::gfx::Renderer3dRx;
