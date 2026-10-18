use std::mem;

use crate::bindings;
use crate::buffer::Buffer;
use crate::command::Command;
use crate::key::{Key, KeyCode, KeySequence, Modifiers};
use crate::keymap::{Keymap, Lookup};
use crate::motion;
use crate::viewport::Viewport;

const DEFAULT_ROWS: usize = 24;

#[derive(Debug)]
pub struct Editor {
    buffer: Buffer,
    keymap: Keymap,
    pending: Vec<Key>,
    last_key: Option<Key>,
    goal_column: Option<usize>,
    message: Option<String>,
    quit_requested: bool,
    viewport: Viewport,
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
            goal_column: None,
            message: None,
            quit_requested: false,
            viewport: Viewport::new(DEFAULT_ROWS),
        }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn should_quit(&self) -> bool {
        self.quit_requested
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn resize_window(&mut self, rows: usize) {
        self.viewport.resize(rows, self.point_line());
    }

    fn point_line(&self) -> usize {
        self.buffer.line_of(self.buffer.point())
    }

    pub fn handle_key(&mut self, key: Key) {
        self.message = None;
        if !self.pending.is_empty() && is_keyboard_quit(key) {
            self.dispatch(key, Command::KeyboardQuit);
            return;
        }
        self.pending.push(key);
        match self.keymap.lookup(&self.pending) {
            Lookup::Prefix => {
                let keys = KeySequence::from(self.pending.clone());
                self.message = Some(format!("{keys}-"));
            }
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
        let goal_column = self.goal_column.take();
        match command {
            Command::SelfInsertCommand => self.self_insert(),
            Command::Newline => self.buffer.insert("\n"),
            Command::DeleteChar => self.buffer.delete_forward(1),
            Command::DeleteBackwardChar => self.buffer.delete_backward(1),
            Command::ForwardChar => self.move_point(|b, p| motion::forward_char(b, p, 1)),
            Command::BackwardChar => self.move_point(|_, p| motion::backward_char(p, 1)),
            Command::NextLine => self.move_vertically(1, goal_column),
            Command::PreviousLine => self.move_vertically(-1, goal_column),
            Command::MoveBeginningOfLine => self.move_point(motion::beginning_of_line),
            Command::MoveEndOfLine => self.move_point(motion::end_of_line),
            Command::ForwardWord => self.move_point(motion::forward_word),
            Command::BackwardWord => self.move_point(motion::backward_word),
            Command::BeginningOfBuffer => self.move_point(|_, _| 0),
            Command::EndOfBuffer => self.move_point(|b, _| b.len_chars()),
            Command::KeyboardQuit => self.message = Some("Quit".to_owned()),
            Command::SaveBuffersKillMx => self.quit_requested = true,
        }
        self.viewport.reveal(self.point_line());
    }

    fn move_point(&mut self, motion: impl FnOnce(&Buffer, usize) -> usize) {
        let target = motion(&self.buffer, self.buffer.point());
        self.buffer.set_point(target);
    }

    fn move_vertically(&mut self, lines: isize, goal_column: Option<usize>) {
        let point = self.buffer.point();
        let goal = goal_column.unwrap_or_else(|| self.buffer.column(point));
        self.buffer
            .set_point(motion::vertical(&self.buffer, point, lines, goal));
        self.goal_column = Some(goal);
    }

    fn self_insert(&mut self) {
        if let Some(KeyCode::Char(c)) = self.last_key.map(Key::code) {
            self.buffer.insert(c.encode_utf8(&mut [0; 4]));
        }
    }
}

fn is_keyboard_quit(key: Key) -> bool {
    key.code() == KeyCode::Char('g') && key.modifiers() == Modifiers::CONTROL
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

    #[test]
    fn prefix_keys_are_echoed_while_pending() {
        let mut editor = Editor::new();

        press(&mut editor, "C-x");

        assert_eq!(editor.message(), Some("C-x-"));
    }

    #[test]
    fn keyboard_quit_reports_quit() {
        let mut editor = Editor::new();

        press(&mut editor, "C-g");

        assert_eq!(editor.message(), Some("Quit"));
    }

    #[test]
    fn keyboard_quit_aborts_a_pending_prefix() {
        let mut editor = Editor::new();

        press(&mut editor, "C-x C-g");
        assert_eq!(editor.message(), Some("Quit"));

        press(&mut editor, "a");
        assert_eq!(editor.buffer().contents(), "a");
    }

    #[test]
    fn quit_sequence_requests_exit() {
        let mut editor = Editor::new();
        assert!(!editor.should_quit());

        press(&mut editor, "C-x C-c");

        assert!(editor.should_quit());
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

    fn contents_after(text: &str, point: usize, notation: &str) -> (String, usize) {
        let mut editor = editor_with(text, point);
        press(&mut editor, notation);
        (editor.buffer().contents(), editor.buffer().point())
    }

    #[test]
    fn return_inserts_a_newline() {
        assert_eq!(contents_after("ab", 1, "RET"), ("a\nb".to_owned(), 2));
    }

    #[test]
    fn delete_char_removes_char_after_point() {
        assert_eq!(contents_after("abc", 1, "C-d"), ("ac".to_owned(), 1));
        assert_eq!(contents_after("abc", 1, "<delete>"), ("ac".to_owned(), 1));
    }

    #[test]
    fn delete_backward_char_removes_char_before_point() {
        assert_eq!(contents_after("abc", 1, "DEL"), ("bc".to_owned(), 0));
    }

    fn numbered_lines(count: usize) -> String {
        (0..count)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn moving_point_out_of_the_window_scrolls_it() {
        let mut editor = editor_with(&numbered_lines(30), 0);
        editor.resize_window(10);

        press(&mut editor, "M->");
        assert_eq!(editor.viewport().top(), 24);

        press(&mut editor, "M-<");
        assert_eq!(editor.viewport().top(), 0);
    }

    #[test]
    fn resizing_the_window_keeps_point_visible() {
        let mut editor = editor_with(&numbered_lines(30), 0);
        editor.resize_window(20);
        press(
            &mut editor,
            "C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n",
        );
        assert_eq!(editor.viewport().top(), 0);

        editor.resize_window(5);

        assert!(editor.viewport().lines().contains(&12));
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
    fn consecutive_vertical_motions_keep_goal_column() {
        let text = "abcdef\nab\nabcdef";
        assert_eq!(point_after(text, 5, "C-n"), 9);
        assert_eq!(point_after(text, 5, "C-n C-n"), 15);
        assert_eq!(point_after(text, 15, "C-p C-p"), 5);
    }

    #[test]
    fn other_commands_reset_goal_column() {
        let text = "abcdef\nab\nabcdef";
        assert_eq!(point_after(text, 5, "C-n C-b C-n"), 11);
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
