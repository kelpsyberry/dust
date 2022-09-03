use super::{
    trigger::{self, Trigger},
    Action, Map, PressedKey,
};
use crate::{config, ui::utils::heading};
use ahash::AHashSet as HashSet;
use dust_core::emu::input::Keys;
use imgui::{MouseButton, StyleColor, TableFlags, Ui};
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
        selection: Selection,
        need_focus: bool,
    },
    ManuallyChanging {
        selection: Selection,
        need_focus: bool,
    },
}

impl State {
    fn selection(self) -> Option<Selection> {
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
            State::Capturing {
                need_focus: needs_focus,
                ..
            }
            | State::ManuallyChanging {
                need_focus: needs_focus,
                ..
            } => {
                let result = *needs_focus;
                *needs_focus = false;
                result
            }
            _ => false,
        }
    }
}

pub struct MapEditor {
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
    (
        Action::ToggleFullscreenRender,
        "Toggle fullscreen rendering",
    ),
];

impl MapEditor {
    pub fn new() -> Self {
        MapEditor {
            state: State::None,
            pressed_keys: HashSet::new(),
            manual_change_buffer: String::new(),
            current_trigger: None,
        }
    }

    fn finalize(&mut self, map: &mut Map) {
        if let Some((trigger, selection)) = match self.state {
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
            match selection {
                Selection::Keypad(key) => {
                    map.keypad.insert(key, trigger);
                }
                Selection::Hotkey(action) => {
                    map.hotkeys.insert(action, trigger);
                }
            }
        }
        self.state = State::None;
    }

    fn draw_input_button(&mut self, ui: &Ui, map: &mut Map, selection: Selection, name: &str) {
        ui.text(&format!("{name}:"));
        ui.same_line();

        let trigger = match selection {
            Selection::Keypad(key) => map.keypad[&key].as_ref(),
            Selection::Hotkey(action) => map.hotkeys[&action].as_ref(),
        };

        {
            let _button_color = if !self.state.is_manually_changing()
                && trigger.map_or(false, |trigger| trigger.activated(&self.pressed_keys))
            {
                Some(ui.push_style_color(
                    StyleColor::Button,
                    ui.style_color(StyleColor::ButtonHovered),
                ))
            } else {
                None
            };

            let mut label = match trigger {
                Some(trigger) => trigger.to_string(),
                None => "-".to_string(),
            };

            if self.state.selection() == Some(selection) {
                let _id = ui.push_id(name);

                if self.state.drain_needs_focus() {
                    ui.set_keyboard_focus_here();
                }

                let finished = if self.state.is_manually_changing() {
                    ui.input_text("", &mut self.manual_change_buffer)
                        .enter_returns_true(true)
                        .build()
                } else {
                    ui.input_text("", &mut label).read_only(true).build();
                    ui.is_item_deactivated()
                };

                if finished {
                    self.finalize(map);
                }
            } else if ui.button(&label) {
                self.finalize(map);
                self.state = State::Capturing {
                    selection,
                    need_focus: true,
                };
            } else if ui.is_item_clicked_with_button(MouseButton::Right) {
                self.finalize(map);
                self.state = State::ManuallyChanging {
                    selection,
                    need_focus: true,
                };
                self.manual_change_buffer = label.to_string();
            }
        }
    }

    pub fn draw(&mut self, ui: &Ui, map: &mut config::File<Map>, opened: &mut bool) {
        ui.window("Input configuration").opened(opened).build(|| {
            if !ui.is_window_focused() {
                self.pressed_keys.clear();
            }

            if ui.button("Restore defaults") {
                map.contents = Map::default();
            }
            ui.same_line();
            if ui.button("Reload") {
                let _ = map.reload();
            }
            ui.same_line();
            if ui.button("Save") {
                let _ = map.write();
            }

            heading(ui, "Keypad", 16.0, 5.0);

            if let Some(_table_token) = ui.begin_table_with_flags(
                "input",
                2,
                TableFlags::BORDERS_INNER_V | TableFlags::NO_CLIP | TableFlags::SIZING_STRETCH_SAME,
            ) {
                for &(key, name) in KEYS {
                    ui.table_next_column();
                    self.draw_input_button(ui, &mut map.contents, Selection::Keypad(key), name);
                }
            }

            ui.dummy([0.0, 8.0]);
            heading(ui, "Hotkeys", 16.0, 5.0);

            for &(action, name) in ACTIONS {
                self.draw_input_button(ui, &mut map.contents, Selection::Hotkey(action), name);
            }
        });
    }

    pub fn process_event<T: 'static>(&mut self, event: &Event<T>, map: &mut Map) {
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
                    self.finalize(map);
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

impl Default for MapEditor {
    fn default() -> Self {
        Self::new()
    }
}
