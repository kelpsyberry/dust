mod map;
pub use map::Map;
mod state;
pub use state::{Changes, State};
pub mod trigger;

use winit::event::{ScanCode, VirtualKeyCode};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    PlayPause,
    Reset,
    Stop,
    ToggleFramerateLimit,
    ToggleSyncToAudio,
    ToggleFullWindowScreen,
}

pub type PressedKey = (Option<VirtualKeyCode>, ScanCode);
