use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Location, Modifiers};
use oxmacs_core::config::Config;
use oxmacs_core::editor::ConnectionId;
use oxmacs_core::external::net::Event;
use oxmacs_gui::app::{App, Message};
use oxmacs_irc::Irc;

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
        if rest == "RET" {
            key(app, Key::Named(Named::Enter), None, modifiers);
        } else {
            key(app, Key::Character(rest.into()), Some(rest), modifiers);
        }
    }
}

fn type_text(app: &mut App, text: &str) {
    for character in text.chars() {
        keys(app, &character.to_string());
    }
}

#[test]
fn channel_windows_list_their_members() {
    let config = Config::parse(
        "[[irc.server]]\nname = \"libera\"\nhost = \"irc.example\"\nnick = \"oxtest\"\nchannels = [\"#c\"]\n",
    )
    .unwrap();
    let (mut app, _) = App::boot(&Ok(config), &[], vec![Box::new(Irc)]);
    keys(&mut app, "M-x");
    type_text(&mut app, "irc");
    keys(&mut app, "RET");
    let id = ConnectionId(0);
    let _ = app.update(Message::Connection(id, Event::Connected));
    for line in [
        ":srv CAP * LS :",
        ":srv 001 oxtest :Welcome",
        ":oxtest!o@h JOIN #c",
        ":srv 353 oxtest = #c :alice @bob oxtest",
        ":srv 366 oxtest #c :End",
    ] {
        let _ = app.update(Message::Connection(id, Event::Line(line.to_owned())));
    }
    keys(&mut app, "C-x b");
    type_text(&mut app, "#c");
    keys(&mut app, "RET");

    let mut ui = iced_test::simulator(app.view());

    assert!(ui.find("3 members").is_ok());
    assert!(ui.find("@bob").is_ok());
    assert!(ui.find(" alice").is_ok());
}
