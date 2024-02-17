use std::str::FromStr;
use winit::keyboard::{KeyCode as WKeyCode, NativeKeyCode};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyCode(pub WKeyCode);

impl From<WKeyCode> for KeyCode {
    fn from(value: WKeyCode) -> Self {
        KeyCode(value)
    }
}

static KEY_CODE_STR_MAP: &[(WKeyCode, &str)] = &[
    (WKeyCode::Backquote, "Backquote"),
    (WKeyCode::Backslash, "Backslash"),
    (WKeyCode::BracketLeft, "BracketLeft"),
    (WKeyCode::BracketRight, "BracketRight"),
    (WKeyCode::Comma, "Comma"),
    (WKeyCode::Digit0, "0"),
    (WKeyCode::Digit1, "1"),
    (WKeyCode::Digit2, "2"),
    (WKeyCode::Digit3, "3"),
    (WKeyCode::Digit4, "4"),
    (WKeyCode::Digit5, "5"),
    (WKeyCode::Digit6, "6"),
    (WKeyCode::Digit7, "7"),
    (WKeyCode::Digit8, "8"),
    (WKeyCode::Digit9, "9"),
    (WKeyCode::Equal, "Equal"),
    (WKeyCode::IntlBackslash, "IntlBackslash"),
    (WKeyCode::IntlRo, "IntlRo"),
    (WKeyCode::IntlYen, "IntlYen"),
    (WKeyCode::KeyA, "A"),
    (WKeyCode::KeyB, "B"),
    (WKeyCode::KeyC, "C"),
    (WKeyCode::KeyD, "D"),
    (WKeyCode::KeyE, "E"),
    (WKeyCode::KeyF, "F"),
    (WKeyCode::KeyG, "G"),
    (WKeyCode::KeyH, "H"),
    (WKeyCode::KeyI, "I"),
    (WKeyCode::KeyJ, "J"),
    (WKeyCode::KeyK, "K"),
    (WKeyCode::KeyL, "L"),
    (WKeyCode::KeyM, "M"),
    (WKeyCode::KeyN, "N"),
    (WKeyCode::KeyO, "O"),
    (WKeyCode::KeyP, "P"),
    (WKeyCode::KeyQ, "Q"),
    (WKeyCode::KeyR, "R"),
    (WKeyCode::KeyS, "S"),
    (WKeyCode::KeyT, "T"),
    (WKeyCode::KeyU, "U"),
    (WKeyCode::KeyV, "V"),
    (WKeyCode::KeyW, "W"),
    (WKeyCode::KeyX, "X"),
    (WKeyCode::KeyY, "Y"),
    (WKeyCode::KeyZ, "Z"),
    (WKeyCode::Minus, "Minus"),
    (WKeyCode::Period, "Period"),
    (WKeyCode::Quote, "Quote"),
    (WKeyCode::Semicolon, "Semicolon"),
    (WKeyCode::Slash, "Slash"),
    (WKeyCode::AltLeft, "AltLeft"),
    (WKeyCode::AltRight, "AltRight"),
    (WKeyCode::Backspace, "Backspace"),
    (WKeyCode::CapsLock, "CapsLock"),
    (WKeyCode::ContextMenu, "ContextMenu"),
    (WKeyCode::ControlLeft, "ControlLeft"),
    (WKeyCode::ControlRight, "ControlRight"),
    (WKeyCode::Enter, "Enter"),
    (WKeyCode::SuperLeft, "SuperLeft"),
    (WKeyCode::SuperRight, "SuperRight"),
    (WKeyCode::ShiftLeft, "ShiftLeft"),
    (WKeyCode::ShiftRight, "ShiftRight"),
    (WKeyCode::Space, "Space"),
    (WKeyCode::Tab, "Tab"),
    (WKeyCode::Convert, "Convert"),
    (WKeyCode::KanaMode, "KanaMode"),
    (WKeyCode::Lang1, "Lang1"),
    (WKeyCode::Lang2, "Lang2"),
    (WKeyCode::Lang3, "Lang3"),
    (WKeyCode::Lang4, "Lang4"),
    (WKeyCode::Lang5, "Lang5"),
    (WKeyCode::NonConvert, "NonConvert"),
    (WKeyCode::Delete, "Delete"),
    (WKeyCode::End, "End"),
    (WKeyCode::Help, "Help"),
    (WKeyCode::Home, "Home"),
    (WKeyCode::Insert, "Insert"),
    (WKeyCode::PageDown, "PageDown"),
    (WKeyCode::PageUp, "PageUp"),
    (WKeyCode::ArrowDown, "ArrowDown"),
    (WKeyCode::ArrowLeft, "ArrowLeft"),
    (WKeyCode::ArrowRight, "ArrowRight"),
    (WKeyCode::ArrowUp, "ArrowUp"),
    (WKeyCode::NumLock, "NumLock"),
    (WKeyCode::Numpad0, "Numpad0"),
    (WKeyCode::Numpad1, "Numpad1"),
    (WKeyCode::Numpad2, "Numpad2"),
    (WKeyCode::Numpad3, "Numpad3"),
    (WKeyCode::Numpad4, "Numpad4"),
    (WKeyCode::Numpad5, "Numpad5"),
    (WKeyCode::Numpad6, "Numpad6"),
    (WKeyCode::Numpad7, "Numpad7"),
    (WKeyCode::Numpad8, "Numpad8"),
    (WKeyCode::Numpad9, "Numpad9"),
    (WKeyCode::NumpadAdd, "NumpadAdd"),
    (WKeyCode::NumpadBackspace, "NumpadBackspace"),
    (WKeyCode::NumpadClear, "NumpadClear"),
    (WKeyCode::NumpadClearEntry, "NumpadClearEntry"),
    (WKeyCode::NumpadComma, "NumpadComma"),
    (WKeyCode::NumpadDecimal, "NumpadDecimal"),
    (WKeyCode::NumpadDivide, "NumpadDivide"),
    (WKeyCode::NumpadEnter, "NumpadEnter"),
    (WKeyCode::NumpadEqual, "NumpadEqual"),
    (WKeyCode::NumpadHash, "NumpadHash"),
    (WKeyCode::NumpadMemoryAdd, "NumpadMemoryAdd"),
    (WKeyCode::NumpadMemoryClear, "NumpadMemoryClear"),
    (WKeyCode::NumpadMemoryRecall, "NumpadMemoryRecall"),
    (WKeyCode::NumpadMemoryStore, "NumpadMemoryStore"),
    (WKeyCode::NumpadMemorySubtract, "NumpadMemorySubtract"),
    (WKeyCode::NumpadMultiply, "NumpadMultiply"),
    (WKeyCode::NumpadParenLeft, "NumpadParenLeft"),
    (WKeyCode::NumpadParenRight, "NumpadParenRight"),
    (WKeyCode::NumpadStar, "NumpadStar"),
    (WKeyCode::NumpadSubtract, "NumpadSubtract"),
    (WKeyCode::Escape, "Escape"),
    (WKeyCode::Fn, "Fn"),
    (WKeyCode::FnLock, "FnLock"),
    (WKeyCode::PrintScreen, "PrintScreen"),
    (WKeyCode::ScrollLock, "ScrollLock"),
    (WKeyCode::Pause, "Pause"),
    (WKeyCode::BrowserBack, "BrowserBack"),
    (WKeyCode::BrowserFavorites, "BrowserFavorites"),
    (WKeyCode::BrowserForward, "BrowserForward"),
    (WKeyCode::BrowserHome, "BrowserHome"),
    (WKeyCode::BrowserRefresh, "BrowserRefresh"),
    (WKeyCode::BrowserSearch, "BrowserSearch"),
    (WKeyCode::BrowserStop, "BrowserStop"),
    (WKeyCode::Eject, "Eject"),
    (WKeyCode::LaunchApp1, "LaunchApp1"),
    (WKeyCode::LaunchApp2, "LaunchApp2"),
    (WKeyCode::LaunchMail, "LaunchMail"),
    (WKeyCode::MediaPlayPause, "MediaPlayPause"),
    (WKeyCode::MediaSelect, "MediaSelect"),
    (WKeyCode::MediaStop, "MediaStop"),
    (WKeyCode::MediaTrackNext, "MediaTrackNext"),
    (WKeyCode::MediaTrackPrevious, "MediaTrackPrevious"),
    (WKeyCode::Power, "Power"),
    (WKeyCode::Sleep, "Sleep"),
    (WKeyCode::AudioVolumeDown, "AudioVolumeDown"),
    (WKeyCode::AudioVolumeMute, "AudioVolumeMute"),
    (WKeyCode::AudioVolumeUp, "AudioVolumeUp"),
    (WKeyCode::WakeUp, "WakeUp"),
    (WKeyCode::Meta, "Meta"),
    (WKeyCode::Hyper, "Hyper"),
    (WKeyCode::Turbo, "Turbo"),
    (WKeyCode::Abort, "Abort"),
    (WKeyCode::Resume, "Resume"),
    (WKeyCode::Suspend, "Suspend"),
    (WKeyCode::Again, "Again"),
    (WKeyCode::Copy, "Copy"),
    (WKeyCode::Cut, "Cut"),
    (WKeyCode::Find, "Find"),
    (WKeyCode::Open, "Open"),
    (WKeyCode::Paste, "Paste"),
    (WKeyCode::Props, "Props"),
    (WKeyCode::Select, "Select"),
    (WKeyCode::Undo, "Undo"),
    (WKeyCode::Hiragana, "Hiragana"),
    (WKeyCode::Katakana, "Katakana"),
    (WKeyCode::F1, "F1"),
    (WKeyCode::F2, "F2"),
    (WKeyCode::F3, "F3"),
    (WKeyCode::F4, "F4"),
    (WKeyCode::F5, "F5"),
    (WKeyCode::F6, "F6"),
    (WKeyCode::F7, "F7"),
    (WKeyCode::F8, "F8"),
    (WKeyCode::F9, "F9"),
    (WKeyCode::F10, "F10"),
    (WKeyCode::F11, "F11"),
    (WKeyCode::F12, "F12"),
    (WKeyCode::F13, "F13"),
    (WKeyCode::F14, "F14"),
    (WKeyCode::F15, "F15"),
    (WKeyCode::F16, "F16"),
    (WKeyCode::F17, "F17"),
    (WKeyCode::F18, "F18"),
    (WKeyCode::F19, "F19"),
    (WKeyCode::F20, "F20"),
    (WKeyCode::F21, "F21"),
    (WKeyCode::F22, "F22"),
    (WKeyCode::F23, "F23"),
    (WKeyCode::F24, "F24"),
    (WKeyCode::F25, "F25"),
    (WKeyCode::F26, "F26"),
    (WKeyCode::F27, "F27"),
    (WKeyCode::F28, "F28"),
    (WKeyCode::F29, "F29"),
    (WKeyCode::F30, "F30"),
    (WKeyCode::F31, "F31"),
    (WKeyCode::F32, "F32"),
    (WKeyCode::F33, "F33"),
    (WKeyCode::F34, "F34"),
    (WKeyCode::F35, "F35"),
];

