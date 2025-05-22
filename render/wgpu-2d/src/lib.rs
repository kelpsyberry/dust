#![feature(new_zeroed_alloc)]
#![warn(clippy::all)]
#![allow(clippy::missing_safety_doc)]

mod common;
pub mod threaded;
pub use common::gfx::Renderer3dRx;
