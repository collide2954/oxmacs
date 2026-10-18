use std::path::Path;

use crate::editor::Editor;
use crate::syntax::Language;
use crate::tools;

impl Editor {
    pub(super) fn rust_format_buffer(&mut self) {
        if let Err(error) = self.format_buffer_at(0) {
            self.show_message(error);
        }
    }

    pub(super) fn format_buffer_at(&mut self, index: usize) -> Result<(), String> {
        if self.buffers.language(index) != Some(Language::Rust) {
            return Err("No formatter for this buffer".to_owned());
        }
        let Some(buffer) = self.buffers.get(index) else {
            return Ok(());
        };
        let directory = buffer
            .file()
            .and_then(|file| file.path.parent())
            .filter(|directory| directory.is_dir())
            .map_or_else(|| self.working_directory.clone(), Path::to_path_buf);
        let edition = tools::cargo_edition(&directory);
        let formatted = (self.formatter)(&buffer.contents(), &directory, edition)
            .map_err(|error| format!("rustfmt: {error}"))?;
        let Some(buffer) = self.buffers.get_mut(index) else {
            return Ok(());
        };
        if buffer.contents() == formatted {
            self.show_message("Already formatted");
            return Ok(());
        }
        let point = buffer.point();
        let line = buffer.line_of(point);
        let column = buffer.column(point);
        buffer.replace_text(&formatted);
        let line = line.min(buffer.len_lines() - 1);
        let start = buffer.line_start(line);
        buffer.set_point(start + column.min(buffer.line_end(line) - start));
        self.show_message("Formatted with rustfmt");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::config::Config;
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, labels, press, type_text};
    use crate::syntax::Language;

    const SQUEEZE: fn(&str, &Path, &str) -> Result<String, String> =
        |text, _, _| Ok(text.replace("  ", " "));

    fn rust(text: &str, point: usize) -> Editor {
        let mut editor = editor_with(text, point);
        editor.set_major_mode(Some(Language::Rust));
        editor.formatter = SQUEEZE;
        editor
    }

    #[test]
    fn c_c_c_f_formats_the_buffer_keeping_point_on_its_line() {
        let mut editor = rust("fn  a() {}\nfn  b() {}", 16);

        press(&mut editor, "C-c C-f");

        assert_eq!(editor.buffer().contents(), "fn a() {}\nfn b() {}");
        assert_eq!(editor.buffer().point(), 15);
        assert_eq!(editor.message(), Some("Formatted with rustfmt"));
    }

    #[test]
    fn formatting_undoes_in_one_step() {
        let mut editor = rust("fn  a() {}\nfn  b() {}", 0);
        press(&mut editor, "C-c C-f");

        press(&mut editor, "C-/");

        assert_eq!(editor.buffer().contents(), "fn  a() {}\nfn  b() {}");
    }

    #[test]
    fn rustfmt_errors_leave_the_buffer_alone() {
        let mut editor = rust("fn  a(", 0);
        editor.formatter = |_, _, _| Err("error: expected `{`".to_owned());

        press(&mut editor, "C-c C-f");

        assert_eq!(editor.buffer().contents(), "fn  a(");
        assert_eq!(editor.message(), Some("rustfmt: error: expected `{`"));
    }

    #[test]
    fn formatting_is_only_offered_in_rust_buffers() {
        let mut editor = editor_with("a  b", 0);

        press(&mut editor, "M-x");
        type_text(&mut editor, "rust-format");

        assert_eq!(labels(&editor), Vec::<String>::new());
    }

    fn saving(format_on_save: bool) -> String {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.rs");
        fs::write(&path, "fn a() {}").unwrap();
        let mut editor = Editor::new();
        editor.apply_config(&Config {
            format_on_save,
            ..Config::default()
        });
        editor.formatter = SQUEEZE;
        editor.open_file(&path);
        type_text(&mut editor, "  ");

        press(&mut editor, "C-x C-s");

        fs::read_to_string(&path).unwrap()
    }

    #[test]
    fn saving_formats_first_when_asked_to() {
        assert_eq!(saving(true), " fn a() {}");
        assert_eq!(saving(false), "  fn a() {}");
    }
}
