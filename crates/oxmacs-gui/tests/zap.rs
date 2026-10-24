use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use iced::Size;
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Location, Modifiers};
use iced_test::Simulator;
use oxmacs_core::cli::Visit;
use oxmacs_gui::app::{App, Message};
use oxmacs_gui::fonts;

const DEMO: &str = "(def (area w h)\n  (* w h))\n\n(area 2 3)\n(area 2 :wide)\n";

fn settings() -> iced::Settings {
    iced::Settings {
        fonts: vec![fonts::JETBRAINS_MONO_REGULAR.into()],
        default_font: fonts::EDITOR,
        antialiasing: true,
        ..iced::Settings::default()
    }
}

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

fn chord(app: &mut App, chord: &str) {
    let mut modifiers = Modifiers::empty();
    let mut rest = chord;
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
    match rest {
        "RET" => key(app, Key::Named(Named::Enter), None, modifiers),
        _ => key(app, Key::Character(rest.into()), Some(rest), modifiers),
    }
}

fn press(app: &mut App, notation: &str) {
    for each in notation.split(' ') {
        chord(app, each);
        settle(app);
    }
}

fn settle(app: &mut App) {
    for _ in 0..5 {
        let _ = app.update(Message::OxTick);
    }
}

fn shot(app: &App, out: &Path, name: &str) {
    let mut ui = Simulator::with_size(settings(), Size::new(560.0, 220.0), app.view());
    for _ in 0..3 {
        let _ = ui.snapshot(&app.theme());
    }
    let snapshot = ui.snapshot(&app.theme()).unwrap();
    snapshot.matches_image(out.join(name)).unwrap();
}

fn home(theme: &str) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    fs::write(
        home.path().join("init.ox"),
        format!(
            "(settings {{theme: :{theme} animations: false font-size: 12 splash: false}})\n(use-plugin zap :config {{duration: 2000}})\n"
        ),
    )
    .unwrap();
    fs::write(home.path().join("demo.ox"), DEMO).unwrap();
    home
}

fn film(theme: &str, out: &Path) {
    let home = home(theme);
    let visit = Visit {
        path: home.path().join("demo.ox"),
        line: None,
        column: None,
    };
    let (mut app, _) = App::boot(&[visit], Some(home.path()));
    settle(&mut app);
    press(&mut app, "C-M-x");
    shot(&app, out, &format!("{theme}-definition"));
    thread::sleep(Duration::from_millis(2100));
    press(&mut app, "C-n C-n C-n C-e C-x C-e");
    shot(&app, out, &format!("{theme}-evaluated"));
    thread::sleep(Duration::from_millis(300));
    shot(&app, out, &format!("{theme}-fading"));
    thread::sleep(Duration::from_millis(1800));
    press(&mut app, "C-n C-e C-x C-e");
    shot(&app, out, &format!("{theme}-failed"));
}

#[test]
#[ignore = "renders snapshots of zap's flashes into $OXMACS_ZAP_SNAPSHOTS"]
fn zap_snapshots() {
    let Ok(out) = std::env::var("OXMACS_ZAP_SNAPSHOTS") else {
        return;
    };
    let out = PathBuf::from(out);
    fs::create_dir_all(&out).unwrap();
    film("adwaita", &out);
    film("adwaita-dark", &out);
}
