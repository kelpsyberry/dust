macro_rules! str_buf {
    ($buf: expr, $($args: tt)*) => {{
        use std::fmt::Write;
        $buf.clear();
        let _ = write!($buf, $($args)*);
        &$buf
    }};
}

mod range_inclusive;
pub use range_inclusive::RangeInclusive;
pub mod disasm;
pub mod regs;
mod scrollbar;
use scrollbar::Scrollbar;
mod y_pos;

use crate::ui::utils::sub2s;
use dust_core::cpu::psr::Mode;
use imgui::{StyleColor, StyleVar, Ui};

pub fn rgb5_to_rgba8(value: u16) -> u32 {
    let value = value as u32;
    let rgb6_8 = (value << 1 & 0x3E) | (value << 4 & 0x3E00) | (value << 7 & 0x3F_0000);
    0xFF00_0000 | rgb6_8 << 2 | (rgb6_8 >> 4 & 0x0003_0303)
}

pub fn rgb5_to_rgba32f(value: u16) -> [f32; 4] {
    let [r, g, b] =
        [value & 0x1F, value >> 5 & 0x1F, value >> 10 & 0x1F].map(|v| v as f32 * (1.0 / 31.0));
    [r, g, b, 1.0]
}

pub fn rgb5_to_rgb32f(value: u16) -> [f32; 3] {
    [value & 0x1F, value >> 5 & 0x1F, value >> 10 & 0x1F].map(|v| v as f32 / 31.0)
}

pub fn rgb32f_to_rgb5(value: [f32; 3]) -> u16 {
    let [r, g, b] = value.map(|v| ((v * 31.0) as u16).min(31));
    r | g << 5 | b << 10
}

pub fn psr_mode_to_str(mode: Mode) -> &'static str {
    match mode {
        Mode::USER => "User",
        Mode::FIQ => "Fiq",
        Mode::IRQ => "Irq",
        Mode::SUPERVISOR => "Supervisor",
        Mode::ABORT => "Abort",
        Mode::UNDEFINED => "Undefined",
        Mode::SYSTEM => "System",
        _ => "Invalid",
    }
}

pub fn separator_with_width(ui: &Ui, width: f32) {
    let thickness = 1.0;
    let half_thickness = thickness * 0.5;

    let color = ui.style_color(StyleColor::Separator);
    let prev_cursor_pos = ui.cursor_pos();
    let window_pos = ui.window_pos();
    let left = sub2s(
        [
            window_pos[0] + prev_cursor_pos[0],
            window_pos[1] + prev_cursor_pos[1] - ui.scroll_y(),
        ],
        half_thickness,
    );
    let right = sub2s(
        [
            left[0]
                + if width > 0.0 {
                    width
                } else {
                    ui.content_region_avail()[0] + width
                },
            left[1],
        ],
        half_thickness,
    );
    ui.get_window_draw_list()
        .add_line(left, right, color)
        .thickness(thickness)
        .build();
    ui.dummy([0.0, 0.0]);
}

pub fn layout_group(ui: &Ui, height: f32, bg_color: Option<[f32; 4]>, f: impl FnOnce(f32)) {
    let window_padding = style!(ui, window_padding);

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
            .rounding(style!(ui, child_rounding))
            .build();
    }

    ui.group(|| f(window_padding[0]));

    let _item_spacing = ui.push_style_var(StyleVar::ItemSpacing([0.0; 2]));
    ui.dummy([0.0, window_padding[1]]);
}

macro_rules! selectable_value {
    ($ui: expr, $name: literal, $width_str: expr, $($fmt: tt)*) => {
        $ui.align_text_to_frame_padding();
        $ui.text(concat!($name, ": "));
        $ui.same_line();
        $ui.set_next_item_width(
            style!($ui, frame_padding)[0] * 2.0 + $ui.calc_text_size($width_str)[0],
        );
        $ui.input_text(
            concat!("##", $name),
            &mut format!($($fmt)*),
        )
        .read_only(true)
        .build();
    }
}

pub fn format_size(size: u32) -> String {
    let log1024 = 31_u32.saturating_sub(size.leading_zeros()) / 10;
    let unit = ["B", "KiB", "MiB", "GiB"][log1024 as usize];
    let amount = size as f64 / (1 << (log1024 * 10)) as f64;
    format!("{amount:.2} {unit}")
}

pub fn format_size_u64(size: u64) -> String {
    let log1024 = 63_u32.saturating_sub(size.leading_zeros()) / 10;
    let unit = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"][log1024 as usize];
    let amount = size as f64 / (1 << (log1024 * 10)) as f64;
    format!("{amount:.2} {unit}")
}

pub fn format_size_shift(shift: usize) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB"];
    if let Some(unit) = units.get(shift / 10) {
        format!("{} {unit}, 2^{shift} B", 1 << (shift % 10))
    } else {
        format!("2^{shift} B")
    }
}
