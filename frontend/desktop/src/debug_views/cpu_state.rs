use super::{
    common::{
        layout_group, psr_mode_to_str,
        regs::{bitfield, regs_32, regs_32_default_label, BitfieldCommand},
        separator_with_width,
    },
    BaseView, FrameDataSlot, FrameView, FrameViewMessages, SingletonView,
};
use crate::ui::{utils::combo_value, window::Window};
use dust_core::{
    cpu::{
        arm7::Arm7,
        arm9::Arm9,
        psr::{Bank, Psr},
        Engine, Regs,
    },
    emu::Emu,
};
use imgui::{StyleColor, StyleVar, TableFlags};

mod bounded {
    use dust_core::utils::bounded_int_lit;
    bounded_int_lit!(pub struct RegIndex(u8), max 15);
}
pub use bounded::*;

pub struct EmuState<const ARM9: bool>;

impl<const ARM9: bool> super::FrameViewEmuState for EmuState<ARM9> {
    type InitData = ();
    type Message = (Bank, RegIndex, u32);
    type FrameData = (Regs, Psr);

    fn new<E: Engine>(_data: Self::InitData, _visible: bool, _emu: &mut Emu<E>) -> Self {
        EmuState
    }

    fn handle_message<E: Engine>(
        &mut self,
        (selected_bank, i, value): Self::Message,
        emu: &mut Emu<E>,
    ) {
        let (mut regs, cpsr) = if ARM9 {
            (emu.arm9.regs(), emu.arm9.cpsr())
        } else {
            (emu.arm7.regs(), emu.arm7.cpsr())
        };

        let cpu_reg_bank = cpsr.mode().reg_bank();

        let i = i.get() as usize;
        match i {
            0..=7 | 15 => regs.gprs[i] = value,

            8..=12 => {
                if (cpu_reg_bank == Bank::Fiq) == (selected_bank == Bank::Fiq) {
                    regs.gprs[i] = value;
                } else {
                    regs.r8_14_fiq[i] = value;
                }
            }

            _ => {
                if cpu_reg_bank == selected_bank {
                    regs.gprs[i] = value;
                } else {
                    match selected_bank {
                        Bank::System => regs.r13_14_sys[i] = value,
                        Bank::Fiq => regs.r8_14_fiq[5 + i] = value,
                        Bank::Irq => regs.r13_14_irq[i] = value,
                        Bank::Supervisor => regs.r13_14_svc[i] = value,
                        Bank::Abort => regs.r13_14_abt[i] = value,
                        Bank::Undefined => regs.r13_14_und[i] = value,
                    }
                }
            }
        }

        if ARM9 {
            Arm9::set_regs(emu, &regs);
        } else {
            Arm7::set_regs(emu, &regs);
        }
    }

    fn prepare_frame_data<'a, E: Engine, S: FrameDataSlot<'a, Self::FrameData>>(
        &mut self,
        emu: &mut Emu<E>,
        frame_data: S,
    ) {
        frame_data.insert(if ARM9 {
            (emu.arm9.regs(), emu.arm9.cpsr())
        } else {
            (emu.arm7.regs(), emu.arm7.cpsr())
        });
    }
}

pub struct CpuState<const ARM9: bool> {
    reg_values: Option<(Regs, Psr)>,
    reg_bank: Option<Bank>,
}

impl<const ARM9: bool> SingletonView for CpuState<ARM9> {
    fn window<'ui>(
        &mut self,
        ui: &'ui imgui::Ui,
    ) -> imgui::Window<'ui, 'ui, impl AsRef<str> + 'static> {
        ui.window(Self::MENU_NAME).always_auto_resize(true)
    }
    fn window_stopped(ui: &'_ imgui::Ui) -> imgui::Window<'_, '_, impl AsRef<str> + 'static> {
        ui.window(Self::MENU_NAME).always_auto_resize(true)
    }
}

impl<const ARM9: bool> BaseView for CpuState<ARM9> {
    const MENU_NAME: &'static str = if ARM9 { "ARM9 state" } else { "ARM7 state" };
}

impl<const ARM9: bool> FrameView for CpuState<ARM9> {
    type EmuState = EmuState<ARM9>;

    fn new(_window: &mut Window) -> Self {
        CpuState {
            reg_values: None,
            reg_bank: None,
        }
    }

    fn emu_state(&self) -> <Self::EmuState as super::FrameViewEmuState>::InitData {}

    fn update_from_frame_data(
        &mut self,
        frame_data: &<Self::EmuState as super::FrameViewEmuState>::FrameData,
        _window: &mut Window,
    ) {
        self.reg_values = Some(frame_data.clone());
    }

