#![feature(
    new_uninit,
    generic_const_exprs,
    const_mut_refs,
    const_trait_impl,
    portable_simd,
    lazy_cell
)]
#![warn(clippy::all)]
#![allow(incomplete_features, clippy::missing_safety_doc)]

mod common;
pub mod threaded;
pub use common::gfx::Renderer3dRx;
