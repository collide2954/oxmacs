use std::thread;
use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;
use oxmacs_core::input::KeySequence;

use crate::{Ox, Pace};

pub fn settle(editor: &mut Editor, ox: &mut Ox) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while ox.tick(editor) != Pace::Idle
        && editor.minibuffer().is_none()
        && editor.question().is_none()
        && Instant::now() < deadline
    {
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
    let notation: Vec<String> = text.chars().map(|c| c.to_string()).collect();
    press(editor, ox, &notation.join(" "));
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
