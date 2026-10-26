#![allow(dead_code, reason = "each test binary uses part of the support module")]

use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Location, Modifiers};
use oxmacs_gui::app::{App, Message};
use oxmacs_gui::fonts;

pub fn settings() -> iced::Settings {
    iced::Settings {
        fonts: vec![fonts::JETBRAINS_MONO_REGULAR.into()],
        default_font: fonts::EDITOR,
        antialiasing: true,
        ..iced::Settings::default()
    }
}

pub fn pressed(key: Key, text: Option<&str>, modifiers: Modifiers) -> Message {
    Message::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Code(Code::KeyA),
        location: Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    })
}

pub fn chord(notation: &str) -> Message {
    let mut modifiers = Modifiers::empty();
    let mut rest = notation;
    loop {
        if let Some(after) = rest.strip_prefix("C-") {
            modifiers |= Modifiers::CTRL;
            rest = after;
        } else if let Some(after) = rest.strip_prefix("M-") {
            modifiers |= Modifiers::ALT;
            rest = after;
        } else {
            break;
        }
    }
    let named = match rest {
        "RET" => Some(Named::Enter),
        "TAB" => Some(Named::Tab),
        "SPC" => Some(Named::Space),
        "DEL" => Some(Named::Backspace),
        "ESC" => Some(Named::Escape),
        "<down>" => Some(Named::ArrowDown),
        "<up>" => Some(Named::ArrowUp),
        "<left>" => Some(Named::ArrowLeft),
        "<right>" => Some(Named::ArrowRight),
        _ => None,
    };
    match named {
        Some(named) => pressed(Key::Named(named), None, modifiers),
        None => pressed(Key::Character(rest.into()), Some(rest), modifiers),
    }
}

pub fn typed(character: char) -> Message {
    if character == '\n' {
        return chord("RET");
    }
    let text = character.to_string();
    pressed(
        Key::Character(text.as_str().into()),
        Some(&text),
        Modifiers::empty(),
    )
}

pub fn settle(app: &mut App) {
    for _ in 0..5 {
        let _ = app.update(Message::OxTick);
    }
}

pub fn keys(app: &mut App, notation: &str) {
    for each in notation.split(' ') {
        let _ = app.update(chord(each));
    }
}

pub fn type_text(app: &mut App, text: &str) {
    for character in text.chars() {
        let _ = app.update(typed(character));
    }
}

pub fn press(app: &mut App, notation: &str) {
    for each in notation.split(' ') {
        let _ = app.update(chord(each));
        settle(app);
    }
}