    fn draw(
        &mut self,
        ui: &imgui::Ui,
        window: &mut Window,
        mut messages: impl FrameViewMessages<Self>,
    ) {
        if let Some((reg_values, cpsr)) = self.reg_values.as_mut() {
            let _mono_font_token = ui.push_font(window.imgui.mono_font);
            let _item_spacing =
                ui.push_style_var(StyleVar::ItemSpacing([0.0, style!(ui, item_spacing)[1]]));
            let _table_border = ui.push_style_color(
                StyleColor::TableBorderLight,
                ui.style_color(StyleColor::Border),
            );

            let mode = cpsr.mode();
            let cpu_reg_bank = mode.reg_bank();
            let cpu_spsr_bank = mode.spsr_bank();

            if let Some(_table_token) = ui.begin_table_with_flags(
                "regs",
                4,
                TableFlags::BORDERS_INNER_V | TableFlags::SIZING_FIXED_FIT | TableFlags::NO_CLIP,
            ) {
                regs_32(
                    ui,
                    0,
                    &reg_values.gprs,
                    |i, value| {
                        messages.push((cpu_reg_bank, RegIndex::new(i as u8), value));
                    },
                    regs_32_default_label,
                    |i| {
                        if i & 3 == 0 {
                            ui.table_next_column();
                        }
                    },
                );
            }

            ui.separator();

            let psr_fields: &'static [BitfieldCommand] = if ARM9 {
                &[
                    BitfieldCommand::Field("Mode", 5),
                    BitfieldCommand::Field("T", 1),
                    BitfieldCommand::Field("F", 1),
                    BitfieldCommand::Field("I", 1),
                    BitfieldCommand::Skip(19),
                    BitfieldCommand::Field("Q", 1),
                    BitfieldCommand::Field("V", 1),
                    BitfieldCommand::Field("C", 1),
                    BitfieldCommand::Field("Z", 1),
                    BitfieldCommand::Field("N", 1),
                ]
            } else {
                &[
                    BitfieldCommand::Field("Mode", 5),
                    BitfieldCommand::Field("T", 1),
                    BitfieldCommand::Field("F", 1),
                    BitfieldCommand::Field("I", 1),
                    BitfieldCommand::Skip(20),
                    BitfieldCommand::Field("V", 1),
                    BitfieldCommand::Field("C", 1),
                    BitfieldCommand::Field("Z", 1),
                    BitfieldCommand::Field("N", 1),
                ]
            };

            ui.text("CPSR:");
            {
                let _frame_rounding = ui.push_style_var(StyleVar::FrameRounding(0.0));
                bitfield(ui, 2.0, false, true, cpsr.raw(), psr_fields);
            }

            ui.text(format!("Mode: {}", psr_mode_to_str(mode)));

            ui.separator();

            ui.text("SPSR: ");
            if mode.has_spsr() {
                {
                    let _frame_rounding = ui.push_style_var(StyleVar::FrameRounding(0.0));
                    bitfield(ui, 2.0, false, true, reg_values.spsr.raw(), psr_fields);
                }

                ui.text(format!("Mode: {}", psr_mode_to_str(reg_values.spsr.mode()),));
            } else {
                ui.same_line();
                ui.text("None");
            }

            ui.separator();

            static REG_BANKS: [Option<Bank>; 7] = [
                None,
                Some(Bank::System),
                Some(Bank::Fiq),
                Some(Bank::Irq),
                Some(Bank::Supervisor),
                Some(Bank::Abort),
                Some(Bank::Undefined),
            ];

            ui.align_text_to_frame_padding();
            ui.text("Banked registers: ");
            ui.same_line();
            combo_value(
                ui,
                "##reg_bank",
                &mut self.reg_bank,
                &REG_BANKS,
                |reg_bank| {
                    if let Some(reg_bank) = reg_bank {
                        let mut label = match reg_bank {
                            Bank::System => "System",
                            Bank::Fiq => "Fiq",
                            Bank::Irq => "Irq",
                            Bank::Supervisor => "Supervisor",
                            Bank::Abort => "Abort",
                            Bank::Undefined => "Undefined",
                        }
                        .to_owned();
                        let same_reg = *reg_bank == cpu_reg_bank;
                        let same_spsr = *reg_bank == cpu_spsr_bank;
                        label.push_str(match (same_reg, same_spsr) {
                            (false, false) => "",
                            (true, false) => " (current regs)",
                            (false, true) => " (current SPSR)",
                            (true, true) => " (current)",
                        });
                        label.into()
                    } else {
                        "None".into()
                    }
                },
            );

            if let Some(reg_bank) = self.reg_bank {
                let mut child_bg_color = ui.style_color(StyleColor::WindowBg);
                for component in &mut child_bg_color[..3] {
                    *component += (0.5 - *component) * 0.33;
                }
                child_bg_color[3] *= 0.33;

                let cell_padding_y = style!(ui, cell_padding)[1];
                let item_spacing_y = style!(ui, item_spacing)[1];

                let mut child_height = 3.0 * ui.frame_height_with_spacing() + 2.0 * cell_padding_y;
                if reg_bank != Bank::System {
                    child_height += 2.0 * item_spacing_y + ui.text_line_height();
                    if reg_bank != cpu_spsr_bank {
                        child_height +=
                            ui.frame_height() + 2.0 * ui.text_line_height() + 3.0 * item_spacing_y;
                    }
                }

                layout_group(ui, child_height, Some(child_bg_color), |window_padding_x| {
                    let (bank_str, r8_r12, r13_14, spsr) = match reg_bank {
                        Bank::System => (
                            "sys",
                            &reg_values.r8_12_other[..],
                            &reg_values.r13_14_sys[..],
                            None,
                        ),
                        Bank::Fiq => (
                            "fiq",
                            &reg_values.r8_14_fiq[..5],
                            &reg_values.r8_14_fiq[5..],
                            Some(reg_values.spsr_fiq),
                        ),
                        Bank::Irq => (
                            "irq",
                            &reg_values.r8_12_other[..],
                            &reg_values.r13_14_irq[..],
                            Some(reg_values.spsr_irq),
                        ),
                        Bank::Supervisor => (
                            "svc",
                            &reg_values.r8_12_other[..],
                            &reg_values.r13_14_svc[..],
                            Some(reg_values.spsr_svc),
                        ),
                        Bank::Abort => (
                            "abt",
                            &reg_values.r8_12_other[..],
                            &reg_values.r13_14_abt[..],
                            Some(reg_values.spsr_abt),
                        ),
                        Bank::Undefined => (
                            "und",
                            &reg_values.r8_12_other[..],
                            &reg_values.r13_14_und[..],
                            Some(reg_values.spsr_und),
                        ),
                    };

                    if let Some(_table_token) = ui.begin_table_with_flags(
                        "banked_regs",
                        3,
                        TableFlags::BORDERS_INNER_V
                            | TableFlags::NO_CLIP
                            | TableFlags::SIZING_STRETCH_PROP,
                    ) {
                        let regs_32_label = |i: usize, max_digits| {
                            format!(
                                "r{i}_{bank_str}: {:<len$}",
                                "",
                                len = (max_digits - i.ilog10()) as usize
                            )
                        };
                        if (reg_bank != Bank::Fiq && cpu_reg_bank != Bank::Fiq)
                            || reg_bank == cpu_reg_bank
                        {
                            ui.table_next_column();
                            for i in 8..13 {
                                if i == 11 {
                                    ui.table_next_column();
                                }
                                ui.align_text_to_frame_padding();
                                ui.text(format!(
                                    "r{i}_{bank_str}: {}",
                                    if i < 10 { " " } else { "" }
                                ));
                                ui.same_line();
                                ui.text("<cur>");
                            }
                        } else {
                            regs_32(
                                ui,
                                8,
                                r8_r12,
                                |i, value| {
                                    messages.push((reg_bank, RegIndex::new(i as u8), value));
                                },
                                regs_32_label,
                                |i| {
                                    if (i - 8) % 3 == 0 {
                                        ui.table_next_column();
                                    }
                                },
                            );
                        }
                        ui.table_next_column();
                        if reg_bank == cpu_reg_bank {
                            for i in 13..15 {
                                ui.align_text_to_frame_padding();
                                ui.text(format!("r{i}_{bank_str}: {:<1}", ""));
                                ui.same_line();
                                ui.text("<cur>");
                                ui.same_line();
                                ui.dummy([window_padding_x, 0.0]);
                            }
                        } else {
                            regs_32(
                                ui,
                                13,
                                r13_14,
                                |i, value| {
                                    messages.push((reg_bank, RegIndex::new(i as u8), value));
                                },
                                regs_32_label,
                                |i| {
                                    if i == 14 {
                                        ui.same_line();
                                        ui.dummy([window_padding_x, 0.0]);
                                    }
                                },
                            );
                            ui.same_line();
                            ui.dummy([window_padding_x, 0.0]);
                        }
                    }

                    if let Some(spsr) = spsr {
                        separator_with_width(ui, -window_padding_x);

                        ui.text(format!("SPSR_{bank_str}:"));
                        if reg_bank == cpu_spsr_bank {
                            ui.same_line();
                            ui.text("<current>");
                        } else {
                            {
                                let _frame_rounding =
                                    ui.push_style_var(StyleVar::FrameRounding(0.0));
                                bitfield(ui, 2.0, false, true, spsr.raw(), psr_fields);
                            }

                            ui.text(format!("Mode: {}", psr_mode_to_str(spsr.mode())));
                        }
                    }
                });
            }
        }
    }
}
