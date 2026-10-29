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

    pub fn set_scratch_language(&mut self, language: Option<Language>) {
        let known = self.languages();
        self.buffers
            .set_scratch_language(language.clone(), known.clone());
        if let Some(index) = self
            .buffers
            .position(|buffer| buffer.name() == super::SCRATCH && buffer.file().is_none())
        {
            self.buffers.set_language(index, language, &known);
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
    use crate::syntax::{Indentation, Language, LanguageSpec, Outline};
    use crate::testing::{kits, press, type_text};
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
        let mut editor = kits();

        editor.open_file(&dir.path().join("main.rs"));
        assert_eq!(editor.language(), Some(Language::rust()));
        assert_eq!(editor.mode_line(), "-- main.rs   L1 C0   (Rust)");

        editor.open_file(&dir.path().join("notes.txt"));
        assert_eq!(editor.language(), None);
        assert_eq!(editor.mode_line(), "-- notes.txt   L1 C0");
    }

    #[test]
    fn toml_files_open_in_toml_mode() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        let mut editor = kits();

        editor.open_file(&dir.path().join("Cargo.toml"));

        assert_eq!(editor.language(), Some(Language::toml()));
        assert_eq!(editor.mode_line(), "-- Cargo.toml   L1 C0   (TOML)");
    }

    #[test]
    fn r_scripts_open_in_r_mode_and_r_mode_is_a_command() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("analysis.R"), "x <- 1\n").unwrap();
        let mut editor = kits();

        editor.open_file(&dir.path().join("analysis.R"));
        assert_eq!(editor.mode_line(), "-- analysis.R   L1 C0   (R)");

        let mut scratch = kits();
        run(&mut scratch, "r-mode");
        assert_eq!(scratch.language(), Some(Language::r()));
    }

    #[test]
    fn toml_comments_use_hashes() {
        let mut editor = kits();
        run(&mut editor, "toml-mode");
        type_text(&mut editor, "a = 1");

        press(&mut editor, "C-a C-x C-;");

        assert_eq!(editor.buffer().contents(), "# a = 1");
    }

    #[test]
    fn mode_commands_switch_the_major_mode() {
        let mut editor = kits();

        run(&mut editor, "rust-mode");
        assert_eq!(editor.language(), Some(Language::rust()));

        run(&mut editor, "text-mode");
        assert_eq!(editor.language(), None);
    }

    fn field_notes_spec() -> LanguageSpec {
        LanguageSpec {
            name: "Field notes".to_owned(),
            extensions: crate::syntax::owned(&["field"]),
            file_names: Vec::new(),
            aliases: Vec::new(),
            line_comment: None,
            pairs: &[],
            indentation: Indentation::KEEP,
            grammar: None,
            grammars: &[],
            outline: Outline::NONE,
            features: &[],
            roots: crate::syntax::Roots::default(),
            name_chars: "",
        }
    }

    #[test]
    fn every_language_added_gets_a_mode_command() {
        let mut editor = kits();
        let field_notes = Language::new(field_notes_spec());
        editor.add_language(field_notes.clone(), None);
        editor.add_language(field_notes.clone(), None);

        let id = editor.command_named("field-notes-mode").unwrap();
        assert_eq!(
            editor.commands.summary(id),
            "Edit the buffer as Field notes"
        );
        assert_eq!(
            editor
                .commands
                .all()
                .filter(|id| editor.commands.name(*id) == "field-notes-mode")
                .count(),
            1
        );
        run(&mut editor, "field-notes-mode");
        assert_eq!(editor.language(), Some(field_notes.clone()));
    }

    #[test]
    fn scratch_can_start_in_a_language_and_comes_back_in_it() {
        let mut editor = kits();
        let field_notes = Language::new(field_notes_spec());
        editor.add_language(field_notes.clone(), None);

        editor.set_scratch_language(Some(field_notes.clone()));
        assert_eq!(editor.buffer().name(), "*scratch*");
        assert_eq!(editor.language(), Some(field_notes.clone()));

        press(&mut editor, "C-x k RET");
        assert_eq!(editor.buffer_names(), ["*scratch*"]);
        assert_eq!(editor.language(), Some(field_notes.clone()));
    }

    #[test]
    fn writing_to_a_rust_file_name_turns_on_rust_mode() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = kits();
        type_text(&mut editor, "fn x() {}");

        editor.finish_write(dir.path().join("x.rs"));

        assert_eq!(editor.language(), Some(Language::rust()));
    }

    #[test]
    fn rust_buffers_are_highlighted_as_you_type() {
        let mut editor = kits();
        run(&mut editor, "rust-mode");

        type_text(&mut editor, "fn main() {}");

        assert_eq!(
            editor.line_style(0).spans,
            [(0..2, Highlight::Keyword), (3..7, Highlight::Function)]
        );
    }

    #[test]
    fn statements_and_definitions_are_found_by_offset() {
        let mut editor = crate::testing::editor_with(
            "x <- 1\n\ny <- df |>\n  head()\nf <- function() {\n  1\n}\n",
            0,
        );
        editor.set_major_mode(Some(Language::r()));

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
        let mut editor = kits();
        editor.about();

        run(&mut editor, "rust-mode");

        assert_eq!(editor.language(), None);
        assert_eq!(editor.message(), Some("This buffer keeps its own mode"));
    }
}
