mod editor;
pub use editor::Editor;
mod keymap;
pub mod trigger;
pub use keymap::Keymap;

use super::config::Config;
use dust_core::emu::input::Keys as EmuKeys;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, MouseButton, ScanCode, VirtualKeyCode, WindowEvent},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Changes {
    pub pressed: EmuKeys,
    pub released: EmuKeys,
    pub touch_pos: Option<Option<[u16; 2]>>,
}

type PressedKey = (Option<VirtualKeyCode>, ScanCode);

pub struct State {
    pressed_keys: Vec<PressedKey>,
    pub keymap: Config<Keymap>,
    touchscreen_bounds: (PhysicalPosition<f64>, PhysicalPosition<f64>),
    touchscreen_center: PhysicalPosition<f64>,
    touchscreen_half_size: PhysicalSize<f64>,
    mouse_pos: PhysicalPosition<f64>,
    touch_pos: Option<[u16; 2]>,
    prev_touch_pos: Option<[u16; 2]>,
    pressed_emu_keys: EmuKeys,
}

impl State {
    pub fn new(keymap: Config<Keymap>) -> Self {
        State {
            pressed_keys: vec![],
            keymap,
            touchscreen_bounds: Default::default(),
            touchscreen_center: Default::default(),
            touchscreen_half_size: Default::default(),
            mouse_pos: Default::default(),
            touch_pos: None,
            prev_touch_pos: None,
            pressed_emu_keys: EmuKeys::empty(),
        }
    }

    pub fn set_touchscreen_bounds(
        &mut self,
        bounds: (PhysicalPosition<f64>, PhysicalPosition<f64>),
    ) {
        self.touchscreen_bounds = bounds;
        self.touchscreen_center = (
            (bounds.0.x + bounds.1.x) * 0.5,
            (bounds.0.y + bounds.1.y) * 0.5,
        )
            .into();
        self.touchscreen_half_size = (
            (bounds.1.x - bounds.0.x) * 0.5,
            (bounds.1.y - bounds.0.y) * 0.5,
        )
            .into();
    }

    fn recalculate_touch_pos(&mut self) {
        let diff = (
            self.mouse_pos.x - self.touchscreen_center.x,
            self.mouse_pos.y - self.touchscreen_center.y,
        );
        let scale = (self.touchscreen_half_size.width / diff.0)
            .abs()
            .min((self.touchscreen_half_size.height / diff.1).abs())
            .min(1.0);
        self.touch_pos = Some([
            (((diff.0 * scale) / self.touchscreen_half_size.width + 1.0) * 2048.0)
                .clamp(0.0, 4095.0) as u16,
            (((diff.1 * scale) / self.touchscreen_half_size.height + 1.0) * 2048.0)
                .clamp(0.0, 4095.0) as u16,
        ]);
    }

    pub fn process_event<T: 'static>(&mut self, event: &Event<T>, catch_new: bool) {
        if let Event::WindowEvent { event, .. } = event {
            match event {
                WindowEvent::KeyboardInput {
                    input,
                    is_synthetic: false,
                    ..
                } => {
                    let key = (input.virtual_keycode, input.scancode);
                    if input.state == ElementState::Released {
                        if let Some(i) = self.pressed_keys.iter().position(|k| *k == key) {
                            self.pressed_keys.remove(i);
                        }
                    } else if catch_new && !self.pressed_keys.contains(&key) {
                        self.pressed_keys.push(key);
                    }
                }

                WindowEvent::CursorMoved { position, .. } => {
                    self.mouse_pos = *position;
                    if self.touch_pos.is_some() {
                        self.recalculate_touch_pos();
                    }
                }

                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } => {
                    if *state == ElementState::Pressed {
                        if catch_new
                            && (self.touchscreen_bounds.0.x..self.touchscreen_bounds.1.x)
                                .contains(&self.mouse_pos.x)
                            && (self.touchscreen_bounds.0.y..self.touchscreen_bounds.1.y)
                                .contains(&self.mouse_pos.y)
                        {
                            self.recalculate_touch_pos();
                        }
                    } else {
                        self.touch_pos = None;
                    }
                }

                WindowEvent::Focused(false) => {
                    self.pressed_keys.clear();
                    self.touch_pos = None;
                }

                _ => {}
            }
        }
    }

    pub fn drain_changes(&mut self) -> Option<Changes> {
        let mut new_pressed_emu_keys = EmuKeys::empty();
        for (&emu_key, trigger) in &self.keymap.contents.0 {
            new_pressed_emu_keys.set(emu_key, trigger.activated(&self.pressed_keys));
        }

        let pressed = new_pressed_emu_keys & !self.pressed_emu_keys;
        let released = self.pressed_emu_keys & !new_pressed_emu_keys;
        let touch_pos = if self.touch_pos == self.prev_touch_pos {
            None
        } else {
            Some(self.touch_pos)
        };

        if touch_pos.is_some() || new_pressed_emu_keys != self.pressed_emu_keys {
            self.pressed_emu_keys = new_pressed_emu_keys;
            self.prev_touch_pos = self.touch_pos;
            Some(Changes {
                pressed,
                released,
                touch_pos,
            })
        } else {
            None
        }
    }
}
