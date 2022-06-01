use super::{
    trigger::{self, Trigger},
    Action, PressedKey, State as InputState,
};
use dust_core::emu::input::Keys;
use imgui::{MouseButton, StyleColor, TableFlags, Ui};
use winit::event::{ElementState, Event, WindowEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Selection {
    Keypad(Keys),
    Hotkey(Action),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

pub struct Editor {
    state: State,
    pressed_keys: Vec<PressedKey>,
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
    (
        Action::ToggleFullscreenRender,
        "Toggle fullscreen rendering",
    ),
    (Action::ToggleAudioSync, "Toggle audio sync"),
    (Action::ToggleFramerateLimit, "Toggle framerate limit"),
];

fn heading(ui: &Ui, text: &str, indent: f32, margin: f32) {
    let window_pos = ui.window_pos();
    let window_x_bounds = [
        window_pos[0] + ui.window_content_region_min()[0],
        window_pos[0] + ui.window_content_region_max()[0],
    ];
    let separator_color = ui.style_color(StyleColor::Separator);

    let mut text_start_pos = ui.cursor_screen_pos();
    text_start_pos[0] += indent;
    ui.set_cursor_screen_pos(text_start_pos);
    ui.text(text);

    text_start_pos[1] += ui.text_line_height() * 0.5;
    let text_end_x = text_start_pos[0] + ui.calc_text_size(text)[0];

    let draw_list = ui.get_window_draw_list();
    draw_list
        .add_line(
            [window_x_bounds[0], text_start_pos[1]],
            [text_start_pos[0] - margin, text_start_pos[1]],
            separator_color,
        )
        .build();
    draw_list
        .add_line(
            [window_x_bounds[1], text_start_pos[1]],
            [text_end_x + margin, text_start_pos[1]],
            separator_color,
        )
        .build();
}

impl Editor {
    pub fn new() -> Self {
        Editor {
            state: State::None,
            pressed_keys: Vec::new(),
            manual_change_buffer: String::new(),
            current_trigger: None,
        }
    }

    fn finalize(&mut self, input_state: &mut InputState) {
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
                    input_state.keymap.contents.keypad.insert(key, trigger);
                }
                Selection::Hotkey(action) => {
                    input_state.keymap.contents.hotkeys.insert(action, trigger);
                }
            }
        }
        self.state = State::None;
    }

    fn draw_input_button(
        &mut self,
        ui: &Ui,
        input_state: &mut InputState,
        selection: Selection,
        name: &str,
    ) {
        ui.text(&format!("{}:", name));
        ui.same_line();

        let trigger = match selection {
            Selection::Keypad(key) => input_state.keymap.contents.keypad[&key].as_ref(),
            Selection::Hotkey(action) => input_state.keymap.contents.hotkeys[&action].as_ref(),
        };

        {
            let _button_color = if !self.state.is_manually_changing()
                && match trigger {
                    Some(trigger) => trigger.activated(&self.pressed_keys),
                    None => false,
                } {
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
                let _id = ui.push_id(&name);

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
                    self.finalize(input_state);
                }
            } else if ui.button(&label) {
                self.finalize(input_state);
                self.state = State::Capturing {
                    selection,
                    need_focus: true,
                };
            } else if ui.is_item_clicked_with_button(MouseButton::Right) {
                self.finalize(input_state);
                self.state = State::ManuallyChanging {
                    selection,
                    need_focus: true,
                };
                self.manual_change_buffer = label.to_string();
            }
        }
    }

    pub fn draw(&mut self, ui: &Ui, input_state: &mut InputState, opened: &mut bool) {
        ui.window("Input configuration").opened(opened).build(|| {
            if !ui.is_window_focused() {
                self.pressed_keys.clear();
            }

            heading(ui, "Keypad", 16.0, 5.0);

            if let Some(_table_token) = ui.begin_table_with_flags(
                "input",
                2,
                TableFlags::BORDERS_INNER_V | TableFlags::NO_CLIP | TableFlags::SIZING_STRETCH_SAME,
            ) {
                for &(key, name) in KEYS {
                    ui.table_next_column();
                    self.draw_input_button(ui, input_state, Selection::Keypad(key), name);
                }
            }

            ui.dummy([0.0, 8.0]);
            ui.same_line();
            heading(ui, "Hotkeys", 16.0, 5.0);

            for &(action, name) in ACTIONS {
                self.draw_input_button(ui, input_state, Selection::Hotkey(action), name);
            }
        });
    }

    pub fn process_event<T: 'static>(&mut self, event: &Event<T>, input_state: &mut InputState) {
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
                if let Some(i) = self.pressed_keys.iter().position(|k| *k == key) {
                    self.pressed_keys.remove(i);
                }

                if self.state.is_capturing() {
                    self.finalize(input_state);
                }
            } else {
                if !self.pressed_keys.contains(&key) {
                    self.pressed_keys.push(key);
                }

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

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
