use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;
use oxmacs_core::input::KeySequence;
pub use oxmacs_core::testing::{at_point, press as keys, question};

use crate::{Ox, Pace, Wake};

mod fixtures;

pub use fixtures::{Chapter, Epub, PIXEL, pdf_file, rust_sample, sample_pdf, timed, write_epub};

const LONG_SLEEP: Duration = Duration::from_millis(500);
const SETTLE_LIMIT: Duration = Duration::from_secs(10);

fn sleeping_long(ox: &Ox) -> bool {
    match ox.next_wake() {
        Wake::Soon => false,
        Wake::At(wake) => wake > Instant::now() + LONG_SLEEP,
        Wake::Never => true,
    }
}

pub fn settle(editor: &mut Editor, ox: &mut Ox) {
    let deadline = Instant::now() + SETTLE_LIMIT;
    let mut quiet = 0;
    while ox.tick(editor) != Pace::Idle
        && editor.minibuffer().is_none()
        && editor.question().is_none()
        && quiet < 3
    {
        assert!(
            Instant::now() < deadline,
            "Ox didn't settle in {} s: {} tasks runnable, {} pending, checker busy: {}",
            SETTLE_LIMIT.as_secs(),
            ox.isolate.runnable(),
            ox.isolate.pending(),
            ox.checker.busy()
        );
        quiet = if sleeping_long(ox) { quiet + 1 } else { 0 };
        thread::sleep(Duration::from_millis(1));
    }
}

pub fn press(editor: &mut Editor, ox: &mut Ox, notation: &str) {
    let keys: KeySequence = notation.parse().unwrap();
    for key in keys.keys() {
        editor.handle_key(*key);
        settle(editor, ox);
    }
}

pub fn typed(editor: &mut Editor, ox: &mut Ox, text: &str) {
    if text.is_empty() {
        return;
    }
    let notation: Vec<String> = text
        .chars()
        .map(|c| match c {
            ' ' => "SPC".to_owned(),
            other => other.to_string(),
        })
        .collect();
    press(editor, ox, &notation.join(" "));
}

pub fn home_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    for (name, text) in files {
        let path = home.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    home
}

pub fn started(init: &str) -> (tempfile::TempDir, Editor, Ox) {
    let home = home_with(&[("init.ox", init)]);
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

pub fn run_command(editor: &mut Editor, ox: &mut Ox, command: &str) {
    let id = editor.command_named(command).unwrap();
    editor.execute(id);
    settle(editor, ox);
}

pub fn git(directory: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    let output = command.args(args).current_dir(directory).output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}
