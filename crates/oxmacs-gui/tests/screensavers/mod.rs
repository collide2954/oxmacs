use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::Size;
use oxmacs_gui::app::{App, Message};

use super::{Film, keys, settle, track, type_char, wait};

const KINDS: [&str; 29] = [
    "starfield",
    "matrix",
    "pond",
    "fireworks",
    "harmonograph",
    "aurora",
    "clock",
    "flow",
    "orbits",
    "snowfall",
    "tunnel",
    "lava",
    "constellations",
    "plasma",
    "bubbles",
    "kaleidoscope",
    "wireframes",
    "city",
    "pendulums",
    "tree",
    "ocean",
    "glass",
    "galaxy",
    "ants",
    "campfire",
    "sorting",
    "storm",
    "fireflies",
    "mandelbrot",
];

const FPS: f64 = 15.0;

fn saver_home(folder: &Path) -> PathBuf {
    let home = folder.join("home");
    fs::create_dir_all(&home).unwrap();
    fs::write(
        home.join("init.ox"),
        "(settings {theme: :adwaita-dark animations: false})\n(use-plugin screensaver)\n",
    )
    .unwrap();
    home
}

fn typed_in(app: &mut App, text: &str) {
    for c in text.chars() {
        type_char(app, c);
    }
}

fn film_one(out: &Path, kind: &str, seconds: f64) {
    let folder = out.join(format!("{kind}.work"));
    let home = saver_home(&folder);
    let (mut app, _) = App::boot(&[], Some(&home));
    settle(&mut app);
    keys(&mut app, "M-x");
    typed_in(&mut app, "screensaver-preview");
    keys(&mut app, "RET");
    wait(&mut app, "the screensaver prompt", |app| {
        app.editor().minibuffer().is_some()
    });
    typed_in(&mut app, kind);
    keys(&mut app, "RET");
    settle(&mut app);
    let mut film = Film::new(folder.join("frames")).sized(Size::new(960.0, 540.0));
    let start = Instant::now();
    let mut frame = 0.0;
    while frame / FPS < seconds {
        track(app.update(Message::Tick(start + Duration::from_secs_f64(frame / FPS))));
        film.shot(&mut app, 1.0 / FPS);
        frame += 1.0;
    }
    film.finish(&out.join(format!("{kind}.mp4")));
    let _ = fs::remove_dir_all(&folder);
}

#[test]
#[ignore = "renders a short video of each screensaver, or of those $OXMACS_SCREENSAVERS names, into the folder $OXMACS_SCREENSAVER_VIDEOS"]
fn screensaver_videos() {
    let Ok(out) = std::env::var("OXMACS_SCREENSAVER_VIDEOS") else {
        return;
    };
    let out = PathBuf::from(out);
    fs::create_dir_all(&out).unwrap();
    let wanted = std::env::var("OXMACS_SCREENSAVERS").ok();
    let seconds = std::env::var("OXMACS_SCREENSAVER_SECONDS")
        .ok()
        .and_then(|seconds| seconds.parse().ok())
        .unwrap_or(8.0);
    for kind in KINDS {
        if wanted
            .as_deref()
            .is_none_or(|wanted| wanted.split(',').any(|named| named.trim() == kind))
        {
            film_one(&out, kind, seconds);
        }
    }
}
