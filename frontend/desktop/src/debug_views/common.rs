macro_rules! str_buf {
    ($buf: expr, $($args: tt)*) => {{
        use core::fmt::Write;
        $buf.clear();
        write!($buf, $($args)*).unwrap();
        &$buf
    }};
}

mod range_inclusive;
pub use range_inclusive::RangeInclusive;
pub mod disasm;
pub mod memory;
pub mod regs;
mod scrollbar;
use scrollbar::Scrollbar;
mod y_pos;

pub fn rgb_5_to_rgba8(value: u16) -> u32 {
    let value = value as u32;
    let rgb_6_8 = (value << 1 & 0x3E) | (value << 4 & 0x3E00) | (value << 7 & 0x3F_0000);
    0xFF00_0000 | rgb_6_8 << 2 | (rgb_6_8 >> 4 & 0x0003_0303)
}

pub fn rgb_5_to_rgba_f32(value: u16) -> [f32; 4] {
    [
        (value & 0x1F) as f32 / 31.0,
        (value >> 5 & 0x1F) as f32 / 31.0,
        (value >> 10 & 0x1F) as f32 / 31.0,
        1.0,
    ]
}
