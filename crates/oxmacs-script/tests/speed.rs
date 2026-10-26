use std::fmt::Write as _;
use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;
use oxmacs_core::input::KeySequence;
use oxmacs_script::Ox;

fn timed(label: &str, runs: usize, mut work: impl FnMut()) -> Duration {
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        work();
        times.push(started.elapsed());
    }
    times.sort_unstable();
    let median = times[times.len() / 2];
    println!(
        "{label:<44} median {median:>10.2?}   worst {:>10.2?}",
        times[times.len() - 1]
    );
    median
}

fn rust(lines: usize) -> String {
    let mut text = String::new();
    for index in 0..lines / 4 {
        let _ = writeln!(
            text,
            "fn item_{index}(value: u64) -> u64 {{\n    let doubled = value * 2; // {index}\n    doubled + {index}\n}}"
        );
    }
    text
}

fn press(editor: &mut Editor, notation: &str) {
    let keys: KeySequence = notation.parse().unwrap();
    for key in keys.keys() {
        editor.handle_key(*key);
    }
}

#[test]
#[ignore = "prints timings; run with --release --ignored --nocapture"]
fn how_long_highlighting_a_window_of_ox_takes() {
    let home = tempfile::tempdir().unwrap();
    let plugins = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/r");
    for name in ["plugin.ox", "session.ox"] {
        let mut editor = Editor::new();
        Ox::new().start(&mut editor, Some(home.path()));
        editor.open_file(&plugins.join(name));
        let lines = editor.buffer().len_lines();
        timed(&format!("{name}: highlight the last 45 lines"), 50, || {
            for line in lines.saturating_sub(45)..lines {
                drop(editor.line_style(line));
            }
        });
    }
}

#[test]
#[ignore = "prints timings; run with --release --ignored --nocapture"]
fn where_a_keystroke_spends_its_time() {
    let home = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    for (name, lines, runs) in [("small.rs", 400, 100), ("big.rs", 20_000, 20)] {
        let path = files.path().join(name);
        std::fs::write(&path, rust(lines)).unwrap();
        timed(&format!("{name}: the first key after opening"), 3, || {
            let mut fresh = Editor::new();
            fresh.open_file(&path);
            press(&mut fresh, "C-n C-n x");
        });
        timed(
            &format!("{name}: the first display after opening"),
            3,
            || {
                let mut fresh = Editor::new();
                fresh.open_file(&path);
                fresh.prepare_display();
            },
        );
        let mut bare = Editor::new();
        bare.open_file(&path);
        press(&mut bare, "C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n");
        bare.prepare_display();
        timed(&format!("{name}: the key without Ox"), runs, || {
            press(&mut bare, "x");
        });
        timed(&format!("{name}: a move without Ox"), runs, || {
            press(&mut bare, "C-f");
        });
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.start(&mut editor, Some(home.path()));
        editor.open_file(&path);
        press(&mut editor, "C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n");
        editor.prepare_display();
        ox.tick(&mut editor);
        timed(&format!("{name}: handle the key"), runs, || {
            press(&mut editor, "x");
        });
        timed(
            &format!("{name}: handle the key and prepare display"),
            runs,
            || {
                press(&mut editor, "x");
                editor.prepare_display();
            },
        );
        timed(&format!("{name}: key, display and effects"), runs, || {
            press(&mut editor, "x");
            editor.prepare_display();
            drop(editor.take_effects());
        });
        timed(&format!("{name}: key and an Ox tick"), runs, || {
            press(&mut editor, "x");
            ox.tick(&mut editor);
        });
        timed(
            &format!("{name}: an Ox tick with nothing new"),
            runs,
            || {
                ox.tick(&mut editor);
            },
        );
        timed(&format!("{name}: the mode line"), runs, || {
            drop(editor.mode_line());
        });
    }
}
