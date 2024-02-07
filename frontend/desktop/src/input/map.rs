use super::{
    trigger::{Op, Trigger},
    Action,
};
use crate::config::SettingOrigin;
use ahash::AHashMap as HashMap;
use dust_core::emu::input::Keys;
use serde::{
    de::{MapAccess, Visitor},
    ser::SerializeMap,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{fmt, hash::Hash};
use winit::keyboard::KeyCode;

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
    (
        Action::ToggleFullWindowScreen,
        "toggle-whole-window-screen-drawing",
    ),
    (Action::ToggleSyncToAudio, "toggle-sync-to-audio"),
    (Action::ToggleFramerateLimit, "toggle-framerate-limit"),
];

#[derive(Clone)]
pub struct Map {
    pub keypad: HashMap<Keys, Option<Trigger>>,
    pub hotkeys: HashMap<Action, Option<Trigger>>,
}

impl Map {
    pub fn empty() -> Self {
        Map {
            keypad: HashMap::new(),
            hotkeys: HashMap::new(),
        }
    }
}

fn default_keypad_map() -> HashMap<Keys, Option<Trigger>> {
    [
        (Keys::A, Some(Trigger::KeyCode(KeyCode::KeyX.into()))),
        (Keys::B, Some(Trigger::KeyCode(KeyCode::KeyZ.into()))),
        (Keys::X, Some(Trigger::KeyCode(KeyCode::KeyS.into()))),
        (Keys::Y, Some(Trigger::KeyCode(KeyCode::KeyA.into()))),
        (Keys::L, Some(Trigger::KeyCode(KeyCode::KeyQ.into()))),
        (Keys::R, Some(Trigger::KeyCode(KeyCode::KeyW.into()))),
        (Keys::START, Some(Trigger::KeyCode(KeyCode::Enter.into()))),
        (
            Keys::SELECT,
            Some(Trigger::Chain(
                Op::Or,
                vec![
                    Trigger::KeyCode(KeyCode::ShiftLeft.into()),
                    Trigger::KeyCode(KeyCode::ShiftRight.into()),
                ],
            )),
        ),
        (
            Keys::RIGHT,
            Some(Trigger::KeyCode(KeyCode::ArrowRight.into())),
        ),
        (
            Keys::LEFT,
            Some(Trigger::KeyCode(KeyCode::ArrowLeft.into())),
        ),
        (Keys::UP, Some(Trigger::KeyCode(KeyCode::ArrowUp.into()))),
        (
            Keys::DOWN,
            Some(Trigger::KeyCode(KeyCode::ArrowDown.into())),
        ),
        (Keys::DEBUG, None),
    ]
    .into_iter()
    .collect()
}

fn default_hotkey_map() -> HashMap<Action, Option<Trigger>> {
    [
        (Action::PlayPause, None),
        (Action::Reset, None),
        (Action::Stop, None),
        (Action::ToggleFullWindowScreen, None),
        (Action::ToggleSyncToAudio, None),
        (Action::ToggleFramerateLimit, None),
    ]
    .into_iter()
    .collect()
}

impl Default for Map {
    fn default() -> Self {
        Map {
            keypad: default_keypad_map(),
            hotkeys: default_hotkey_map(),
        }
    }
}

impl Serialize for Map {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;

        struct TriggerMap<'a, T: 'static + Eq, U: 'static + Serialize>(
            &'a HashMap<T, U>,
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

        let mut map = serializer.serialize_struct("Map", 2)?;
        map.serialize_field("keypad", &TriggerMap(&self.keypad, KEY_IDENTS))?;
        map.serialize_field("hotkeys", &TriggerMap(&self.hotkeys, ACTION_IDENTS))?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for Map {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TriggerMapVisitor<T: 'static + Eq + Hash>(
            &'static [(T, &'static str)],
            &'static str,
        );

        impl<'de, T: 'static + Eq + Hash + Copy> Visitor<'de> for TriggerMapVisitor<T> {
            type Value = HashMap<T, Option<Trigger>>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(self.1)
            }

            fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<Self::Value, M::Error> {
                let mut map = HashMap::with_capacity_and_hasher(
                    access.size_hint().unwrap_or(0),
                    Default::default(),
                );

                while let Some((ident, value)) = access.next_entry::<&str, Option<Trigger>>()? {
                    if let Some((key, _)) = self.0.iter().find(|(_, ident_)| *ident_ == ident) {
                        map.insert(*key, value);
                    }
                }

                Ok(map)
            }
        }

        struct DeserializedKeypadMap(HashMap<Keys, Option<Trigger>>);

        impl<'de> Deserialize<'de> for DeserializedKeypadMap {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer
                    .deserialize_map(TriggerMapVisitor::<Keys>(
                        KEY_IDENTS,
                        "a map of triggers corresponding to keypad keys",
                    ))
                    .map(Self)
            }
        }

        struct DeserializedHotkeyMap(HashMap<Action, Option<Trigger>>);

        impl<'de> Deserialize<'de> for DeserializedHotkeyMap {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer
                    .deserialize_map(TriggerMapVisitor::<Action>(
                        ACTION_IDENTS,
                        "a map of triggers corresponding to action identifiers",
                    ))
                    .map(Self)
            }
        }

        struct MapVisitor;

        impl<'de> Visitor<'de> for MapVisitor {
            type Value = Map;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an input map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut keypad = None;
                let mut hotkeys = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        "keypad" => keypad = Some(map.next_value::<DeserializedKeypadMap>()?.0),
                        "hotkeys" => hotkeys = Some(map.next_value::<DeserializedHotkeyMap>()?.0),
                        _ => {}
                    }
                }
                Ok(Map {
                    keypad: keypad.unwrap_or_default(),
                    hotkeys: hotkeys.unwrap_or_default(),
                })
            }
        }

        deserializer.deserialize_map(MapVisitor)
    }
}

impl Map {
    pub fn merge(mut self, b: Map) -> Map {
        for (key, trigger) in b.keypad {
            self.keypad.insert(key, trigger);
        }
        for (action, trigger) in b.hotkeys {
            self.hotkeys.insert(action, trigger);
        }
        self
    }

    pub fn resolve(global: &GlobalMap, game: &Map) -> (Self, SettingOrigin) {
        (
            Self::merge(global.0.clone(), game.clone()),
            SettingOrigin::Game,
        )
    }
}

#[derive(Clone, Default)]
pub struct GlobalMap(pub Map);

impl Serialize for GlobalMap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for GlobalMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(GlobalMap(
            Map::default().merge(Map::deserialize(deserializer)?),
        ))
    }
}
