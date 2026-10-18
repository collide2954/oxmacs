use std::str::FromStr;

use bitflags::bitflags;
use thiserror::Error;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Modifiers: u8 {
        const CONTROL = 1;
        const META = 1 << 1;
        const SHIFT = 1 << 2;
        const SUPER = 1 << 3;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    Return,
    Tab,
    Backspace,
    Delete,
    Escape,
    Insert,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    code: KeyCode,
    modifiers: Modifiers,
}

impl Key {
    pub fn new(code: KeyCode, modifiers: Modifiers) -> Self {
        if let KeyCode::Char(c) = code
            && modifiers.contains(Modifiers::SHIFT)
            && let Some(upper) = single_uppercase(c)
        {
            return Self {
                code: KeyCode::Char(upper),
                modifiers: modifiers - Modifiers::SHIFT,
            };
        }
        Self { code, modifiers }
    }

    pub fn code(self) -> KeyCode {
        self.code
    }

    pub fn modifiers(self) -> Modifiers {
        self.modifiers
    }
}

fn single_uppercase(c: char) -> Option<char> {
    if !c.is_lowercase() {
        return None;
    }
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => Some(u),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum KeyParseError {
    #[error("empty key")]
    Empty,
    #[error("unknown modifier `{0}-`")]
    UnknownModifier(char),
    #[error("unknown key `{0}`")]
    UnknownKey(String),
}

impl FromStr for Key {
    type Err = KeyParseError;

    fn from_str(notation: &str) -> Result<Self, Self::Err> {
        if notation.is_empty() {
            return Err(KeyParseError::Empty);
        }
        let mut modifiers = Modifiers::empty();
        let mut rest = notation;
        while let Some((prefix, tail)) = split_modifier(rest) {
            modifiers |= modifier(prefix)?;
            rest = tail;
        }
        let code = key_code(rest).ok_or_else(|| KeyParseError::UnknownKey(notation.to_owned()))?;
        Ok(Self::new(code, modifiers))
    }
}

fn split_modifier(notation: &str) -> Option<(char, &str)> {
    let mut chars = notation.chars();
    let prefix = chars.next()?;
    let tail = chars.as_str().strip_prefix('-')?;
    (!tail.is_empty()).then_some((prefix, tail))
}

fn modifier(prefix: char) -> Result<Modifiers, KeyParseError> {
    match prefix {
        'C' => Ok(Modifiers::CONTROL),
        'M' => Ok(Modifiers::META),
        'S' => Ok(Modifiers::SHIFT),
        's' => Ok(Modifiers::SUPER),
        other => Err(KeyParseError::UnknownModifier(other)),
    }
}

fn key_code(name: &str) -> Option<KeyCode> {
    let code = match name {
        "RET" | "<return>" => KeyCode::Return,
        "TAB" | "<tab>" => KeyCode::Tab,
        "DEL" | "<backspace>" => KeyCode::Backspace,
        "ESC" | "<escape>" => KeyCode::Escape,
        "SPC" => KeyCode::Char(' '),
        "<delete>" | "<deletechar>" => KeyCode::Delete,
        "<insert>" => KeyCode::Insert,
        "<left>" => KeyCode::Left,
        "<right>" => KeyCode::Right,
        "<up>" => KeyCode::Up,
        "<down>" => KeyCode::Down,
        "<home>" => KeyCode::Home,
        "<end>" => KeyCode::End,
        "<prior>" => KeyCode::PageUp,
        "<next>" => KeyCode::PageDown,
        _ => return function_key(name).or_else(|| single_char(name)),
    };
    Some(code)
}

fn function_key(name: &str) -> Option<KeyCode> {
    let number: u8 = name.strip_prefix("<f")?.strip_suffix('>')?.parse().ok()?;
    (1..=35).contains(&number).then_some(KeyCode::F(number))
}

fn single_char(name: &str) -> Option<KeyCode> {
    let mut chars = name.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(KeyCode::Char(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: Modifiers) -> Key {
        Key::new(code, modifiers)
    }

    #[test]
    fn parses_plain_characters() {
        assert_eq!("a".parse(), Ok(key(KeyCode::Char('a'), Modifiers::empty())));
        assert_eq!("é".parse(), Ok(key(KeyCode::Char('é'), Modifiers::empty())));
    }

    #[test]
    fn parses_modifier_prefixes() {
        assert_eq!(
            "C-x".parse(),
            Ok(key(KeyCode::Char('x'), Modifiers::CONTROL))
        );
        assert_eq!(
            "C-M-%".parse(),
            Ok(key(
                KeyCode::Char('%'),
                Modifiers::CONTROL | Modifiers::META
            ))
        );
        assert_eq!("s-a".parse(), Ok(key(KeyCode::Char('a'), Modifiers::SUPER)));
        assert_eq!("S-<left>".parse(), Ok(key(KeyCode::Left, Modifiers::SHIFT)));
    }

    #[test]
    fn parses_dash_as_a_key() {
        assert_eq!(
            "C--".parse(),
            Ok(key(KeyCode::Char('-'), Modifiers::CONTROL))
        );
        assert_eq!("-".parse(), Ok(key(KeyCode::Char('-'), Modifiers::empty())));
    }

    #[test]
    fn parses_named_keys() {
        let cases = [
            ("RET", KeyCode::Return),
            ("TAB", KeyCode::Tab),
            ("DEL", KeyCode::Backspace),
            ("ESC", KeyCode::Escape),
            ("SPC", KeyCode::Char(' ')),
            ("<delete>", KeyCode::Delete),
            ("<left>", KeyCode::Left),
            ("<right>", KeyCode::Right),
            ("<up>", KeyCode::Up),
            ("<down>", KeyCode::Down),
            ("<home>", KeyCode::Home),
            ("<end>", KeyCode::End),
            ("<prior>", KeyCode::PageUp),
            ("<next>", KeyCode::PageDown),
            ("<insert>", KeyCode::Insert),
            ("<f1>", KeyCode::F(1)),
            ("<f12>", KeyCode::F(12)),
        ];
        for (notation, code) in cases {
            assert_eq!(
                notation.parse(),
                Ok(key(code, Modifiers::empty())),
                "{notation}"
            );
        }
    }

    #[test]
    fn shift_with_letter_folds_into_uppercase() {
        assert_eq!(
            "C-S-a".parse(),
            Ok(key(KeyCode::Char('A'), Modifiers::CONTROL))
        );
    }

    #[test]
    fn rejects_malformed_keys() {
        assert_eq!("".parse::<Key>(), Err(KeyParseError::Empty));
        assert_eq!(
            "Q-x".parse::<Key>(),
            Err(KeyParseError::UnknownModifier('Q'))
        );
        assert_eq!(
            "<bogus>".parse::<Key>(),
            Err(KeyParseError::UnknownKey("<bogus>".to_owned()))
        );
        assert_eq!(
            "C-".parse::<Key>(),
            Err(KeyParseError::UnknownKey("C-".to_owned()))
        );
    }
}
