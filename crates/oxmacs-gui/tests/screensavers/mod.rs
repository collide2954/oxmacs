use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::Size;
use iced_test::Simulator;
use oxmacs_gui::app::{App, Message};

use super::{Film, keys, settings, settle, track, type_char, wait};

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

const FPS: f64 = 20.0;

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

fn started(folder: &Path, kind: &str) -> App {
    oxmacs_gui::widgets::backdrop::wait_for_shaders();
    let home = saver_home(folder);
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
    app
}

fn still(out: &Path, kind: &str, times: &[f64]) {
    let folder = out.join(format!("{kind}.work"));
    let mut app = started(&folder, kind);
    let size = Size::new(960.0, 540.0);
    let mut ui = Simulator::with_size(settings(), size, app.view());
    let _ = ui.snapshot(&app.theme());
    for message in ui.into_messages() {
        track(app.update(message));
    }
    let start = Instant::now();
    track(app.update(Message::Tick(start)));
    for seconds in times {
        track(app.update(Message::Tick(start + Duration::from_secs_f64(*seconds))));
        for _ in 0..3 {
            let mut ui = Simulator::with_size(settings(), size, app.view());
            let _ = ui.snapshot(&app.theme());
            std::thread::sleep(Duration::from_millis(300));
        }
        let mut ui = Simulator::with_size(settings(), size, app.view());
        let shot = ui.snapshot(&app.theme()).unwrap();
        shot.matches_image(out.join(format!("{kind}-{:06.0}", seconds * 1000.0)))
            .unwrap();
    }
    let _ = fs::remove_dir_all(&folder);
}

#[test]
#[ignore = "renders stills of each screensaver, or of those $OXMACS_SCREENSAVERS names, at $OXMACS_SCREENSAVER_TIMES seconds into the folder $OXMACS_SCREENSAVER_STILLS"]
fn screensaver_stills() {
    let Ok(out) = std::env::var("OXMACS_SCREENSAVER_STILLS") else {
        return;
    };
    let out = PathBuf::from(out);
    fs::create_dir_all(&out).unwrap();
    let wanted = std::env::var("OXMACS_SCREENSAVERS").ok();
    let times: Vec<f64> = std::env::var("OXMACS_SCREENSAVER_TIMES").map_or_else(
        |_| vec![2.0, 9.0],
        |times| {
            times
                .split(',')
                .filter_map(|time| time.trim().parse().ok())
                .collect()
        },
    );
    for kind in KINDS {
        if wanted
            .as_deref()
            .is_none_or(|wanted| wanted.split(',').any(|named| named.trim() == kind))
        {
            still(&out, kind, &times);
        }
    }
}

fn film_one(out: &Path, kind: &str, seconds: f64) {
    let folder = out.join(format!("{kind}.work"));
    let mut app = started(&folder, kind);
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
