use crate::ui::utils::add2;
use imgui::{StyleColor, Ui};

pub fn regs_32_default_label(i: usize, max_digits: u32) -> String {
    format!(
        "r{i}: {:<len$}",
        "",
        len = (max_digits - i.max(1).ilog10()) as usize
    )
}

pub fn regs_32(
    ui: &Ui,
    start_i: usize,
    values: &[u32],
    mut change: impl FnMut(usize, u32),
    mut label: impl FnMut(usize, u32) -> String,
    mut f: impl FnMut(usize),
) {
    let reg_32_bit_width = style!(ui, frame_padding)[0] * 2.0 + ui.calc_text_size("00000000")[0];
    let max_digits = (start_i + values.len()).max(1).ilog10();
    let mut i = start_i;
    for value in values {
        f(i);
        ui.align_text_to_frame_padding();
        ui.text(label(i, max_digits));
        ui.same_line();
        ui.set_next_item_width(reg_32_bit_width);
        let mut buffer = format!("{value:08X}");
        if ui
            .input_text(&format!("##r{i}"), &mut buffer)
            .enter_returns_true(true)
            .auto_select_all(true)
            .chars_hexadecimal(true)
            .build()
        {
            if let Ok(new_value) = u32::from_str_radix(buffer.as_str(), 16) {
                change(i, new_value);
            }
        }
        i += 1;
    }
}

pub enum BitfieldCommand<'a> {
    Field(&'a str, u32),
    Skip(u32),
    Callback(fn(&Ui)),
    CallbackName(fn(&Ui)),
    CallbackValue(fn(&Ui)),
}

pub fn bitfield(
    ui: &Ui,
    spacing: f32,
    exact_sizing: bool,
    show_skip_values: bool,
    value: u32,
    cmds: &[BitfieldCommand],
) {
    let mut field_widths: Vec<f32> = vec![];
    let mut total_bits = 0;
    {
        let frame_padding = 2.0 * style!(ui, frame_padding)[0];
        for cmd in cmds.iter().rev() {
            match cmd {
                BitfieldCommand::Field(name, bits) => {
                    let name_width = ui.calc_text_size(name)[0];
                    let bit_value_width =
                        ui.calc_text_size(format!("{:0bits$}", 0, bits = *bits as usize))[0];
                    field_widths.push(if exact_sizing {
                        (name_width + frame_padding).max(
                            bit_value_width
                                + *bits as f32 * frame_padding
                                + (*bits - 1) as f32 * spacing,
                        )
                    } else {
                        name_width.max(bit_value_width) + frame_padding
                    });
                    total_bits += *bits;
                }
                BitfieldCommand::Skip(bits) => {
                    let bit_value_width =
                        ui.calc_text_size(format!("{:0bits$}", 0, bits = *bits as usize))[0];
                    field_widths.push(if exact_sizing {
                        bit_value_width
                            + *bits as f32 * frame_padding
                            + (*bits - 1) as f32 * spacing
                    } else {
                        bit_value_width + frame_padding
                    });
                    total_bits += *bits;
                }
                _ => {}
            }
        }
    }

    let mut field_i = 0;
    let text_color = ui.style_color(StyleColor::Text);
    let text_disabled_color = ui.style_color(StyleColor::TextDisabled);
    let field_bg_color = ui.style_color(StyleColor::FrameBg);
    let skip_bg_color = [
        field_bg_color[0] * 0.5,
        field_bg_color[1] * 0.5,
        field_bg_color[2] * 0.5,
        field_bg_color[3],
    ];
    let mut window_pos = ui.window_pos();
    window_pos[1] -= ui.scroll_y();
    let mut cursor_pos = ui.cursor_pos();

    let mut cur_bit = total_bits;
    for cmd in cmds.iter().rev() {
        macro_rules! show_field {
            ($bits: expr, $show_value: expr, $text_color: expr, $bg_color: expr) => {
                let field_width = field_widths[field_i];
                let upper_left = add2(window_pos, cursor_pos);
                let draw_list = ui.get_window_draw_list();
                draw_list
                    .add_rect(
                        upper_left,
                        [
                            upper_left[0] + field_width,
                            upper_left[1] + ui.frame_height(),
                        ],
                        $bg_color,
                    )
                    .filled(true)
                    .rounding(style!(ui, frame_rounding))
                    .build();
                cur_bit -= $bits;
                if $show_value {
                    let text = format!(
                        "{:0bits$b}",
                        value >> cur_bit & ((1 << $bits) - 1),
                        bits = $bits as usize
                    );
                    let text_width = ui.calc_text_size(&text)[0];
                    draw_list.add_text(
                        [
                            upper_left[0] + 0.5 * (field_width - text_width),
                            upper_left[1] + style!(ui, frame_padding)[1],
                        ],
                        $text_color,
                        &text,
                    );
                }
                cursor_pos[0] += field_width + spacing;
                field_i += 1;
            };
        }
        match cmd {
            BitfieldCommand::Field(_, bits) => {
                show_field!(*bits, true, text_color, field_bg_color);
            }
            BitfieldCommand::Skip(bits) => {
                show_field!(*bits, show_skip_values, text_disabled_color, skip_bg_color);
            }
            BitfieldCommand::Callback(f) => f(ui),
            BitfieldCommand::CallbackName(f) => f(ui),
            _ => {}
        }
    }

    ui.dummy([cursor_pos[0] - spacing, ui.frame_height()]);

    field_i = 0;
    let mut next_spacing = 0.0;
    for cmd in cmds.iter().rev() {
        match cmd {
            BitfieldCommand::Field(name, _) => {
                let text_width = ui.calc_text_size(name)[0];
                let text_padding = (field_widths[field_i] - text_width) * 0.5;
                if field_i == 0 {
                    ui.dummy([0.0; 2]);
                }
                ui.same_line_with_spacing(0.0, next_spacing + text_padding);
                ui.text(name);
                next_spacing = spacing + text_padding;
                field_i += 1;
            }
            BitfieldCommand::Skip(_) => {
                if field_i == 0 {
                    ui.dummy([0.0; 2]);
                }
                next_spacing += spacing + field_widths[field_i];
                field_i += 1;
            }
            BitfieldCommand::Callback(f) => f(ui),
            BitfieldCommand::CallbackValue(f) => f(ui),
            _ => {}
        }
    }
}
