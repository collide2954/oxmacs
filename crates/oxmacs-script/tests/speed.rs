use oxmacs_core::editor::Editor;
use oxmacs_script::Ox;
use oxmacs_script::testing::{keys, rust_sample, timed};

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
        std::fs::write(&path, rust_sample(lines)).unwrap();
        timed(&format!("{name}: the first key after opening"), 3, || {
            let mut fresh = Editor::new();
            fresh.open_file(&path);
            keys(&mut fresh, "C-n C-n x");
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
        keys(&mut bare, "C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n");
        bare.prepare_display();
        timed(&format!("{name}: the key without Ox"), runs, || {
            keys(&mut bare, "x");
        });
        timed(&format!("{name}: a move without Ox"), runs, || {
            keys(&mut bare, "C-f");
        });
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.start(&mut editor, Some(home.path()));
        editor.open_file(&path);
        keys(&mut editor, "C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n");
        editor.prepare_display();
        ox.tick(&mut editor);
        timed(&format!("{name}: handle the key"), runs, || {
            keys(&mut editor, "x");
        });
        timed(
            &format!("{name}: handle the key and prepare display"),
            runs,
            || {
                keys(&mut editor, "x");
                editor.prepare_display();
            },
        );
        timed(&format!("{name}: key, display and effects"), runs, || {
            keys(&mut editor, "x");
            editor.prepare_display();
            drop(editor.take_effects());
        });
        timed(&format!("{name}: key and an Ox tick"), runs, || {
            keys(&mut editor, "x");
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
