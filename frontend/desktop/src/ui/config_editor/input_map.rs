use super::{SettingsData, Tab, BORDER_WIDTH};
use crate::input::{
    trigger::{self, Trigger},
    Action, GlobalMap, Map, PressedKey,
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
use winit::event::{Event, KeyEvent, WindowEvent};

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

type InputMap = config::Overridable<Map, GlobalMap, Map, ()>;

fn global_trigger(selection: Selection, input_map: &InputMap) -> &Option<Trigger> {
    match selection {
        Selection::Keypad(key) => &input_map.global().0.keypad[&key],
        Selection::Hotkey(action) => &input_map.global().0.hotkeys[&action],
    }
}

fn game_trigger(selection: Selection, input_map: &InputMap) -> Option<&Option<Trigger>> {
    match selection {
        Selection::Keypad(key) => input_map.game().keypad.get(&key),
        Selection::Hotkey(action) => input_map.game().hotkeys.get(&action),
    }
}

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

    fn finalize(&mut self, input_map: &mut InputMap) {
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
            let update = |map: &mut Map| match selection {
                Selection::Keypad(key) => {
                    map.keypad.insert(key, trigger);
                }
                Selection::Hotkey(action) => {
                    map.hotkeys.insert(action, trigger);
                }
            };
            if game {
                input_map.update_game(update);
            } else {
                input_map.update_global(|map| update(&mut map.0));
            }
        }
        self.state = State::None;
    }

    fn draw_input_button(
        &mut self,
        selection: (Selection, bool),
        ui: &Ui,
        input_map: &mut InputMap,
        tooltip: &str,
        width: f32,
    ) {
        let trigger = if selection.1 {
            global_trigger(selection.0, input_map)
        } else {
            game_trigger(selection.0, input_map).unwrap()
        };

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
                self.finalize(input_map);
            }
        } else {
            let trigger_string = trigger.as_ref().map(Trigger::to_string);
            if ui.button_with_size(
                format!("{}###", trigger_string.as_deref().unwrap_or("\u{f00d}")),
                [width, 0.0],
            ) {
                self.finalize(input_map);
                self.state = State::Capturing {
                    selection,
                    needs_focus: true,
                };
            } else if ui.is_item_clicked_with_button(MouseButton::Right) {
                self.finalize(input_map);
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

        if !tooltip.is_empty()
            && ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED)
        {
            ui.tooltip_text(tooltip);
        }
    }

    fn draw_entry(
        &mut self,
        name: &str,
        selection: Selection,
        ui: &Ui,
        input_map: &mut InputMap,
        data: &SettingsData,
    ) {
        let _id = ui.push_id(name);

        ui.table_next_row();

        ui.table_next_column();
        ui.align_text_to_frame_padding();
        ui.text(format!("{name}:"));

        ui.table_next_column();

        let tab_is_global = data.cur_tab == Tab::Global;
        let game_override_enabled = game_trigger(selection, input_map).is_some();

        let button_width = ui.calc_text_size("\u{f055}")[0].max(ui.calc_text_size("\u{f056}")[0])
            + style!(ui, frame_padding)[0] * 2.0;
        let mut width = ui.content_region_avail()[0];
        if !tab_is_global {
            width -= button_width + style!(ui, item_spacing)[0];
        }
        if tab_is_global || !game_override_enabled {
            ui.enabled(tab_is_global, || {
                let _id = ui.push_id("global");
                self.draw_input_button(
                    (selection, false),
                    ui,
                    input_map,
                    if tab_is_global { "" } else { "Global setting" },
                    width,
                );
            });
        } else {
            let _id = ui.push_id("game");
            self.draw_input_button((selection, true), ui, input_map, "", width);
        }
        if !tab_is_global {
            ui.same_line();
            let (label, tooltip) = if game_override_enabled {
                ("\u{f056}", "Remove game override")
            } else {
                ("\u{f055}", "Add game override")
            };
            if ui.button_with_size(label, [button_width, 0.0]) {
                if game_override_enabled {
                    input_map.update_game(|map| match selection {
                        Selection::Keypad(key) => {
                            map.keypad.remove(&key);
                        }
                        Selection::Hotkey(action) => {
                            map.hotkeys.remove(&action);
                        }
                    });
                } else {
                    let trigger = global_trigger(selection, input_map).clone();
                    input_map.update_game(|map| match selection {
                        Selection::Keypad(key) => {
                            map.keypad.insert(key, trigger);
                        }
                        Selection::Hotkey(action) => {
                            map.hotkeys.insert(action, trigger);
                        }
                    });
                }
            }
            if ui.is_item_hovered() {
                ui.tooltip_text(tooltip);
            }
        }

        ui.table_next_column();
        ui.enabled(tab_is_global || game_override_enabled, || {
            if ui.button("\u{f1f8}") {
                let update = |map: &mut Map| {
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
                };
                match data.cur_tab {
                    Tab::Global => {
                        input_map.update_global(|map| update(&mut map.0));
                    }
                    Tab::Game => {
                        input_map.update_game(update);
                    }
                }
            }
            if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
                ui.tooltip_text("Set default");
            }
        });
    }

    pub(super) fn draw(&mut self, ui: &Ui, config: &mut Config, data: &SettingsData) {
        let input_map = config.input_map.inner_mut();

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
                    input_map.set_default_global();
                }
                if game {
                    input_map.unset_game();
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
                    input_map.$set(map_file.contents);
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
                    let _ = config::File::write_value(input_map.$get(), &path);
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

        heading(ui, "Keypad", 16.0, 5.0, BORDER_WIDTH);

        macro_rules! section {
            ($id: literal, $draw: expr) => {
                if let Some(_table_token) = ui.begin_table_with_flags(
                    $id,
                    3,
                    TableFlags::SIZING_STRETCH_SAME | TableFlags::NO_CLIP,
                ) {
                    ui.table_setup_column_with(TableColumnSetup {
                        flags: TableColumnFlags::WIDTH_FIXED,
                        ..TableColumnSetup::new("")
                    });
                    ui.table_setup_column("");
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
                self.draw_entry(name, Selection::Keypad(key), ui, input_map, data);
            }
        });

        ui.dummy([0.0, 8.0]);
        heading(ui, "Hotkeys", 16.0, 5.0, BORDER_WIDTH);

        section!("hotkeys", {
            for &(action, name) in ACTIONS {
                self.draw_entry(name, Selection::Hotkey(action), ui, input_map, data);
            }
        });
    }

    pub fn process_event<T: 'static>(&mut self, event: &Event<T>, config: &mut Config) {
        if let Event::WindowEvent {
            event:
                WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            physical_key,
                            state,
                            ..
                        },
                    ..
                },
            ..
        } = event
        {
            let Ok(key) = (*physical_key).try_into() else {
                return;
            };
            if state.is_pressed() {
                self.pressed_keys.insert(key);

                if self.state.is_capturing() {
                    let new_trigger = match key {
                        PressedKey::KeyCode(key_code) => Trigger::KeyCode(key_code),
                        PressedKey::ScanCode(scan_code) => Trigger::ScanCode(scan_code),
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
            } else {
                self.pressed_keys.remove(&key);

                if self.state.is_capturing() {
                    self.finalize(config.input_map.inner_mut());
                }
            }
        }
    }
}
