mod editor;
pub use editor::Editor;
mod keymap;
pub mod trigger;
pub use keymap::Keymap;

use super::config::Config;
use dust_core::emu::input::Keys as EmuKeys;
use std::collections::HashSet;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, MouseButton, ScanCode, VirtualKeyCode, WindowEvent},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    PlayPause,
    Reset,
    Stop,
    ToggleFullscreenRender,
    ToggleAudioSync,
    ToggleFramerateLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Changes {
    pub pressed: EmuKeys,
    pub released: EmuKeys,
    pub touch_pos: Option<Option<[u16; 2]>>,
}

type PressedKey = (Option<VirtualKeyCode>, ScanCode);

pub struct State {
    pressed_keys: HashSet<PressedKey>,
    pub keymap: Config<Keymap>,
    touchscreen_center: PhysicalPosition<f64>,
    touchscreen_size: PhysicalSize<f64>,
    touchscreen_half_size: PhysicalSize<f64>,
    touchscreen_rot: (f64, f64),
    touchscreen_rot_center: PhysicalPosition<f64>,
    mouse_pos: PhysicalPosition<f64>,
    touch_pos: Option<[u16; 2]>,
    prev_touch_pos: Option<[u16; 2]>,
    pressed_emu_keys: EmuKeys,
    pressed_hotkeys: HashSet<Action>,
}

impl State {
    pub fn new(keymap: Config<Keymap>) -> Self {
        State {
            pressed_keys: HashSet::new(),
            keymap,
            touchscreen_size: Default::default(),
            touchscreen_center: Default::default(),
            touchscreen_half_size: Default::default(),
            touchscreen_rot: (0.0, 1.0),
            touchscreen_rot_center: Default::default(),
            mouse_pos: Default::default(),
            touch_pos: None,
            prev_touch_pos: None,
            pressed_emu_keys: EmuKeys::empty(),
            pressed_hotkeys: HashSet::new(),
        }
    }

    pub fn set_touchscreen_bounds(
        &mut self,
        rot_center: PhysicalPosition<f64>,
        center: PhysicalPosition<f64>,
        size: PhysicalSize<f64>,
        rot: f64,
    ) {
        self.touchscreen_center = center;
        self.touchscreen_size = size;
        self.touchscreen_half_size = (size.width * 0.5, size.height * 0.5).into();
        self.touchscreen_rot = rot.sin_cos();
        self.touchscreen_rot_center = rot_center;
    }

    fn recalculate_touch_pos<const CLAMP: bool>(&mut self) {
        let mut diff = [
            self.mouse_pos.x - self.touchscreen_rot_center.x,
            self.mouse_pos.y - self.touchscreen_rot_center.y,
        ];
        diff = [
            self.touchscreen_rot_center.x
                + diff[0] * self.touchscreen_rot.1
                + diff[1] * self.touchscreen_rot.0
                - self.touchscreen_center.x,
            self.touchscreen_rot_center.y - diff[0] * self.touchscreen_rot.0
                + diff[1] * self.touchscreen_rot.1
                - self.touchscreen_center.y,
        ];
        if CLAMP {
            let scale = (self.touchscreen_half_size.width / diff[0])
                .abs()
                .min((self.touchscreen_half_size.height / diff[1]).abs())
                .min(1.0);
            diff = diff.map(|v| v * scale);
        } else if diff[0].abs() >= self.touchscreen_half_size.width
            || diff[1].abs() >= self.touchscreen_half_size.height
        {
            return;
        }
        self.touch_pos = Some([
            ((diff[0] / self.touchscreen_half_size.width + 1.0) * 2048.0).clamp(0.0, 4095.0) as u16,
            ((diff[1] / self.touchscreen_half_size.height + 1.0) * 1536.0).clamp(0.0, 3072.0)
                as u16,
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
                        self.pressed_keys.remove(&key);
                    } else if catch_new {
                        self.pressed_keys.insert(key);
                    }
                }

                WindowEvent::CursorMoved { position, .. } => {
                    self.mouse_pos = *position;
                    if self.touch_pos.is_some() {
                        self.recalculate_touch_pos::<true>();
                    }
                }

                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } => {
                    if *state == ElementState::Pressed {
                        if catch_new {
                            self.recalculate_touch_pos::<false>();
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

    pub fn drain_changes(&mut self, emu_playing: bool) -> (Vec<Action>, Option<Changes>) {
        let mut actions = Vec::new();
        for (&action, trigger) in &self.keymap.contents.hotkeys {
            if let Some(trigger) = trigger {
                if trigger.activated(&self.pressed_keys) {
                    if self.pressed_hotkeys.insert(action) {
                        actions.push(action);
                    }
                } else {
                    self.pressed_hotkeys.remove(&action);
                }
            }
        }

        if !emu_playing {
            return (actions, None);
        }

        let mut new_pressed_emu_keys = EmuKeys::empty();
        for (&emu_key, trigger) in &self.keymap.contents.keypad {
            new_pressed_emu_keys.set(emu_key, trigger.activated(&self.pressed_keys));
        }

        let pressed = new_pressed_emu_keys & !self.pressed_emu_keys;
        let released = self.pressed_emu_keys & !new_pressed_emu_keys;
        let touch_pos = if self.touch_pos == self.prev_touch_pos {
            None
        } else {
            Some(self.touch_pos)
        };

        (
            actions,
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
            },
        )
    }
}
