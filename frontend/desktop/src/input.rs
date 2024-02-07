mod map;
pub use map::{GlobalMap, Map};
mod state;
pub use state::{Changes, State};
pub mod key_codes;
pub mod trigger;
pub use key_codes::{KeyCode, ScanCode};

use winit::keyboard::PhysicalKey;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    PlayPause,
    Reset,
    Stop,
    ToggleFramerateLimit,
    ToggleSyncToAudio,
    ToggleFullWindowScreen,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PressedKey {
    KeyCode(KeyCode),
    ScanCode(ScanCode),
}

impl TryFrom<PhysicalKey> for PressedKey {
    type Error = ();

    fn try_from(value: PhysicalKey) -> Result<Self, ()> {
        Ok(match value {
            PhysicalKey::Code(key_code) => PressedKey::KeyCode(key_code.into()),
            PhysicalKey::Unidentified(scan_code) => {
                let Ok(scan_code) = scan_code.try_into() else {
                    return Err(());
                };
                PressedKey::ScanCode(scan_code)
            }
        })
    }
}
