use super::PressedKey;
use serde::{de::IntoDeserializer, Deserialize, Serialize};
use std::{
    error::Error,
    fmt::{self, Write},
    str::FromStr,
};
use winit::event::{ScanCode, VirtualKeyCode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    And,
    Or,
    Xor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "&str", into = "String")]
pub enum Trigger {
    KeyCode(VirtualKeyCode),
    // TODO: Proper keyboard key to character conversion; right now winit doesn't support reading
    // the keyboard layout or the character corresponding to a key other than through virtual key
    // code mapping
    ScanCode(ScanCode, Option<VirtualKeyCode>),
    Not(Box<Trigger>),
    Chain(Op, Vec<Trigger>),
}

impl Trigger {
    pub(super) fn activated<'a>(
        &self,
        pressed_keys: impl IntoIterator<Item = &'a PressedKey> + Copy,
    ) -> bool {
        match self {
            Trigger::KeyCode(keycode) => {
                pressed_keys.into_iter().any(|key| key.0 == Some(*keycode))
            }
            Trigger::ScanCode(scancode, _) => {
                pressed_keys.into_iter().any(|key| key.1 == *scancode)
            }
            Trigger::Not(trigger) => !trigger.activated(pressed_keys),
            Trigger::Chain(op, triggers) => match op {
                Op::And => triggers
                    .iter()
                    .all(|trigger| trigger.activated(pressed_keys)),
                Op::Or => triggers
                    .iter()
                    .any(|trigger| trigger.activated(pressed_keys)),
                Op::Xor => triggers
                    .iter()
                    .fold(false, |res, trigger| res ^ trigger.activated(pressed_keys)),
            },
        }
    }
}

impl ToString for Trigger {
    fn to_string(&self) -> String {
        fn write_trigger(result: &mut String, trigger: &Trigger, needs_parens_if_multiple: bool) {
            match trigger {
                &Trigger::KeyCode(key_code) => {
                    write!(result, "v{:?}", key_code).unwrap();
                }
                &Trigger::ScanCode(scan_code, key_code) => {
                    write!(result, "s{}v{:?}", scan_code, key_code).unwrap();
                }
                Trigger::Not(trigger) => {
                    result.push('!');
                    write_trigger(result, trigger, true);
                }
                Trigger::Chain(op, triggers) => {
                    if needs_parens_if_multiple {
                        result.push('(');
                    }
                    let op_str = match op {
                        Op::And => " & ",
                        Op::Or => " | ",
                        Op::Xor => " ^ ",
                    };
                    for (i, trigger) in triggers.iter().enumerate() {
                        if i != 0 {
                            result.push_str(op_str);
                        }
                        write_trigger(result, trigger, true);
                    }
                    if needs_parens_if_multiple {
                        result.push(')');
                    }
                }
            }
        }

        let mut result = String::new();
        write_trigger(&mut result, self, false);
        result
    }
}

impl From<Trigger> for String {
    fn from(trigger: Trigger) -> Self {
        trigger.to_string()
    }
}

pub struct ParseError;

impl Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("parse error")
    }
}

impl fmt::Debug for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <Self as fmt::Display>::fmt(self, f)
    }
}

impl FromStr for Trigger {
    type Err = ParseError;

    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        fn parse_key_code(s: &mut &str) -> Result<VirtualKeyCode, ParseError> {
            let end_index = s
                .char_indices()
                .find_map(|(i, c)| if c.is_alphanumeric() { None } else { Some(i) })
                .unwrap_or(s.len());
            let key_code_str = &s[..end_index];
            *s = &s[end_index..];

            VirtualKeyCode::deserialize(key_code_str.into_deserializer())
                .map_err(|_: serde::de::value::Error| ParseError)
        }

        fn parse_value(s: &mut &str) -> Result<Trigger, ParseError> {
            let mut negate = false;
            let mut operator = None;
            let mut values = Vec::new();

            loop {
                *s = s.trim_start();

                let mut char_indices = s.char_indices();
                let next_char = char_indices.next().map(|(_, c)| c);

                if next_char == Some(')') || next_char.is_none() {
                    if let Some(operator) = operator {
                        if values.len() <= 1 {
                            return Err(ParseError);
                        }
                        return Ok(Trigger::Chain(operator, values));
                    } else {
                        if values.len() != 1 {
                            return Err(ParseError);
                        }
                        return Ok(values.remove(0));
                    }
                }

                if let Some((new_start_index, _)) = char_indices.next() {
                    *s = &s[new_start_index..];
                }

                let value = match next_char {
                    Some('!') => {
                        negate = !negate;
                        continue;
                    }

                    Some('&') => {
                        operator = Some(Op::And);
                        continue;
                    }

                    Some('|') => {
                        operator = Some(Op::Or);
                        continue;
                    }

                    Some('^') => {
                        operator = Some(Op::Xor);
                        continue;
                    }

                    Some('v') => Trigger::KeyCode(parse_key_code(s)?),

                    Some('s') => {
                        let mut char_indices = s.char_indices();
                        let (scan_code_end_index, scan_code_end_char) = char_indices
                            .find_map(|(i, c)| {
                                if c.is_numeric() {
                                    None
                                } else {
                                    Some((i, Some(c)))
                                }
                            })
                            .unwrap_or((s.len(), None));
                        let scan_code_str = &s[..scan_code_end_index];
                        *s = &s[scan_code_end_index..];

                        let scan_code =
                            ScanCode::from_str(scan_code_str).map_err(|_| ParseError)?;

                        let virtual_key_code = match scan_code_end_char {
                            Some('v') => Some(parse_key_code(s)?),
                            Some(c) if c.is_alphanumeric() => return Err(ParseError),
                            _ => None,
                        };

                        Trigger::ScanCode(scan_code, virtual_key_code)
                    }

                    Some('(') => {
                        let value = parse_value(s)?;
                        *s = s.strip_prefix(')').ok_or(ParseError)?;
                        value
                    }

                    _ => return Err(ParseError),
                };

                values.push(if negate {
                    Trigger::Not(Box::new(value))
                } else {
                    value
                });
                negate = false;
            }
        }

        parse_value(&mut s)
    }
}

impl TryFrom<&str> for Trigger {
    type Error = ParseError;
    fn try_from(str: &str) -> Result<Self, Self::Error> {
        Self::from_str(str)
    }
}

impl Trigger {
    pub fn option_from_str(s: &str) -> Result<Option<Self>, ParseError> {
        if s.chars().all(char::is_whitespace) {
            Ok(None)
        } else {
            Trigger::from_str(s).map(Some)
        }
    }
}
