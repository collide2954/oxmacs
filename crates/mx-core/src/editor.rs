use std::mem;

use crate::bindings;
use crate::buffer::Buffer;
use crate::command::Command;
use crate::key::{Key, KeyCode, KeySequence, Modifiers};
use crate::keymap::{Keymap, Lookup};
use crate::motion;

#[derive(Debug)]
pub struct Editor {
    buffer: Buffer,
    keymap: Keymap,
    pending: Vec<Key>,
    last_key: Option<Key>,
    message: Option<String>,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    pub fn new() -> Self {
        Self::with_buffer(Buffer::new("*scratch*"))
    }

    pub fn with_buffer(buffer: Buffer) -> Self {
        Self {
            buffer,
            keymap: bindings::global(),
            pending: Vec::new(),
            last_key: None,
            message: None,
        }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn handle_key(&mut self, key: Key) {
        self.message = None;
        self.pending.push(key);
        match self.keymap.lookup(&self.pending) {
            Lookup::Prefix => {}
            Lookup::Command(command) => self.dispatch(key, command),
            Lookup::Unbound if self.pending.len() == 1 && inserts_itself(key) => {
                self.dispatch(key, Command::SelfInsertCommand);
            }
            Lookup::Unbound => {
                let keys = KeySequence::from(mem::take(&mut self.pending));
                self.message = Some(format!("{keys} is undefined"));
            }
        }
    }

    fn dispatch(&mut self, key: Key, command: Command) {
        self.pending.clear();
        self.last_key = Some(key);
        self.execute(command);
    }

    pub fn execute(&mut self, command: Command) {
        match command {
            Command::SelfInsertCommand => self.self_insert(),
            Command::ForwardChar => self.move_point(|b, p| motion::forward_char(b, p, 1)),
            Command::BackwardChar => self.move_point(|_, p| motion::backward_char(p, 1)),
            Command::NextLine => self.move_point(|b, p| motion::vertical(b, p, 1, b.column(p))),
            Command::PreviousLine => {
                self.move_point(|b, p| motion::vertical(b, p, -1, b.column(p)));
            }
            Command::MoveBeginningOfLine => self.move_point(motion::beginning_of_line),
            Command::MoveEndOfLine => self.move_point(motion::end_of_line),
            Command::ForwardWord => self.move_point(motion::forward_word),
            Command::BackwardWord => self.move_point(motion::backward_word),
            Command::BeginningOfBuffer => self.move_point(|_, _| 0),
            Command::EndOfBuffer => self.move_point(|b, _| b.len_chars()),
        }
    }

    fn move_point(&mut self, motion: impl FnOnce(&Buffer, usize) -> usize) {
        let target = motion(&self.buffer, self.buffer.point());
        self.buffer.set_point(target);
    }

    fn self_insert(&mut self) {
        if let Some(KeyCode::Char(c)) = self.last_key.map(Key::code) {
            self.buffer.insert(c.encode_utf8(&mut [0; 4]));
        }
    }
}

fn inserts_itself(key: Key) -> bool {
    let chords = Modifiers::CONTROL | Modifiers::META | Modifiers::SUPER;
    matches!(key.code(), KeyCode::Char(c) if !c.is_control()) && !key.modifiers().intersects(chords)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(editor: &mut Editor, notation: &str) {
        let sequence: KeySequence = notation.parse().unwrap();
        for key in sequence.keys() {
            editor.handle_key(*key);
        }
    }

    #[test]
    fn starts_with_empty_scratch_buffer() {
        let editor = Editor::new();

        assert_eq!(editor.buffer().name(), "*scratch*");
        assert_eq!(editor.buffer().contents(), "");
    }

    #[test]
    fn printable_keys_insert_themselves() {
        let mut editor = Editor::new();

        press(&mut editor, "h i SPC A");

        assert_eq!(editor.buffer().contents(), "hi A");
        assert_eq!(editor.buffer().point(), 4);
    }

    #[test]
    fn unbound_keys_report_undefined_and_insert_nothing() {
        let mut editor = Editor::new();

        press(&mut editor, "C-q");

        assert_eq!(editor.buffer().contents(), "");
        assert_eq!(editor.message(), Some("C-q is undefined"));
    }

    #[test]
    fn next_key_clears_the_message() {
        let mut editor = Editor::new();

        press(&mut editor, "C-q a");

        assert_eq!(editor.message(), None);
    }

    fn editor_with(text: &str, point: usize) -> Editor {
        let mut buffer = Buffer::with_text("t", text);
        buffer.set_point(point);
        Editor::with_buffer(buffer)
    }

    fn point_after(text: &str, point: usize, notation: &str) -> usize {
        let mut editor = editor_with(text, point);
        press(&mut editor, notation);
        editor.buffer().point()
    }

    #[test]
    fn char_motions() {
        assert_eq!(point_after("hello", 2, "C-f"), 3);
        assert_eq!(point_after("hello", 2, "<right>"), 3);
        assert_eq!(point_after("hello", 2, "C-b"), 1);
        assert_eq!(point_after("hello", 2, "<left>"), 1);
    }

    #[test]
    fn line_motions() {
        let text = "one\ntwo\nthree";
        assert_eq!(point_after(text, 5, "C-a"), 4);
        assert_eq!(point_after(text, 5, "<home>"), 4);
        assert_eq!(point_after(text, 5, "C-e"), 7);
        assert_eq!(point_after(text, 5, "<end>"), 7);
        assert_eq!(point_after(text, 5, "C-n"), 9);
        assert_eq!(point_after(text, 5, "<down>"), 9);
        assert_eq!(point_after(text, 5, "C-p"), 1);
        assert_eq!(point_after(text, 5, "<up>"), 1);
    }

    #[test]
    fn word_motions() {
        let text = "alpha beta gamma";
        assert_eq!(point_after(text, 0, "M-f"), 5);
        assert_eq!(point_after(text, 0, "C-<right>"), 5);
        assert_eq!(point_after(text, 16, "M-b"), 11);
        assert_eq!(point_after(text, 16, "C-<left>"), 11);
    }

    #[test]
    fn buffer_motions() {
        let text = "one\ntwo";
        assert_eq!(point_after(text, 5, "M-<"), 0);
        assert_eq!(point_after(text, 5, "C-<home>"), 0);
        assert_eq!(point_after(text, 1, "M->"), 7);
        assert_eq!(point_after(text, 1, "C-<end>"), 7);
    }
}