impl From<KeyCode> for &'static str {
    fn from(value: KeyCode) -> Self {
        KEY_CODE_STR_MAP
            .iter()
            .find_map(|(key_code, str)| (*key_code == value.0).then_some(*str))
            .expect("invalid key code")
    }
}

impl FromStr for KeyCode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        Ok(KeyCode(
            KEY_CODE_STR_MAP
                .iter()
                .find_map(|(key_code, str)| (*str == s).then_some(*key_code))
                .ok_or(())?,
        ))
    }
}

impl TryFrom<&str> for KeyCode {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, ()> {
        Self::from_str(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScanCode(NativeKeyCode);

impl TryFrom<NativeKeyCode> for ScanCode {
    type Error = ();

    fn try_from(value: NativeKeyCode) -> Result<Self, ()> {
        if matches!(value, NativeKeyCode::Unidentified) {
            return Err(());
        }
        Ok(ScanCode(value))
    }
}

#[allow(clippy::to_string_trait_impl)]
impl ToString for ScanCode {
    fn to_string(&self) -> String {
        match self.0 {
            NativeKeyCode::Android(scan_code) => format!("A{scan_code}"),
            NativeKeyCode::MacOS(scan_code) => format!("M{scan_code}"),
            NativeKeyCode::Windows(scan_code) => format!("W{scan_code}"),
            NativeKeyCode::Xkb(scan_code) => format!("X{scan_code}"),
            NativeKeyCode::Unidentified => unreachable!(),
        }
    }
}

impl FromStr for ScanCode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        if !s.is_char_boundary(1) {
            return Err(());
        }
        let (first_char, scan_code) = s.split_at(1);
        Ok(ScanCode(match first_char {
            "A" => NativeKeyCode::Android(scan_code.parse().map_err(drop)?),
            "M" => NativeKeyCode::MacOS(scan_code.parse().map_err(drop)?),
            "W" => NativeKeyCode::Windows(scan_code.parse().map_err(drop)?),
            "X" => NativeKeyCode::Xkb(scan_code.parse().map_err(drop)?),
            _ => return Err(()),
        }))
    }
}

impl TryFrom<&str> for ScanCode {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, ()> {
        Self::from_str(value)
    }
}
