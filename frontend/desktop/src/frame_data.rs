#[cfg(feature = "debug-views")]
use crate::debug_views;
use dust_core::{gpu::Framebuffer, utils::zeroed_box};

#[repr(C)]
pub struct FrameData {
    pub fb: Box<Framebuffer>,
    pub fps: f32,
    #[cfg(feature = "debug-views")]
    pub debug: debug_views::FrameData,
}

impl Default for FrameData {
    fn default() -> Self {
        FrameData {
            fb: zeroed_box(),
            fps: 0.0,
            #[cfg(feature = "debug-views")]
            debug: debug_views::FrameData::new(),
        }
    }
}
