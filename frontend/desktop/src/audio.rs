pub mod input;
mod interp;
pub mod output;
pub use interp::{Interp, InterpMethod};

const SYS_CLOCK_RATE: u32 = 1 << 25;
const ORIG_FRAME_RATE: f64 = SYS_CLOCK_RATE as f64 / (6.0 * 355.0 * 263.0);
pub const SAMPLE_RATE_ADJUSTMENT_RATIO: f64 = 60.0 / ORIG_FRAME_RATE;
