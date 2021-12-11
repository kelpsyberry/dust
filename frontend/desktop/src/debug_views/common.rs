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
