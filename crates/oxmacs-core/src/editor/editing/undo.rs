use crate::command::Command;
use crate::editor::Editor;

const TYPING_STEP: usize = 20;

impl Editor {
    pub(in crate::editor) fn close_undo_step(
        &mut self,
        command: Command,
        previous: Option<Command>,
    ) {
        if self.macros.replaying() {
            return;
        }
        let typing = command == Command::SelfInsertCommand;
        if typing && previous == Some(command) && self.typed_in_step < TYPING_STEP {
            self.typed_in_step += 1;
            return;
        }
        self.current_mut().close_undo_step();
        self.typed_in_step = usize::from(typing);
    }

    pub(in crate::editor) fn undo(&mut self) {
        self.shift_selection = false;
        if self.current_mut().undo() {
            self.show_message("Undo");
        } else {
            self.show_message("No further undo information");
        }
    }

    pub(in crate::editor) fn redo(&mut self) {
        self.shift_selection = false;
        if self.current_mut().redo() {
            self.show_message("Redo");
        } else {
            self.show_message("No further redo information");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::testing::{editor_with, press, type_text};

    #[test]
    fn undo_keys_revert_the_last_command() {
        for keys in ["C-/", "C-_", "C-x u", "s-z"] {
            let mut editor = editor_with("ab", 1);
            press(&mut editor, "x");

            press(&mut editor, keys);

            assert_eq!(editor.buffer().contents(), "ab", "{keys}");
            assert_eq!(editor.message(), Some("Undo"), "{keys}");
        }
    }

    #[test]
    fn redo_keys_reapply_an_undone_command() {
        for keys in ["C-?", "C-M-_", "s-S-z"] {
            let mut editor = editor_with("ab", 1);
            press(&mut editor, "x C-/");

            press(&mut editor, keys);

            assert_eq!(editor.buffer().contents(), "axb", "{keys}");
            assert_eq!(editor.message(), Some("Redo"), "{keys}");
        }
    }

    #[test]
    fn typing_merges_into_steps_of_twenty_characters() {
        let mut editor = Editor::new();
        type_text(&mut editor, &"a".repeat(25));

        press(&mut editor, "C-/");
        assert_eq!(editor.buffer().contents(), "a".repeat(20));

        press(&mut editor, "C-/");
        assert_eq!(editor.buffer().contents(), "");
    }

    #[test]
    fn other_commands_split_typing_steps() {
        let mut editor = Editor::new();
        type_text(&mut editor, "ab");
        press(&mut editor, "C-b c");
        assert_eq!(editor.buffer().contents(), "acb");

        press(&mut editor, "C-/");
        assert_eq!(editor.buffer().contents(), "ab");

        press(&mut editor, "C-/");
        assert_eq!(editor.buffer().contents(), "");
    }

    #[test]
    fn undo_restores_a_replaced_selection() {
        let mut editor = editor_with("hello world", 0);

        press(&mut editor, "C-SPC M-f x C-/");

        assert_eq!(editor.buffer().contents(), "hello world");
    }

    #[test]
    fn exhausted_history_is_reported() {
        let mut editor = Editor::new();

        press(&mut editor, "C-/");
        assert_eq!(editor.message(), Some("No further undo information"));

        press(&mut editor, "C-?");
        assert_eq!(editor.message(), Some("No further redo information"));
    }
}
