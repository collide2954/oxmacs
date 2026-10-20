use std::time::Duration;

use iced::keyboard::key::{Code, Physical};
use iced::keyboard::{self, Key, Location, Modifiers};
use iced::time::Instant;
use oxmacs_core::config::Config;
use oxmacs_glow::Glow;
use oxmacs_gui::app::{App, Message};

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

fn later(app: &mut App) {
    let _ = app.update(Message::Tick(Instant::now() + Duration::from_secs(1)));
}

#[test]
fn the_minibuffer_glows_while_a_prompt_waits() {
    let config = Config::parse("splash = false\n").unwrap();
    let (mut app, _) = App::boot(&Ok(config), &[], vec![Box::new(Glow::default())]);
    assert!(app.minibuffer_style().is_none());

    key(
        &mut app,
        Key::Character("x".into()),
        Some("x"),
        Modifiers::ALT,
    );
    assert!(app.is_animating());
    later(&mut app);

    let glowing = app.minibuffer_style().unwrap();
    assert!(glowing.shadow.blur_radius > 0.0);
    assert!(glowing.shadow.color.a > 0.0);

    key(
        &mut app,
        Key::Character("g".into()),
        Some("g"),
        Modifiers::CTRL,
    );
    later(&mut app);

    assert!(app.minibuffer_style().is_none());
    assert!(!app.is_animating());
}
