use super::SettingsData;
use crate::input::{
    trigger::{self, Trigger},
    Action, Map, PressedKey,
};
use crate::{
    config::{self, Config, Setting},
    ui::utils::heading,
};
use ahash::AHashSet as HashSet;
use dust_core::emu::input::Keys;
use imgui::{
    ItemHoveredFlags, MouseButton, StyleColor, TableColumnFlags, TableColumnSetup, TableFlags, Ui,
};
use rfd::FileDialog;
use winit::event::{ElementState, Event, WindowEvent};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Selection {
    Keypad(Keys),
    Hotkey(Action),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    None,
    Capturing {
        selection: (Selection, bool),
        needs_focus: bool,
    },
    ManuallyChanging {
        selection: (Selection, bool),
        needs_focus: bool,
    },
}

impl State {
    fn selection(self) -> Option<(Selection, bool)> {
        match self {
            State::Capturing { selection, .. } | State::ManuallyChanging { selection, .. } => {
                Some(selection)
            }
            _ => None,
        }
    }

    fn is_capturing(&self) -> bool {
        matches!(self, State::Capturing { .. })
    }

    fn is_manually_changing(&self) -> bool {
        matches!(self, State::ManuallyChanging { .. })
    }

    fn drain_needs_focus(&mut self) -> bool {
        match self {
            State::Capturing { needs_focus, .. } | State::ManuallyChanging { needs_focus, .. } => {
                let result = *needs_focus;
                *needs_focus = false;
                result
            }
            _ => false,
        }
    }
}

pub struct Editor {
    state: State,
    pressed_keys: HashSet<PressedKey>,
    manual_change_buffer: String,
    current_trigger: Option<Trigger>,
}

static KEYS: &[(Keys, &str)] = &[
    (Keys::A, "A"),
    (Keys::B, "B"),
    (Keys::X, "X"),
    (Keys::Y, "Y"),
    (Keys::L, "L"),
    (Keys::R, "R"),
    (Keys::START, "Start"),
    (Keys::SELECT, "Select"),
    (Keys::RIGHT, "Right"),
    (Keys::LEFT, "Left"),
    (Keys::UP, "Up"),
    (Keys::DOWN, "Down"),
    (Keys::DEBUG, "Debug"),
];

static ACTIONS: &[(Action, &str)] = &[
    (Action::PlayPause, "Play/pause"),
    (Action::Reset, "Reset"),
    (Action::Stop, "Stop"),
    (Action::ToggleFramerateLimit, "Toggle framerate limit"),
    (Action::ToggleSyncToAudio, "Toggle sync to audio"),
    (Action::ToggleFullWindowScreen, "Toggle full-window screen"),
];

impl Editor {
    pub fn new() -> Self {
        Editor {
            state: State::None,
            pressed_keys: HashSet::new(),
            manual_change_buffer: String::new(),
            current_trigger: None,
        }
    }

    pub fn emu_stopped(&mut self) {
        if matches!(self.state.selection(), Some((_, true))) {
            self.state = State::None;
        }
    }

    fn finalize(&mut self, config: &mut Config) {
        if let Some((trigger, (selection, game))) = match self.state {
            State::None => None,
            State::Capturing { selection, .. } => self
                .current_trigger
                .take()
                .map(|trigger| (Some(trigger), selection)),
            State::ManuallyChanging { selection, .. } => {
                Trigger::option_from_str(&self.manual_change_buffer)
                    .ok()
                    .map(|trigger| (trigger, selection))
            }
        } {
            config.input_map.update(|inner| {
                let update = |map: &mut Map| match selection {
                    Selection::Keypad(key) => {
                        map.keypad.insert(key, trigger);
                    }
                    Selection::Hotkey(action) => {
                        map.hotkeys.insert(action, trigger);
                    }
                };
                if game {
                    inner.update_game(update);
                } else {
                    inner.update_global(update);
                }
            });
        }
        self.state = State::None;
    }

