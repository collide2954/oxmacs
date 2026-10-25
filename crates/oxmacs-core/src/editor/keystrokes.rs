use std::mem;

use crate::command::{Command, CommandId};
use crate::editor::Editor;
use crate::input::{Key, KeySequence};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keystroke {
    pub keys: KeySequence,
    pub command: &'static str,
    pub typed: bool,
    pub repeated: bool,
}

#[derive(Debug, Default)]
pub(super) struct Keystrokes {
    watching: bool,
    noted: Vec<Keystroke>,
    count: usize,
    extended: Option<KeySequence>,
}

impl Editor {
    pub fn watch_keystrokes(&mut self) {
        self.keystrokes.watching = true;
    }

    pub fn take_keystrokes(&mut self) -> Vec<Keystroke> {
        mem::take(&mut self.keystrokes.noted)
    }

    pub(super) fn noting_keystrokes(&self) -> Option<usize> {
        (self.keystrokes.watching && !self.macros.replaying()).then_some(self.keystrokes.count)
    }

    pub(super) fn note_keystroke(
        &mut self,
        keys: Vec<Key>,
        command: CommandId,
        before: usize,
        repeated: bool,
    ) {
        if self.keystrokes.count != before {
            return;
        }
        let keys = KeySequence::from(keys);
        if command == Command::ExecuteExtendedCommand.into() {
            self.keystrokes.extended = Some(keys);
        } else {
            self.push_keystroke(keys, command, repeated);
        }
    }

    pub(super) fn note_extended_command(&mut self, command: CommandId) {
        if self.noting_keystrokes().is_none() {
            return;
        }
        let keys = self
            .keystrokes
            .extended
            .take()
            .or_else(|| self.where_is(Command::ExecuteExtendedCommand.into()));
        if let Some(keys) = keys {
            self.push_keystroke(keys, command, false);
        }
    }

    fn push_keystroke(&mut self, keys: KeySequence, command: CommandId, repeated: bool) {
        let typed = command == Command::SelfInsertCommand.into();
        let command = self.commands.name(command);
        self.keystrokes.noted.push(Keystroke {
            keys,
            command,
            typed,
            repeated,
        });
        self.keystrokes.count += 1;
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, press, type_text};

    fn noted(editor: &mut Editor) -> Vec<(String, &'static str, bool)> {
        editor
            .take_keystrokes()
            .into_iter()
            .map(|stroke| (stroke.keys.to_string(), stroke.command, stroke.typed))
            .collect()
    }

    fn chords(editor: &mut Editor) -> Vec<(String, &'static str)> {
        noted(editor)
            .into_iter()
            .filter(|(_, _, typed)| !typed)
            .map(|(keys, command, _)| (keys, command))
            .collect()
    }

    fn stroke(keys: &str, command: &'static str) -> (String, &'static str) {
        (keys.to_owned(), command)
    }

    #[test]
    fn nothing_is_noted_until_keystrokes_are_watched() {
        let mut editor = editor_with("one\ntwo\n", 0);

        press(&mut editor, "C-n");

        assert_eq!(noted(&mut editor), []);
    }

    #[test]
    fn each_key_sequence_that_runs_a_command_is_noted_with_its_name() {
        let mut editor = editor_with("one\ntwo\n", 0);
        editor.watch_keystrokes();

        press(&mut editor, "C-n C-x C-x a RET C-x C-g S-<right>");

        assert_eq!(
            noted(&mut editor),
            [
                ("C-n".to_owned(), "next-line", false),
                ("C-x C-x".to_owned(), "exchange-point-and-mark", false),
                ("a".to_owned(), "self-insert-command", true),
                ("RET".to_owned(), "newline", false),
                ("C-x C-g".to_owned(), "keyboard-quit", false),
                ("S-<right>".to_owned(), "forward-char", false),
            ]
        );
        assert_eq!(noted(&mut editor), []);
    }

    #[test]
    fn each_noted_command_says_whether_it_repeats_the_one_before() {
        let mut editor = editor_with("one\ntwo\nthree\n", 0);
        editor.watch_keystrokes();

        press(&mut editor, "C-n C-n");
        type_text(&mut editor, "a");
        press(&mut editor, "C-n M-x");
        type_text(&mut editor, "next-line");
        press(&mut editor, "RET");

        let repeated: Vec<(&str, bool)> = editor
            .take_keystrokes()
            .into_iter()
            .filter(|stroke| !stroke.typed)
            .map(|stroke| (stroke.command, stroke.repeated))
            .collect();
        assert_eq!(
            repeated,
            [
                ("next-line", false),
                ("next-line", true),
                ("next-line", false),
                ("next-line", false),
            ]
        );
    }

    #[test]
    fn m_x_is_noted_as_the_command_it_runs() {
        let mut editor = editor_with("one\ntwo\n", 0);
        editor.watch_keystrokes();

        press(&mut editor, "M-x");
        assert_eq!(chords(&mut editor), []);
        type_text(&mut editor, "end-of-buffer");
        press(&mut editor, "RET ESC x");
        type_text(&mut editor, "beginning-of-buffer");
        press(&mut editor, "RET M-x C-g");

        assert_eq!(
            chords(&mut editor),
            [
                stroke("M-x", "end-of-buffer"),
                stroke("M-x", "beginning-of-buffer"),
                stroke("C-g", "keyboard-quit"),
            ]
        );
    }

    #[test]
    fn keys_a_macro_plays_are_not_noted_again() {
        let mut editor = editor_with("one\ntwo\nthree\nfour\n", 0);
        press(&mut editor, "<f3> C-n <f4>");
        editor.watch_keystrokes();

        press(&mut editor, "<f4>");
        editor.let_host_play_macros();
        press(&mut editor, "<f4>");
        while editor.macro_playing() {
            editor.play_macro_key();
        }

        assert_eq!(
            chords(&mut editor),
            [
                stroke("<f4>", "kmacro-end-or-call-macro"),
                stroke("<f4>", "kmacro-end-or-call-macro"),
            ]
        );
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 3);
    }
}
