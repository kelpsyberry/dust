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

use dust_core::cpu::psr::Mode;
use imgui::{StyleColor, StyleVar, Ui};

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

pub fn psr_mode_to_str(mode: Mode) -> &'static str {
    match mode {
        Mode::User => "User",
        Mode::Fiq => "Fiq",
        Mode::Irq => "Irq",
        Mode::Supervisor => "Supervisor",
        Mode::Abort => "Abort",
        Mode::Undefined => "Undefined",
        Mode::System => "System",
    }
}

pub fn separator_with_width(ui: &Ui, width: f32) {
    let color = ui.style_color(StyleColor::Separator);
    let prev_cursor_pos = ui.cursor_pos();
    let window_pos = ui.window_pos();
    let left = [
        window_pos[0] + prev_cursor_pos[0],
        window_pos[1] + prev_cursor_pos[1] - ui.scroll_y(),
    ];
    let right = [
        left[0]
            + if width > 0.0 {
                width
            } else {
                ui.content_region_avail()[0] + width
            },
        left[1],
    ];
    ui.get_window_draw_list()
        .add_line(left, right, color)
        .build();
    ui.dummy([0.0, 0.0]);
}

pub fn layout_group(ui: &Ui, height: f32, bg_color: Option<[f32; 4]>, f: impl FnOnce(f32)) {
    let (child_rounding, window_padding) = unsafe {
        let style = ui.style();
        (style.child_rounding, style.window_padding)
    };

    let prev_cursor_pos = ui.cursor_pos();
    ui.set_cursor_pos([
        prev_cursor_pos[0] + window_padding[0],
        prev_cursor_pos[1] + window_padding[1],
    ]);

    if let Some(bg_color) = bg_color {
        let window_pos = ui.window_pos();
        let upper_left = [
            window_pos[0] + prev_cursor_pos[0],
            window_pos[1] - ui.scroll_y() + prev_cursor_pos[1],
        ];
        let lower_right = [
            window_pos[0] + ui.content_region_max()[0],
            upper_left[1] + height + 2.0 * window_padding[1],
        ];
        ui.get_window_draw_list()
            .add_rect(upper_left, lower_right, bg_color)
            .filled(true)
            .rounding(child_rounding)
            .build();
    }

    ui.group(|| f(window_padding[0]));

    let _item_spacing = ui.push_style_var(StyleVar::ItemSpacing([0.0; 2]));
    ui.dummy([0.0, window_padding[1]]);
}