    fn draw_input_button(
        &mut self,
        trigger: Option<Trigger>,
        selection: (Selection, bool),
        ui: &Ui,
        config: &mut Config,
        width: f32,
    ) {
        let _button_color = (!self.state.is_manually_changing()
            && trigger
                .as_ref()
                .map_or(false, |trigger| trigger.activated(&self.pressed_keys)))
        .then(|| {
            ui.push_style_color(
                StyleColor::Button,
                ui.style_color(StyleColor::ButtonHovered),
            )
        });

        if self.state.selection() == Some(selection) {
            if self.state.drain_needs_focus() {
                ui.set_keyboard_focus_here();
            }

            ui.set_next_item_width(width);
            let finished = if self.state.is_manually_changing() {
                ui.input_text("", &mut self.manual_change_buffer)
                    .enter_returns_true(true)
                    .build()
            } else {
                ui.input_text(
                    "",
                    &mut self
                        .current_trigger
                        .as_ref()
                        .map_or_else(String::new, Trigger::to_string),
                )
                .read_only(true)
                .build();
                ui.is_item_deactivated()
            };

            if finished {
                self.finalize(config);
            }
        } else {
            let trigger_string = trigger.map(|trigger| trigger.to_string());
            if ui.button_with_size(
                format!("{}###", trigger_string.as_deref().unwrap_or("\u{f00d}")),
                [width, 0.0],
            ) {
                self.finalize(config);
                self.state = State::Capturing {
                    selection,
                    needs_focus: true,
                };
            } else if ui.is_item_clicked_with_button(MouseButton::Right) {
                self.finalize(config);
                self.state = State::ManuallyChanging {
                    selection,
                    needs_focus: true,
                };
                self.manual_change_buffer.clear();
                if let Some(trigger_string) = &trigger_string {
                    self.manual_change_buffer.push_str(trigger_string);
                }
            }
        }
    }

    fn draw_entry(
        &mut self,
        name: &str,
        selection: Selection,
        ui: &Ui,
        config: &mut Config,
        data: &SettingsData,
    ) {
        let (global_trigger, game_trigger) = {
            let input_map = config.input_map.inner();
            match selection {
                Selection::Keypad(key) => (
                    input_map.global().keypad[&key].clone(),
                    input_map.game().keypad.get(&key).cloned(),
                ),
                Selection::Hotkey(action) => (
                    input_map.global().hotkeys[&action].clone(),
                    input_map.game().hotkeys.get(&action).cloned(),
                ),
            }
        };

        let _id = ui.push_id(name);

        ui.table_next_row();

        ui.table_next_column();
        ui.align_text_to_frame_padding();
        ui.text(format!("{name}:"));

        ui.table_next_column();
        {
            let _id = ui.push_id("global");
            self.draw_input_button(
                global_trigger,
                (selection, false),
                ui,
                config,
                ui.content_region_avail()[0],
            );
        }

        ui.table_next_column();
        {
            let game_override_enabled = game_trigger.is_some();
            let (button_text, tooltip) = if let Some(game_trigger) = game_trigger {
                let width = ui.content_region_avail()[0]
                    - (ui.calc_text_size("-")[0]
                        + style!(ui, frame_padding)[0] * 2.0
                        + style!(ui, item_spacing)[0]);
                let _id = ui.push_id("game");
                self.draw_input_button(game_trigger, (selection, true), ui, config, width);
                ui.same_line();
                ("-", "Remove game override")
            } else {
                ("+", "Add game override")
            };
            ui.enabled(data.game_loaded, || {
                if ui.button(button_text) {
                    config.input_map.update(|inner| {
                        inner.update_game(|map| {
                            if game_override_enabled {
                                match selection {
                                    Selection::Keypad(key) => {
                                        map.keypad.remove(&key);
                                    }
                                    Selection::Hotkey(action) => {
                                        map.hotkeys.remove(&action);
                                    }
                                }
                            } else {
                                match selection {
                                    Selection::Keypad(key) => {
                                        map.keypad.insert(key, None);
                                    }
                                    Selection::Hotkey(action) => {
                                        map.hotkeys.insert(action, None);
                                    }
                                }
                            }
                        })
                    });
                }
                if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
                    ui.tooltip_text(tooltip);
                }
            });
        }

