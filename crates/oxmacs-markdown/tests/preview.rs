use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Location, Modifiers};
use iced::widget;
use oxmacs_core::config::Config;
use oxmacs_gui::app::{App, Message};
use oxmacs_markdown::Markdown;

fn key(app: &mut App, key: Key, text: Option<&str>, modifiers: Modifiers) {
    let _ = app.update(Message::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Code(Code::KeyA),
        location: Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    }));
}

fn keys(app: &mut App, notation: &str) {
    for chord in notation.split(' ') {
        let (modifiers, rest) = if let Some(rest) = chord.strip_prefix("C-") {
            (Modifiers::CTRL, rest)
        } else if let Some(rest) = chord.strip_prefix("M-") {
            (Modifiers::ALT, rest)
        } else {
            (Modifiers::empty(), chord)
        };
        match rest {
            "RET" => key(app, Key::Named(Named::Enter), None, modifiers),
            "SPC" => key(app, Key::Named(Named::Space), Some(" "), modifiers),
            _ => key(app, Key::Character(rest.into()), Some(rest), modifiers),
        }
    }
}

fn type_text(app: &mut App, text: &str) {
    for character in text.chars() {
        match character {
            ' ' => keys(app, "SPC"),
            '\n' => keys(app, "RET"),
            other => keys(app, &other.to_string()),
        }
    }
}

fn previewing(text: &str) -> App {
    let config = Config::parse("splash = false\n").unwrap();
    let (mut app, _) = App::boot(&Ok(config), &[], vec![Box::new(Markdown::default())]);
    keys(&mut app, "M-x");
    type_text(&mut app, "markdown-mode");
    keys(&mut app, "RET");
    type_text(&mut app, text);
    keys(&mut app, "C-c C-c p");
    app
}

#[test]
fn the_preview_window_draws_the_rendered_document() {
    let app = previewing("# Title\n\nHello preview");
    let preview = app.editor().tiling().windows()[1];

    let mut ui = iced_test::simulator(app.view());

    assert!(
        ui.find(widget::Id::from(format!("preview-{preview:?}")))
            .is_ok()
    );
}
