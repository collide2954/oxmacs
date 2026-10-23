use std::fmt::Write as _;
use std::time::{Duration, Instant};

use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key, Location, Modifiers};
use oxmacs_core::cli::Visit;
use oxmacs_gui::app::{App, Message};

fn key(app: &mut App, key: Key, text: Option<&str>) {
    let _ = app.update(Message::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Code(Code::KeyA),
        location: Location::Standard,
        modifiers: Modifiers::empty(),
        text: text.map(Into::into),
        repeat: false,
    }));
}

fn timed(label: &str, runs: usize, mut work: impl FnMut()) -> Duration {
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        work();
        times.push(started.elapsed());
    }
    times.sort_unstable();
    let median = times[times.len() / 2];
    let worst = times[times.len() - 1];
    println!("{label:<40} median {median:>10.2?}   worst {worst:>10.2?}   ({runs} runs)");
    median
}

fn big_rust(lines: usize) -> String {
    let mut text = String::new();
    for index in 0..lines / 4 {
        let _ = writeln!(
            text,
            "fn item_{index}(value: u64) -> u64 {{\n    let doubled = value * 2; // {index}\n    doubled + {index}\n}}"
        );
    }
    text
}

#[test]
#[ignore = "prints timings; run with --release --ignored --nocapture"]
fn how_fast_the_editor_is() {
    let home = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let small = files.path().join("small.rs");
    let big = files.path().join("big.rs");
    std::fs::write(&small, big_rust(400)).unwrap();
    std::fs::write(&big, big_rust(20_000)).unwrap();

    timed("boot with the bundled plugins", 5, || {
        let _ = App::boot(&[], Some(home.path()));
    });

    let (mut app, _) = App::boot(&[], Some(home.path()));
    let visit = |path: &std::path::Path| Visit {
        path: path.to_path_buf(),
        line: None,
        column: None,
    };
    timed("open a 400-line Rust file", 5, || {
        app.start(&[visit(&small)], false);
    });
    timed("open a 20,000-line Rust file", 3, || {
        app.start(&[visit(&big)], false);
    });

    for (name, path) in [("small", &small), ("big", &big)] {
        let (mut app, _) = App::boot(&[], Some(home.path()));
        app.start(&[visit(path)], false);
        for _ in 0..20 {
            key(&mut app, Key::Named(Named::ArrowDown), None);
        }
        timed(&format!("type a letter in the {name} file"), 200, || {
            key(&mut app, Key::Character("x".into()), Some("x"));
        });
        timed(&format!("move down a line in the {name} file"), 200, || {
            key(&mut app, Key::Named(Named::ArrowDown), None);
        });
        timed(&format!("build the view of the {name} file"), 50, || {
            drop(app.view());
        });
        timed(&format!("page down in the {name} file"), 50, || {
            key(&mut app, Key::Named(Named::PageDown), None);
        });
    }
}