        ui.table_next_column();
        modify_configs_mask!(
            ui,
            icon_tooltip "\u{f1f8}", "Reset",
            "reset",
            true,
            data.game_loaded,
            |global, game| {
                config.input_map.update(|inner| {
                    if global {
                        inner.update_global(|map| {
                            let mut default = Map::default();
                            match selection {
                                Selection::Keypad(key) => {
                                    map.keypad.insert(key, default.keypad.remove(&key).unwrap());
                                }
                                Selection::Hotkey(action) => {
                                    map.hotkeys
                                        .insert(action, default.hotkeys.remove(&action).unwrap());
                                }
                            }
                        });
                    }
                    if game {
                        inner.update_game(|map| match selection {
                            Selection::Keypad(key) => {
                                map.keypad.insert(key, None);
                            }
                            Selection::Hotkey(action) => {
                                map.hotkeys.insert(action, None);
                            }
                        });
                    }
                });
            }
        );
    }

    pub(super) fn draw(&mut self, ui: &Ui, config: &mut Config, data: &SettingsData) {
        if !ui.is_window_focused() {
            self.pressed_keys.clear();
        }

        modify_configs_mask!(
            ui,
            label "\u{f1f8} Restore default map",
            "restore_defaults",
            true,
            data.game_loaded,
            |global, game| {
                if global {
                    config.input_map.update(|inner| inner.set_default_global());
                }
                if game {
                    config.input_map.update(|inner| inner.set_default_game());
                }
            }
        );

        macro_rules! import_map {
            ($set: ident) => {
                if let Some(map_file) = FileDialog::new()
                    .add_filter("JSON configuration file", &["json"])
                    .pick_file()
                    .and_then(|path| config::File::read(&path, false).ok())
                {
                    config
                        .input_map
                        .update(|inner| inner.$set(map_file.contents));
                }
            };
        }

        ui.same_line();
        modify_configs!(
            ui,
            label "\u{f56f} Import map",
            "import",
            data.game_loaded,
            import_map!(set_global),
            import_map!(set_game)
        );

        macro_rules! export_map {
            ($get: ident) => {
                if let Some(path) = FileDialog::new()
                    .add_filter("JSON configuration file", &["json"])
                    .set_file_name("keymap.json")
                    .save_file()
                {
                    let _ = config::File {
                        contents: config.input_map.inner().$get().clone(),
                        path: Some(path),
                    }
                    .write();
                }
            };
        }

        ui.same_line();
        modify_configs!(
            ui,
            label "\u{f56e} Export map",
            "export",
            data.game_loaded,
            export_map!(global),
            export_map!(game)
        );

        heading(ui, "Keypad", 16.0, 5.0);

        macro_rules! section {
            ($id: literal, $draw: expr) => {
                if let Some(_table_token) = ui.begin_table_with_flags(
                    $id,
                    4,
                    TableFlags::SIZING_STRETCH_SAME | TableFlags::NO_CLIP,
                ) {
                    ui.table_setup_column_with(TableColumnSetup {
                        flags: TableColumnFlags::WIDTH_FIXED,
                        ..TableColumnSetup::new("")
                    });
                    ui.table_setup_column("");
                    ui.table_setup_column_with(TableColumnSetup {
                        flags: if data.game_loaded {
                            TableColumnFlags::empty()
                        } else {
                            TableColumnFlags::WIDTH_FIXED
                        },
                        ..TableColumnSetup::new("")
                    });
                    ui.table_setup_column_with(TableColumnSetup {
                        flags: TableColumnFlags::WIDTH_FIXED,
                        ..TableColumnSetup::new("")
                    });

                    $draw
                }
            };
        }

        section!("keypad", {
            for &(key, name) in KEYS {
                self.draw_entry(name, Selection::Keypad(key), ui, config, data);
            }
        });

        ui.dummy([0.0, 8.0]);
        heading(ui, "Hotkeys", 16.0, 5.0);

        section!("hotkeys", {
            for &(action, name) in ACTIONS {
                self.draw_entry(name, Selection::Hotkey(action), ui, config, data);
            }
        });
    }

    pub fn process_event<T: 'static>(&mut self, event: &Event<T>, config: &mut Config) {
        if let Event::WindowEvent {
            event:
                WindowEvent::KeyboardInput {
                    input,
                    is_synthetic: false,
                    ..
                },
            ..
        } = event
        {
            let key = (input.virtual_keycode, input.scancode);
            if input.state == ElementState::Released {
                self.pressed_keys.remove(&key);

                if self.state.is_capturing() {
                    self.finalize(config);
                }
            } else {
                self.pressed_keys.insert(key);

                if self.state.is_capturing() {
                    let new_trigger = if let Some(key_code) = input.virtual_keycode {
                        Trigger::KeyCode(key_code)
                    } else {
                        Trigger::ScanCode(input.scancode, None)
                    };

                    if let Some(trigger) = &mut self.current_trigger {
                        match trigger {
                            Trigger::Chain(trigger::Op::And, contents) => {
                                if !contents.contains(&new_trigger) {
                                    contents.push(new_trigger);
                                }
                            }

                            others => {
                                if *others != new_trigger {
                                    let others = self.current_trigger.take().unwrap();
                                    self.current_trigger = Some(Trigger::Chain(
                                        trigger::Op::And,
                                        vec![others, new_trigger],
                                    ));
                                }
                            }
                        }
                    } else {
                        self.current_trigger = Some(new_trigger);
                    }
                }
            }
        }
    }
}
