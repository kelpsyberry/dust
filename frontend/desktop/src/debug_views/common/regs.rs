use imgui::Ui;

pub fn regs_32(ui: &Ui, values: &[u32]) {
    let style = ui.clone_style();
    let reg_32_bit_width = style.frame_padding[0] * 2.0 + ui.calc_text_size("00000000")[0];
    let max_digits = values.len().log10();
    for (i, &value) in values.iter().enumerate() {
        ui.align_text_to_frame_padding();
        ui.text(&format!(
            "r{}: {:<len$}",
            i,
            "",
            len = (max_digits - i.log10()) as usize
        ));
        ui.same_line();
        ui.set_next_item_width(reg_32_bit_width);
        ui.input_text(&format!("##r{}", i), &mut format!("{:08X}", value))
            .read_only(true)
            .build();
    }
}

pub enum BitfieldCommand<'a> {
    Field(&'a str, u32),
    Callback(fn(&Ui)),
    CallbackName(fn(&Ui)),
    CallbackValue(fn(&Ui)),
}

pub fn bitfield(ui: &Ui, ident: &str, spacing: f32, value: usize, cmds: &[BitfieldCommand]) {
    let mut field_widths: Vec<f32> = vec![];
    let mut total_bits = 0;
    {
        let bit_padding = ui.clone_style().frame_padding[0];
        let bit_value_width = ui.calc_text_size("0")[0];
        for cmd in cmds.iter().rev() {
            if let BitfieldCommand::Field(name, bits) = cmd {
                field_widths.push(
                    ui.calc_text_size(name)[0].max(*bits as f32 * bit_value_width)
                        + 2.0 * bit_padding,
                );
                total_bits += *bits;
            }
        }
    }

    let mut first = true;
    let mut field_i = 0;

    let mut cur_bit = total_bits;
    for cmd in cmds.iter().rev() {
        match cmd {
            BitfieldCommand::Field(_, bits) => {
                if !first {
                    ui.same_line_with_spacing(0.0, spacing);
                }
                first = false;
                cur_bit -= *bits;
                ui.set_next_item_width(field_widths[field_i]);
                ui.input_text(
                    &format!("##{}_{}", ident, field_i),
                    &mut format!("{}", value >> cur_bit & ((1 << *bits) - 1)),
                )
                .read_only(true)
                .build();
                field_i += 1;
            }
            BitfieldCommand::Callback(f) => f(ui),
            BitfieldCommand::CallbackName(f) => f(ui),
            _ => {}
        }
    }

    first = true;
    field_i = 0;
    for cmd in cmds.iter().rev() {
        match cmd {
            BitfieldCommand::Field(name, _) => {
                let width = ui.calc_text_size(name)[0];
                let padding = (field_widths[field_i] - width) * 0.5;
                field_i += 1;
                if first {
                    ui.dummy([0.0; 2]);
                    ui.same_line_with_spacing(0.0, padding);
                } else {
                    ui.same_line_with_spacing(0.0, spacing + 2.0 * padding);
                }
                first = false;
                ui.text(name);
            }
            BitfieldCommand::Callback(f) => f(ui),
            BitfieldCommand::CallbackValue(f) => f(ui),
            _ => {}
        }
    }
}
