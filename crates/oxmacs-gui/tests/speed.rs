mod support;

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use oxmacs_core::editor::Visit;
use oxmacs_gui::app::App;
use oxmacs_script::testing::{rust_sample, timed};

use support::pressed;

fn key(app: &mut App, key: Key, text: Option<&str>) {
    let _ = app.update(pressed(key, text, Modifiers::empty()));
}

#[test]
#[ignore = "prints timings; run with --release --ignored --nocapture"]
fn how_fast_ox_files_scroll() {
    let home = tempfile::tempdir().unwrap();
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for name in [
        "oxmacs-script/plugins/r/plugin.ox",
        "oxmacs-script/plugins/r/session.ox",
        "oxmacs-script/src/ide/help.rs",
    ] {
        let (mut app, _) = App::boot(&[], Some(home.path()));
        app.start(
            &[Visit {
                path: crates.join(name),
                line: None,
                column: None,
            }],
            false,
        );
        timed(&format!("move down a line in {name}"), 120, || {
            key(&mut app, Key::Named(Named::ArrowDown), None);
        });
        timed(&format!("build the view of {name}"), 50, || {
            drop(app.view());
        });
    }
}

#[test]
#[ignore = "prints timings; run with --release --ignored --nocapture"]
fn how_fast_the_editor_is() {
    let home = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let small = files.path().join("small.rs");
    let big = files.path().join("big.rs");
    std::fs::write(&small, rust_sample(400)).unwrap();
    std::fs::write(&big, rust_sample(20_000)).unwrap();

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
