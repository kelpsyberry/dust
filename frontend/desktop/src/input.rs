mod map_editor;
pub use map_editor::MapEditor;
mod map;
pub use map::Map;
mod state;
pub use state::{Changes, State};
pub mod trigger;

use winit::event::{ScanCode, VirtualKeyCode};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    PlayPause,
    Reset,
    Stop,
    ToggleFramerateLimit,
    ToggleSyncToAudio,
    ToggleFullscreenRender,
}

type PressedKey = (Option<VirtualKeyCode>, ScanCode);
