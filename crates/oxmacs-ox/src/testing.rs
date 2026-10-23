use std::thread;
use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;
use oxmacs_core::input::KeySequence;

use crate::{Ox, Pace, Wake};

const LONG_SLEEP: Duration = Duration::from_millis(500);

fn sleeping_long(ox: &Ox) -> bool {
    match ox.next_wake() {
        Wake::Soon => false,
        Wake::At(wake) => wake > Instant::now() + LONG_SLEEP,
        Wake::Never => true,
    }
}

pub fn settle(editor: &mut Editor, ox: &mut Ox) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut quiet = 0;
    while ox.tick(editor) != Pace::Idle
        && editor.minibuffer().is_none()
        && editor.question().is_none()
        && quiet < 3
        && Instant::now() < deadline
    {
        quiet = if sleeping_long(ox) { quiet + 1 } else { 0 };
        thread::sleep(Duration::from_millis(1));
    }
}

pub fn keys(editor: &mut Editor, notation: &str) {
    let keys: KeySequence = notation.parse().unwrap();
    for key in keys.keys() {
        editor.handle_key(*key);
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

pub fn at_point(editor: &Editor) -> String {
    let buffer = editor.buffer();
    let line = buffer.line_of(buffer.point());
    buffer.text_in(buffer.point()..buffer.line_end(line))
}

pub fn question(editor: &Editor) -> Option<String> {
    editor
        .question()
        .map(|question| question.prompt().to_owned())
}
