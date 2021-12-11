use super::{trigger::Trigger, PressedKey, State as InputState};
use dust_core::emu::input::Keys;
use imgui::{StyleColor, Ui, Window};
use winit::event::{ElementState, Event, WindowEvent};

#[derive(Default)]
pub struct Editor {
    current_key: Option<Keys>,
    pressed_keys: Vec<PressedKey>,
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

impl Editor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn draw(&mut self, ui: &Ui, input_state: &mut InputState, opened: &mut bool) {
        Window::new("Keymap").opened(opened).build(ui, || {
            if !ui.is_window_focused() {
                self.pressed_keys.clear();
            }

            ui.columns(2, "input", true);
            for &(key, name) in KEYS {
                let id = format!("{}:", name);
                ui.text(&id);
                ui.same_line();

                let id_ = ui.push_id(&id);
                let button_color = if self.current_key == Some(key) {
                    Some(ui.push_style_color(
                        StyleColor::Button,
                        ui.style_color(StyleColor::ButtonActive),
                    ))
                } else if input_state.keymap.contents.0[&key].activated(&self.pressed_keys) {
                    Some(ui.push_style_color(
                        StyleColor::Button,
                        ui.style_color(StyleColor::ButtonHovered),
                    ))
                } else {
                    None
                };

                if ui.button(&input_state.keymap.contents.0[&key].to_string()) {
                    ui.set_keyboard_focus_here();
                    self.current_key = Some(key);
                }

                drop((id_, button_color));
                if self.current_key == Some(key) && !ui.is_item_focused() {
                    self.current_key = None;
                }

                ui.next_column();
            }
            ui.columns(1, "", false);
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
            } else if !self.pressed_keys.contains(&key) {
                self.pressed_keys.push(key);
            }

            if let Some(current_key) = self.current_key.take() {
                if let Some(key_code) = input.virtual_keycode {
                    input_state
                        .keymap
                        .contents
                        .0
                        .insert(current_key, Trigger::KeyCode(key_code));
                } else {
                    input_state.keymap.contents.0.remove(&current_key);
                }
            }
        }
    }
}
