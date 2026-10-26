use iced::keyboard::key::Named;
use iced::keyboard::{Key as IcedKey, Modifiers as IcedModifiers};
use oxmacs_core::input::{Key, KeyCode, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Other,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Other
        }
    }
}

pub fn translate(
    key: &IcedKey,
    modified_key: &IcedKey,
    modifiers: IcedModifiers,
    platform: Platform,
) -> Option<Key> {
    let mut chord = Modifiers::empty();
    chord.set(Modifiers::CONTROL, modifiers.control());
    chord.set(Modifiers::META, modifiers.alt());
    chord.set(Modifiers::SHIFT, modifiers.shift());
    chord.set(Modifiers::SUPER, modifiers.logo());
    match key {
        IcedKey::Named(named) => Some(Key::new(named_code(*named)?, chord)),
        IcedKey::Character(_) if platform == Platform::MacOs && modifiers.alt() => {
            Some(Key::new(KeyCode::Char(single_char(key)?), chord))
        }
        IcedKey::Character(_) => Some(Key::new(
            KeyCode::Char(single_char(modified_key)?),
            chord - Modifiers::SHIFT,
        )),
        IcedKey::Unidentified => None,
    }
}

pub fn starts_prefix(key: &IcedKey, modifiers: IcedModifiers) -> bool {
    modifiers.control() && matches!(key, IcedKey::Character(text) if text.eq_ignore_ascii_case("x"))
}

pub fn game_key(key: &IcedKey) -> Option<String> {
    let name = match key {
        IcedKey::Named(named) => match named {
            Named::ArrowLeft => "left",
            Named::ArrowRight => "right",
            Named::ArrowUp => "up",
            Named::ArrowDown => "down",
            Named::Space => "space",
            Named::Enter => "enter",
            Named::Escape => "escape",
            Named::Tab => "tab",
            Named::Backspace => "backspace",
            Named::Shift => "shift",
            Named::Control => "control",
            Named::Alt => "alt",
            _ => return None,
        },
        IcedKey::Character(_) => return single_char(key).map(|c| c.to_lowercase().collect()),
        IcedKey::Unidentified => return None,
    };
    Some(name.to_owned())
}

fn single_char(key: &IcedKey) -> Option<char> {
    let IcedKey::Character(text) = key else {
        return None;
    };
    let mut chars = text.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

fn named_code(named: Named) -> Option<KeyCode> {
    let code = match named {
        Named::Enter => KeyCode::Return,
        Named::Tab => KeyCode::Tab,
        Named::Backspace => KeyCode::Backspace,
        Named::Delete => KeyCode::Delete,
        Named::Escape => KeyCode::Escape,
        Named::Insert => KeyCode::Insert,
        Named::Space => KeyCode::Char(' '),
        Named::ArrowLeft => KeyCode::Left,
        Named::ArrowRight => KeyCode::Right,
        Named::ArrowUp => KeyCode::Up,
        Named::ArrowDown => KeyCode::Down,
        Named::Home => KeyCode::Home,
        Named::End => KeyCode::End,
        Named::PageUp => KeyCode::PageUp,
        Named::PageDown => KeyCode::PageDown,
        Named::F1 => KeyCode::F(1),
        Named::F2 => KeyCode::F(2),
        Named::F3 => KeyCode::F(3),
        Named::F4 => KeyCode::F(4),
        Named::F5 => KeyCode::F(5),
        Named::F6 => KeyCode::F(6),
        Named::F7 => KeyCode::F(7),
        Named::F8 => KeyCode::F(8),
        Named::F9 => KeyCode::F(9),
        Named::F10 => KeyCode::F(10),
        Named::F11 => KeyCode::F(11),
        Named::F12 => KeyCode::F(12),
        _ => return None,
    };
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: IcedModifiers = IcedModifiers::empty();
    const CTRL: IcedModifiers = IcedModifiers::CTRL;
    const ALT: IcedModifiers = IcedModifiers::ALT;
    const SHIFT: IcedModifiers = IcedModifiers::SHIFT;
    const LOGO: IcedModifiers = IcedModifiers::LOGO;

    fn chord(
        key: &str,
        modified: &str,
        modifiers: IcedModifiers,
        platform: Platform,
    ) -> Option<String> {
        let key = IcedKey::Character(key.into());
        let modified = IcedKey::Character(modified.into());
        translate(&key, &modified, modifiers, platform).map(|key| key.to_string())
    }

    fn named(named: Named, modifiers: IcedModifiers) -> Option<String> {
        let key = IcedKey::Named(named);
        translate(&key, &key, modifiers, Platform::Other).map(|key| key.to_string())
    }

    #[test]
    fn plain_characters() {
        assert_eq!(chord("a", "a", NONE, Platform::Other).as_deref(), Some("a"));
    }

    #[test]
    fn shift_is_folded_into_the_produced_character() {
        assert_eq!(
            chord("a", "A", SHIFT, Platform::Other).as_deref(),
            Some("A")
        );
        assert_eq!(
            chord(",", "<", SHIFT, Platform::Other).as_deref(),
            Some("<")
        );
    }

    #[test]
    fn control_and_alt_become_control_and_meta() {
        assert_eq!(
            chord("x", "x", CTRL, Platform::Other).as_deref(),
            Some("C-x")
        );
        assert_eq!(
            chord(",", "<", ALT | SHIFT, Platform::Other).as_deref(),
            Some("M-<")
        );
    }

    #[test]
    fn logo_becomes_super() {
        assert_eq!(
            chord("a", "a", LOGO, Platform::Other).as_deref(),
            Some("s-a")
        );
    }

    #[test]
    fn macos_option_acts_as_meta_instead_of_composing() {
        assert_eq!(
            chord("f", "ƒ", ALT, Platform::MacOs).as_deref(),
            Some("M-f")
        );
    }

    #[test]
    fn named_keys() {
        let cases = [
            (Named::Enter, NONE, "RET"),
            (Named::Tab, NONE, "TAB"),
            (Named::Backspace, NONE, "DEL"),
            (Named::Delete, NONE, "<delete>"),
            (Named::Escape, NONE, "ESC"),
            (Named::Space, CTRL, "C-SPC"),
            (Named::ArrowLeft, CTRL, "C-<left>"),
            (Named::ArrowRight, SHIFT, "S-<right>"),
            (Named::Home, NONE, "<home>"),
            (Named::PageDown, NONE, "<next>"),
            (Named::F5, NONE, "<f5>"),
        ];
        for (key, modifiers, notation) in cases {
            assert_eq!(
                named(key, modifiers).as_deref(),
                Some(notation),
                "{notation}"
            );
        }
    }

    #[test]
    fn games_hear_keys_by_plain_names() {
        let name = |key: IcedKey| game_key(&key);
        assert_eq!(
            name(IcedKey::Named(Named::ArrowLeft)).as_deref(),
            Some("left")
        );
        assert_eq!(name(IcedKey::Named(Named::Space)).as_deref(), Some("space"));
        assert_eq!(name(IcedKey::Character("Z".into())).as_deref(), Some("z"));
        assert_eq!(name(IcedKey::Named(Named::F1)), None);
        assert!(starts_prefix(&IcedKey::Character("x".into()), CTRL));
        assert!(!starts_prefix(&IcedKey::Character("x".into()), NONE));
        assert!(!starts_prefix(&IcedKey::Character("c".into()), CTRL));
    }

    #[test]
    fn lone_modifier_keys_are_ignored() {
        assert_eq!(named(Named::Shift, SHIFT), None);
    }
}
