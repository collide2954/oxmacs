use crate::editor::Editor;
use crate::syntax::Language;

impl Editor {
    pub fn language(&self) -> Option<Language> {
        self.buffers.language(0)
    }

    pub(super) fn set_major_mode(&mut self, language: Option<Language>) {
        if self.buffers.is_special(0) {
            self.show_message("This buffer keeps its own mode");
            return;
        }
        self.buffers.set_language(0, language);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::editor::Editor;
    use crate::editor::test_support::{press, type_text};
    use crate::syntax::Language;

    fn run(editor: &mut Editor, command: &str) {
        press(editor, "M-x");
        type_text(editor, command);
        press(editor, "RET");
    }

    #[test]
    fn rust_files_open_in_rust_mode() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(dir.path().join("notes.txt"), "notes\n").unwrap();
        let mut editor = Editor::new();

        editor.open_file(&dir.path().join("main.rs"));
        assert_eq!(editor.language(), Some(Language::Rust));
        assert_eq!(editor.mode_line(), "-- main.rs   L1 C0   (Rust)");

        editor.open_file(&dir.path().join("notes.txt"));
        assert_eq!(editor.language(), None);
        assert_eq!(editor.mode_line(), "-- notes.txt   L1 C0");
    }

    #[test]
    fn mode_commands_switch_the_major_mode() {
        let mut editor = Editor::new();

        run(&mut editor, "rust-mode");
        assert_eq!(editor.language(), Some(Language::Rust));

        run(&mut editor, "text-mode");
        assert_eq!(editor.language(), None);
    }

    #[test]
    fn writing_to_a_rust_file_name_turns_on_rust_mode() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        type_text(&mut editor, "fn x() {}");

        editor.finish_write(dir.path().join("x.rs"));

        assert_eq!(editor.language(), Some(Language::Rust));
    }

    #[test]
    fn special_buffers_keep_their_mode() {
        let mut editor = Editor::new();
        editor.about();

        run(&mut editor, "rust-mode");

        assert_eq!(editor.language(), None);
        assert_eq!(editor.message(), Some("This buffer keeps its own mode"));
    }
}
