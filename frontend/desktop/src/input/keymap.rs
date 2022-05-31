use super::{
    trigger::{self, Trigger},
    Action,
};
use dust_core::emu::input::Keys;
use fxhash::FxHashMap;
use serde::{
    de::{MapAccess, Visitor},
    ser::SerializeMap,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{fmt, hash::Hash, marker::PhantomData};
use winit::event::VirtualKeyCode;

static KEY_IDENTS: &[(Keys, &str)] = &[
    (Keys::A, "a"),
    (Keys::B, "b"),
    (Keys::X, "x"),
    (Keys::Y, "y"),
    (Keys::L, "l"),
    (Keys::R, "r"),
    (Keys::START, "start"),
    (Keys::SELECT, "select"),
    (Keys::RIGHT, "right"),
    (Keys::LEFT, "left"),
    (Keys::UP, "up"),
    (Keys::DOWN, "down"),
    (Keys::DEBUG, "debug"),
];

static ACTION_IDENTS: &[(Action, &str)] = &[
    (Action::PlayPause, "play-pause"),
    (Action::Reset, "reset"),
    (Action::Stop, "stop"),
    (Action::ToggleFullscreenRender, "toggle-fullscreen-render"),
    (Action::ToggleAudioSync, "toggle-audio-sync"),
    (Action::ToggleFramerateLimit, "toggle-framerate-limit"),
];

#[derive(Clone, Debug)]
pub struct Keymap {
    pub keypad: FxHashMap<Keys, Trigger>,
    pub hotkeys: FxHashMap<Action, Option<Trigger>>,
}

fn default_keypad_map() -> FxHashMap<Keys, Trigger> {
    [
        (Keys::A, Trigger::KeyCode(VirtualKeyCode::X)),
        (Keys::B, Trigger::KeyCode(VirtualKeyCode::Z)),
        (Keys::X, Trigger::KeyCode(VirtualKeyCode::S)),
        (Keys::Y, Trigger::KeyCode(VirtualKeyCode::A)),
        (Keys::L, Trigger::KeyCode(VirtualKeyCode::Q)),
        (Keys::R, Trigger::KeyCode(VirtualKeyCode::W)),
        (Keys::START, Trigger::KeyCode(VirtualKeyCode::Return)),
        (
            Keys::SELECT,
            Trigger::Chain(
                trigger::Op::Or,
                vec![
                    Trigger::KeyCode(VirtualKeyCode::LShift),
                    Trigger::KeyCode(VirtualKeyCode::RShift),
                ],
            ),
        ),
        (Keys::RIGHT, Trigger::KeyCode(VirtualKeyCode::Right)),
        (Keys::LEFT, Trigger::KeyCode(VirtualKeyCode::Left)),
        (Keys::UP, Trigger::KeyCode(VirtualKeyCode::Up)),
        (Keys::DOWN, Trigger::KeyCode(VirtualKeyCode::Down)),
        (Keys::DEBUG, Trigger::KeyCode(VirtualKeyCode::Tab)),
    ]
    .into_iter()
    .collect()
}

fn default_hotkey_map() -> FxHashMap<Action, Option<Trigger>> {
    [
        (Action::PlayPause, None),
        (Action::Reset, None),
        (Action::Stop, None),
        (Action::ToggleFullscreenRender, None),
        (Action::ToggleAudioSync, None),
        (Action::ToggleFramerateLimit, None),
    ]
    .into_iter()
    .collect()
}

impl Default for Keymap {
    fn default() -> Self {
        Keymap {
            keypad: default_keypad_map(),
            hotkeys: default_hotkey_map(),
        }
    }
}

impl Serialize for Keymap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;

        struct TriggerMap<'a, T: 'static + Eq, U: 'static + Serialize>(
            &'a FxHashMap<T, U>,
            &'static [(T, &'static str)],
        );
        impl<'a, T: 'static + Eq, U: 'static + Serialize> Serialize for TriggerMap<'a, T, U> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(Some(self.0.len()))?;
                for (key, value) in self.0 {
                    if let Some((_, ident)) = self.1.iter().find(|(key_, _)| key_ == key) {
                        map.serialize_entry(*ident, value)?;
                    }
                }
                map.end()
            }
        }

        let mut keymap = serializer.serialize_struct("Keymap", 2)?;
        keymap.serialize_field("keypad", &TriggerMap(&self.keypad, KEY_IDENTS))?;
        keymap.serialize_field("hotkeys", &TriggerMap(&self.hotkeys, ACTION_IDENTS))?;
        keymap.end()
    }
}

impl<'de> Deserialize<'de> for Keymap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TriggerMapVisitor<T: 'static + Eq + Hash, U: 'static>(
            &'static [(T, &'static str)],
            &'static str,
            PhantomData<U>,
        );

        impl<'de, T: 'static + Eq + Hash + Copy, U: 'static + Deserialize<'de>> Visitor<'de>
            for TriggerMapVisitor<T, U>
        {
            type Value = FxHashMap<T, U>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(self.1)
            }

            fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<Self::Value, M::Error> {
                let mut map = FxHashMap::with_capacity_and_hasher(
                    access.size_hint().unwrap_or(0),
                    Default::default(),
                );

                while let Some((ident, value)) = access.next_entry::<&str, U>()? {
                    if let Some((key, _)) = self.0.iter().find(|(_, ident_)| *ident_ == ident) {
                        map.insert(*key, value);
                    }
                }

                Ok(map)
            }
        }

        struct DeserializedKeypadMap(FxHashMap<Keys, Trigger>);

        impl<'de> Deserialize<'de> for DeserializedKeypadMap {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer
                    .deserialize_map(TriggerMapVisitor::<Keys, Trigger>(
                        KEY_IDENTS,
                        "a map of triggers corresponding to keypad keys",
                        PhantomData,
                    ))
                    .map(Self)
            }
        }

        struct DeserializedHotkeyMap(FxHashMap<Action, Option<Trigger>>);

        impl<'de> Deserialize<'de> for DeserializedHotkeyMap {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer
                    .deserialize_map(TriggerMapVisitor::<Action, Option<Trigger>>(
                        ACTION_IDENTS,
                        "a map of triggers corresponding to action identifiers",
                        PhantomData,
                    ))
                    .map(Self)
            }
        }

        struct KeymapVisitor;

        impl<'de> Visitor<'de> for KeymapVisitor {
            type Value = Keymap;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a keymap")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut keypad = None;
                let mut hotkeys = None;
                loop {
                    if let Ok(next) = map.next_entry::<&str, DeserializedKeypadMap>() {
                        if let Some(("keypad", value)) = next {
                            keypad = Some(value);
                        } else {
                            break;
                        }
                    }
                    if let Ok(next) = map.next_entry::<&str, DeserializedHotkeyMap>() {
                        if let Some(("hotkeys", value)) = next {
                            hotkeys = Some(value);
                        } else {
                            break;
                        }
                    }
                }
                Ok(Keymap {
                    keypad: match keypad {
                        Some(keypad) => keypad.0,
                        None => default_keypad_map(),
                    },
                    hotkeys: match hotkeys {
                        Some(hotkeys) => hotkeys.0,
                        None => default_hotkey_map(),
                    },
                })
            }
        }

        deserializer.deserialize_map(KeymapVisitor)
    }
}
