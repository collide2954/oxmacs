use std::fmt;
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

    pub fn unshifted(self) -> Option<Self> {
        if self.modifiers.contains(Modifiers::SHIFT) {
            return Some(Self {
                modifiers: self.modifiers - Modifiers::SHIFT,
                ..self
            });
        }
        let chords = Modifiers::CONTROL | Modifiers::META | Modifiers::SUPER;
        if let KeyCode::Char(c) = self.code
            && self.modifiers.intersects(chords)
            && let Some(lower) = single_lowercase(c)
        {
            return Some(Self {
                code: KeyCode::Char(lower),
                ..self
            });
        }
        None
    }
}

fn single_lowercase(c: char) -> Option<char> {
    if !c.is_uppercase() {
        return None;
    }
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => Some(l),
        _ => None,
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

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefixes = [
            (Modifiers::CONTROL, "C-"),
            (Modifiers::META, "M-"),
            (Modifiers::SHIFT, "S-"),
            (Modifiers::SUPER, "s-"),
        ];
        for (modifier, prefix) in prefixes {
            if self.modifiers.contains(modifier) {
                f.write_str(prefix)?;
            }
        }
        self.code.fmt(f)
    }
}

impl fmt::Display for KeyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Char(' ') => "SPC",
            Self::Char(c) => return write!(f, "{c}"),
            Self::F(number) => return write!(f, "<f{number}>"),
            Self::Return => "RET",
            Self::Tab => "TAB",
            Self::Backspace => "DEL",
            Self::Escape => "ESC",
            Self::Delete => "<delete>",
            Self::Insert => "<insert>",
            Self::Left => "<left>",
            Self::Right => "<right>",
            Self::Up => "<up>",
            Self::Down => "<down>",
            Self::Home => "<home>",
            Self::End => "<end>",
            Self::PageUp => "<prior>",
            Self::PageDown => "<next>",
        };
        f.write_str(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeySequence(Vec<Key>);

impl KeySequence {
    pub fn keys(&self) -> &[Key] {
        &self.0
    }
}

impl From<Vec<Key>> for KeySequence {
    fn from(keys: Vec<Key>) -> Self {
        Self(keys)
    }
}

impl FromStr for KeySequence {
    type Err = KeyParseError;

    fn from_str(notation: &str) -> Result<Self, Self::Err> {
        let keys = notation
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<Vec<Key>, _>>()?;
        if keys.is_empty() {
            return Err(KeyParseError::Empty);
        }
        Ok(Self(keys))
    }
}

impl fmt::Display for KeySequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, key) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(" ")?;
            }
            key.fmt(f)?;
        }
        Ok(())
    }
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

    #[test]
    fn display_round_trips_canonical_notation() {
        let notations = [
            "a",
            "C-x",
            "C-M-%",
            "s-a",
            "S-<left>",
            "C-M-S-s-<f5>",
            "RET",
            "TAB",
            "DEL",
            "ESC",
            "SPC",
            "C-SPC",
            "C--",
            "<delete>",
            "<prior>",
            "<next>",
            "<home>",
            "M-<",
        ];
        for notation in notations {
            let key: Key = notation.parse().unwrap();
            assert_eq!(key.to_string(), notation);
        }
    }

    #[test]
    fn key_sequence_parses_space_separated_keys() {
        let sequence: KeySequence = "C-x  C-f".parse().unwrap();

        assert_eq!(
            sequence.keys(),
            [
                key(KeyCode::Char('x'), Modifiers::CONTROL),
                key(KeyCode::Char('f'), Modifiers::CONTROL),
            ]
        );
        assert_eq!(sequence.to_string(), "C-x C-f");
    }

    fn unshifted(notation: &str) -> Option<String> {
        notation
            .parse::<Key>()
            .unwrap()
            .unshifted()
            .map(|key| key.to_string())
    }

    #[test]
    fn unshifting_removes_shift_from_named_keys() {
        assert_eq!(unshifted("S-<left>").as_deref(), Some("<left>"));
        assert_eq!(unshifted("C-S-<right>").as_deref(), Some("C-<right>"));
    }

    #[test]
    fn unshifting_lowercases_chorded_letters() {
        assert_eq!(unshifted("C-S-f").as_deref(), Some("C-f"));
        assert_eq!(unshifted("M-S-f").as_deref(), Some("M-f"));
    }

    #[test]
    fn unshifting_leaves_unshifted_and_typed_keys_alone() {
        assert_eq!(unshifted("A"), None);
        assert_eq!(unshifted("C-f"), None);
        assert_eq!(unshifted("<left>"), None);
    }

    #[test]
    fn key_sequence_rejects_empty_and_invalid_input() {
        assert_eq!("  ".parse::<KeySequence>(), Err(KeyParseError::Empty));
        assert_eq!(
            "C-x <nope>".parse::<KeySequence>(),
            Err(KeyParseError::UnknownKey("<nope>".to_owned()))
        );
    }
}
