use std::ops::Range;

use crate::editor::Editor;
use crate::syntax::Language;
use crate::theme::Highlight;

impl Editor {
    pub fn language(&self) -> Option<Language> {
        self.buffers.language(0)
    }

    pub fn sync_syntax(&mut self) {
        self.buffers.sync_syntax(0);
    }

    pub fn statement_at(&mut self, point: usize) -> Option<Range<usize>> {
        self.sync_syntax();
        let buffer = self.buffers.current();
        self.buffers.syntax(0)?.statement_at(buffer, point)
    }

    pub fn statement_after(&mut self, position: usize) -> Option<usize> {
        self.sync_syntax();
        let buffer = self.buffers.current();
        self.buffers.syntax(0)?.statement_after(buffer, position)
    }

    pub fn defun_around(&mut self, point: usize) -> Option<Range<usize>> {
        self.sync_syntax();
        let buffer = self.buffers.current();
        self.buffers.syntax(0)?.defun_around(buffer, point)
    }

    pub fn wants_reparse(&self) -> bool {
        self.buffers.syntax_stale(0)
            || self
                .visible_buffers()
                .into_iter()
                .any(|index| self.buffers.syntax_stale(index))
    }

    pub fn reparse(&mut self) {
        self.buffers.sync_syntax(0);
        for index in self.visible_buffers() {
            self.buffers.sync_syntax(index);
        }
    }

    pub(in crate::editor) fn syntax_highlights_at(
        &self,
        index: usize,
        line: usize,
    ) -> Vec<(Range<usize>, Highlight)> {
        match (self.buffers.syntax(index), self.buffers.get(index)) {
            (Some(syntax), Some(buffer)) => syntax.highlights(buffer, line),
            _ => Vec::new(),
        }
    }

    pub fn set_major_mode(&mut self, language: Option<Language>) {
        if self.buffers.is_special(0) {
            self.show_message("This buffer keeps its own mode");
            return;
        }
        let known = self.languages();
        self.buffers.set_language(0, language, &known);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::editor::Editor;
    use crate::editor::test_support::{press, type_text};
    use crate::syntax::Language;
    use crate::theme::Highlight;

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
        assert_eq!(editor.language(), Some(Language::RUST));
        assert_eq!(editor.mode_line(), "-- main.rs   L1 C0   (Rust)");

        editor.open_file(&dir.path().join("notes.txt"));
        assert_eq!(editor.language(), None);
        assert_eq!(editor.mode_line(), "-- notes.txt   L1 C0");
    }

    #[test]
    fn toml_files_open_in_toml_mode() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        let mut editor = Editor::new();

        editor.open_file(&dir.path().join("Cargo.toml"));

        assert_eq!(editor.language(), Some(Language::TOML));
        assert_eq!(editor.mode_line(), "-- Cargo.toml   L1 C0   (TOML)");
    }

    #[test]
    fn r_scripts_open_in_r_mode_and_r_mode_is_a_command() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("analysis.R"), "x <- 1\n").unwrap();
        let mut editor = Editor::new();

        editor.open_file(&dir.path().join("analysis.R"));
        assert_eq!(editor.mode_line(), "-- analysis.R   L1 C0   (R)");

        let mut scratch = Editor::new();
        run(&mut scratch, "r-mode");
        assert_eq!(scratch.language(), Some(Language::R));
    }

    #[test]
    fn toml_comments_use_hashes() {
        let mut editor = Editor::new();
        run(&mut editor, "toml-mode");
        type_text(&mut editor, "a = 1");

        press(&mut editor, "C-a C-x C-;");

        assert_eq!(editor.buffer().contents(), "# a = 1");
    }

    #[test]
    fn mode_commands_switch_the_major_mode() {
        let mut editor = Editor::new();

        run(&mut editor, "rust-mode");
        assert_eq!(editor.language(), Some(Language::RUST));

        run(&mut editor, "text-mode");
        assert_eq!(editor.language(), None);
    }

    #[test]
    fn writing_to_a_rust_file_name_turns_on_rust_mode() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        type_text(&mut editor, "fn x() {}");

        editor.finish_write(dir.path().join("x.rs"));

        assert_eq!(editor.language(), Some(Language::RUST));
    }

    #[test]
    fn rust_buffers_are_highlighted_as_you_type() {
        let mut editor = Editor::new();
        run(&mut editor, "rust-mode");

        type_text(&mut editor, "fn main() {}");

        assert_eq!(
            editor.line_style(0).spans,
            [(0..2, Highlight::Keyword), (3..7, Highlight::Function)]
        );
    }

    #[test]
    fn statements_and_definitions_are_found_by_offset() {
        let mut editor = crate::editor::test_support::editor_with(
            "x <- 1\n\ny <- df |>\n  head()\nf <- function() {\n  1\n}\n",
            0,
        );
        editor.set_major_mode(Some(Language::R));

        assert_eq!(editor.statement_at(2), Some(0..6));
        assert_eq!(editor.statement_after(6), Some(8));
        assert_eq!(editor.statement_at(8), Some(8..27));
        assert_eq!(editor.defun_around(40), Some(28..51));
        assert_eq!(editor.defun_around(2), None);
        editor.set_major_mode(None);
        assert_eq!(editor.statement_at(2), None);
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
