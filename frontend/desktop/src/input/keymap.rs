use super::trigger::{self, Trigger};
use core::fmt;
use dust_core::emu::input::Keys;
use fxhash::FxHashMap;
use serde::{
    de::{MapAccess, Visitor},
    ser::SerializeMap,
    Deserialize, Deserializer, Serialize, Serializer,
};
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

#[derive(Clone, Debug)]
pub struct Keymap(pub FxHashMap<Keys, Trigger>);

impl Default for Keymap {
    fn default() -> Self {
        Keymap(
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
            .collect(),
        )
    }
}

impl Serialize for Keymap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            if let Some((_, ident)) = KEY_IDENTS.iter().find(|(key_, _)| key_ == key) {
                map.serialize_entry(*ident, value)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Keymap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapVisitor;

        impl<'de> Visitor<'de> for MapVisitor {
            type Value = Keymap;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map of key identifiers to triggers")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<Self::Value, M::Error> {
                let mut map = FxHashMap::with_capacity_and_hasher(
                    access.size_hint().unwrap_or(0),
                    Default::default(),
                );

                while let Some((ident, value)) = access.next_entry::<&str, Trigger>()? {
                    if let Some((key, _)) = KEY_IDENTS.iter().find(|(_, ident_)| *ident_ == ident) {
                        map.insert(*key, value);
                    }
                }

                Ok(Keymap(map))
            }
        }

        deserializer.deserialize_map(MapVisitor)
    }
}
