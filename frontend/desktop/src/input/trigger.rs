use super::{KeyCode, PressedKey, ScanCode};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt::{self, Write},
    str::FromStr,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    And,
    Or,
    Xor,
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Op::And => f.write_char('&'),
            Op::Or => f.write_char('|'),
            Op::Xor => f.write_char('^'),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(try_from = "&str", into = "String")]
pub enum Trigger {
    KeyCode(KeyCode),
    ScanCode(ScanCode),
    Not(Box<Trigger>),
    Chain(Op, Vec<Trigger>),
}

impl Trigger {
    pub fn activated<'a>(
        &self,
        pressed_keys: impl IntoIterator<Item = &'a PressedKey> + Copy,
    ) -> bool {
        match self {
            Trigger::KeyCode(key_code) => pressed_keys
                .into_iter()
                .any(|key| matches!(key, PressedKey::KeyCode(key_code_) if key_code_ == key_code)),
            Trigger::ScanCode(scan_code) => pressed_keys.into_iter().any(
                |key| matches!(key, PressedKey::ScanCode(scan_code_) if scan_code_ == scan_code),
            ),
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

#[allow(clippy::to_string_trait_impl)]
impl ToString for Trigger {
    fn to_string(&self) -> String {
        fn write_trigger(result: &mut String, trigger: &Trigger, needs_parens_if_multiple: bool) {
            match trigger {
                &Trigger::KeyCode(key_code) => {
                    write!(result, "v{}", <&str>::from(key_code)).unwrap();
                }
                &Trigger::ScanCode(scan_code) => {
                    write!(result, "s{}", scan_code.to_string()).unwrap();
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
    fn from(value: Trigger) -> Self {
        value.to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseErrorKind {
    UnexpectedCharacter,
    UnexpectedClosingParen,
    InvalidKeyScanCode,
    ExpectedValue,
    UnexpectedValue,
    UnexpectedUnaryOperator,
    MismatchedOperators { expected: Op, found: Op },
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedCharacter => f.write_str("unexpected character"),
            Self::UnexpectedClosingParen => f.write_str("unexpected closing parens"),
            Self::InvalidKeyScanCode => f.write_str("invalid key/scan code"),
            Self::ExpectedValue => f.write_str("expected value"),
            Self::UnexpectedValue => f.write_str("unexpected value"),
            Self::UnexpectedUnaryOperator => f.write_str("unexpected unary operator after values"),
            Self::MismatchedOperators { expected, found } => write!(
                f,
                "mismatched operators: expected {}, found {}",
                expected, found
            ),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ParseError {
    pos: usize,
    kind: ParseErrorKind,
}

impl Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error at character {}: {}", self.pos, self.kind)
    }
}

impl fmt::Debug for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <Self as fmt::Display>::fmt(self, f)
    }
}

struct TriggerParser<'a> {
    s: &'a str,
    pos: usize,
    new_pos: usize,
}

impl<'a> TriggerParser<'a> {
    fn parse(s: &'a str) -> Result<Trigger, ParseError> {
        TriggerParser {
            s,
            pos: 0,
            new_pos: 0,
        }
        .parse_trigger(false)
    }

    fn consume_char(&mut self) -> Option<char> {
        let mut char_indices = self.s.char_indices();
        let next_char = char_indices.next().map(|(_, c)| c);
        let end_index = match char_indices.next() {
            Some((i, _)) => i,
            None => self.s.len(),
        };
        self.s = &self.s[end_index..];
        self.new_pos += end_index;
        next_char
    }

    fn commit(&mut self) {
        self.pos = self.new_pos;
    }

    fn validate_and_change_op(
        &mut self,
        new_op: Op,
        op: &mut Option<Op>,
        expects_value: &mut bool,
    ) -> Result<(), ParseError> {
        if *expects_value {
            return Err(ParseError {
                pos: self.pos,
                kind: ParseErrorKind::ExpectedValue,
            });
        }

        if let Some(op) = *op {
            if op != new_op {
                return Err(ParseError {
                    pos: self.pos,
                    kind: ParseErrorKind::MismatchedOperators {
                        expected: new_op,
                        found: op,
                    },
                });
            }
        }

        *op = Some(new_op);
        *expects_value = true;

        self.commit();

        Ok(())
    }

    fn parse_value<T: std::str::FromStr>(&mut self) -> Result<T, ParseError> {
        let end_index = self
            .s
            .char_indices()
            .find_map(|(i, c)| (!c.is_alphanumeric()).then_some(i))
            .unwrap_or(self.s.len());
        let value_str = &self.s[..end_index];
        self.s = &self.s[end_index..];
        self.new_pos += end_index;

        let result = value_str.parse().map_err(|_| ParseError {
            pos: self.pos,
            kind: ParseErrorKind::InvalidKeyScanCode,
        })?;

        self.commit();

        Ok(result)
    }

    fn parse_trigger(&mut self, expect_parens: bool) -> Result<Trigger, ParseError> {
        let mut negate = false;
        let mut op = None;
        let mut values = vec![];
        let mut expects_value = true;

        loop {
            self.s = self.s.trim_start();

            let next_char = self.consume_char();
            if next_char == Some(')') {
                if !expect_parens {
                    return Err(ParseError {
                        pos: self.pos,
                        kind: ParseErrorKind::UnexpectedClosingParen,
                    });
                }
                self.commit();
            }

            if next_char == Some(')') || next_char.is_none() {
                if expects_value {
                    return Err(ParseError {
                        pos: self.pos,
                        kind: ParseErrorKind::ExpectedValue,
                    });
                }

                if let Some(op) = op {
                    return Ok(Trigger::Chain(op, values));
                } else {
                    return Ok(values.remove(0));
                }
            }
            let next_char = next_char.unwrap();

            match next_char {
                '!' => {
                    if !expects_value {
                        return Err(ParseError {
                            pos: self.pos,
                            kind: ParseErrorKind::UnexpectedUnaryOperator,
                        });
                    }
                    negate = !negate;
                    self.commit();
                    continue;
                }

                '&' => {
                    self.validate_and_change_op(Op::And, &mut op, &mut expects_value)?;
                    continue;
                }

                '|' => {
                    self.validate_and_change_op(Op::Or, &mut op, &mut expects_value)?;
                    continue;
                }

                '^' => {
                    self.validate_and_change_op(Op::Xor, &mut op, &mut expects_value)?;
                    continue;
                }

                _ => {}
            }

            if !matches!(next_char, 'v' | 's' | '(') {
                return Err(ParseError {
                    pos: self.pos,
                    kind: ParseErrorKind::UnexpectedCharacter,
                });
            }

            if !expects_value {
                return Err(ParseError {
                    pos: self.pos,
                    kind: ParseErrorKind::UnexpectedValue,
                });
            }

            let trigger = match next_char {
                'v' => Trigger::KeyCode(self.parse_value::<KeyCode>()?),
                's' => Trigger::ScanCode(self.parse_value::<ScanCode>()?),
                '(' => {
                    self.commit();
                    self.parse_trigger(true)?
                }
                _ => unreachable!(),
            };
            values.push(trigger);
            expects_value = false;
        }
    }
}

impl FromStr for Trigger {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TriggerParser::parse(s)
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
