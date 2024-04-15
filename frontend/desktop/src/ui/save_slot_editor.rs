use super::EmuState;
use crate::{
    config::{saves, Config, Setting},
    emu::SavePathUpdate,
};
use imgui::Ui;

pub(super) struct Editor {
    editing_i: Option<usize>,
}

impl Editor {
    pub fn new() -> Self {
        Editor { editing_i: None }
    }

    pub fn draw(&mut self, ui: &Ui, config: &mut Config, emu_state: &mut Option<EmuState>) {
        let mut shown = false;
        ui.menu_with_enabled(
            "\u{f0c7} Save slot",
            emu_state.as_ref().map_or(false, |emu| emu.game_loaded),
            || {
                shown = true;

                let emu_state = emu_state.as_mut().unwrap();

                if let Some(path_config) = config!(config, &save_path_config) {
                    macro_rules! update_path_config {
                        (|$path_config: ident| $inner: expr) => {
                            config.save_path_config.inner_mut().update(|path_config| {
                                let $path_config = path_config.as_mut().unwrap();
                                $inner
                            });
                        };
                    }

                    let save_dir = &config!(config, &save_dir_path).0;
                    let game_title = &emu_state.title;

                    if let saves::Slots::Multiple { current, slots } = &path_config.slots {
                        let mut text_width = ui.text_line_height() * 6.0;
                        for slot in slots {
                            text_width = ui.calc_text_size(slot)[0].max(text_width);
                        }
                        let two_frame_padding = style!(ui, frame_padding)[0] * 2.0;
                        text_width += two_frame_padding;
                        let line_width = text_width
                            + style!(ui, item_spacing)[0]
                            + two_frame_padding
                            + ui.calc_text_size("-")[0];
                        ui.dummy([line_width, 0.0]);

                        let mut switch = None;
                        let mut remove = None;
                        let mut rename = None;

                        let current = *current;

                        for (i, slot) in slots.iter().enumerate() {
                            let _id = ui.push_id_usize(i);

                            if Some(i) == self.editing_i {
                                let mut buffer = slot.clone();
                                ui.set_keyboard_focus_here();
                                ui.set_next_item_width(line_width);
                                if ui
                                    .input_text("", &mut buffer)
                                    .auto_select_all(true)
                                    .enter_returns_true(true)
                                    .build()
                                {
                                    self.editing_i = None;
                                    rename = Some((i, buffer));
                                }
                            } else {
                                let color = (Some(i) == current).then(|| {
                                    ui.push_style_color(
                                        imgui::StyleColor::Button,
                                        ui.style_color(imgui::StyleColor::ButtonActive),
                                    )
                                });
                                if ui.button_with_size(slot, [text_width, 0.0]) {
                                    switch = Some(i);
                                } else if ui.is_item_clicked_with_button(imgui::MouseButton::Right)
                                {
                                    self.editing_i = Some(i);
                                }
                                drop(color);

                                ui.same_line();
                                if ui.button("-") {
                                    remove = Some(i);
                                }
                            }
                        }

                        if self.editing_i == Some(slots.len()) {
                            let mut buffer = String::new();
                            ui.set_keyboard_focus_here();
                            ui.set_next_item_width(line_width);
                            if ui
                                .input_text("##new", &mut buffer)
                                .auto_select_all(true)
                                .enter_returns_true(true)
                                .build()
                            {
                                self.editing_i = None;
                                update_path_config!(|path_config| {
                                    path_config.create_slot(buffer);
                                });
                            }
                        } else if ui.button("+") {
                            self.editing_i = Some(slots.len());
                        }

                        if let Some(i) = switch {
                            update_path_config!(|path_config| {
                                if path_config.switch_slot(i) {
                                    let new_path = path_config.path(save_dir, game_title).unwrap();
                                    emu_state.save_path_update = Some(SavePathUpdate {
                                        new: Some(new_path),
                                        new_prev: None,
                                        reload: true,
                                        reset: config!(config, reset_on_save_slot_switch),
                                    });
                                }
                            });
                        } else if let Some(i) = remove {
                            update_path_config!(|path_config| {
                                path_config.remove_slot(i, save_dir, game_title);
                            });
                        } else if let Some((i, name)) = rename {
                            update_path_config!(|path_config| {
                                path_config.rename_slot(i, name, save_dir, game_title);
                            });
                        }
                    } else if ui.button("Make multi-slot") {
                        update_path_config!(|path_config| {
                            path_config.make_multi_slot();
                        });
                    }
                }
            },
        );

        if !shown {
            self.editing_i = None;
        }
    }
}
