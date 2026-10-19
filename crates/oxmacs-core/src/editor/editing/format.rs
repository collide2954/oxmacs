use std::env;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::editor::Editor;
use crate::external::managed::AIR;
use crate::external::tools;
use crate::syntax::Language;

impl Editor {
    pub(in crate::editor) fn rust_format_buffer(&mut self) {
        if let Err(error) = self.format_buffer_at(0) {
            self.show_message(error);
        }
    }

    pub(in crate::editor) fn r_format_buffer(&mut self) {
        let result = match self.buffers.current().selection() {
            Some(range) if !range.is_empty() => self.air_region(range),
            _ => self.format_buffer_at(0),
        };
        if let Err(error) = result {
            self.show_message(error);
        }
    }

    pub(in crate::editor) fn format_buffer_at(&mut self, index: usize) -> Result<(), String> {
        match self.buffers.language(index) {
            Some(Language::Rust) => self.rustfmt_buffer_at(index),
            Some(Language::R) => self.air_buffer_at(index),
            _ => Err("No formatter for this buffer".to_owned()),
        }
    }

    fn file_directory(&self, index: usize) -> PathBuf {
        self.buffers
            .get(index)
            .and_then(|buffer| buffer.file())
            .and_then(|file| file.path.parent())
            .filter(|directory| directory.is_dir())
            .map_or_else(|| self.working_directory.clone(), Path::to_path_buf)
    }

    fn rustfmt_buffer_at(&mut self, index: usize) -> Result<(), String> {
        let Some(buffer) = self.buffers.get(index) else {
            return Ok(());
        };
        let directory = self.file_directory(index);
        let edition = tools::cargo_edition(&directory);
        let formatted = (self.formatter)(&buffer.contents(), &directory, edition)
            .map_err(|error| format!("rustfmt: {error}"))?;
        self.apply_formatted(index, &formatted, "rustfmt");
        Ok(())
    }

    pub(in crate::editor) fn air_program(&self) -> Result<PathBuf, String> {
        let search = env::var_os("PATH");
        AIR.locate(
            self.config.air_program.as_deref(),
            self.tools_root.as_deref(),
            search.as_deref(),
        )
        .map(|(path, _)| path)
        .ok_or_else(|| match &self.config.air_program {
            Some(program) => format!("air-program not found: {program}"),
            None => "air is not installed; M-x list-tools installs it".to_owned(),
        })
    }

    fn r_file(&self, index: usize) -> PathBuf {
        self.buffers
            .get(index)
            .and_then(|buffer| buffer.file())
            .map_or_else(
                || self.working_directory.join("untitled.R"),
                |file| file.path.clone(),
            )
    }

    fn air_buffer_at(&mut self, index: usize) -> Result<(), String> {
        let program = self.air_program()?;
        let Some(buffer) = self.buffers.get(index) else {
            return Ok(());
        };
        let formatted = (self.air_formatter)(&program, &buffer.contents(), &self.r_file(index))
            .map_err(|error| format!("air: {error}"))?;
        self.apply_formatted(index, &formatted, "air");
        Ok(())
    }

    fn air_region(&mut self, range: Range<usize>) -> Result<(), String> {
        let program = self.air_program()?;
        let buffer = self.buffers.current();
        let first = buffer.line_of(range.start);
        let last = buffer.line_of(range.end - 1).max(first);
        let (start, end) = (buffer.line_start(first), buffer.line_end(last));
        let lines = buffer.text_in(start..end);
        let indent = lines
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);
        let stripped: Vec<&str> = lines
            .lines()
            .map(|line| line.get(indent..).unwrap_or_else(|| line.trim_start()))
            .collect();
        let formatted = (self.air_formatter)(
            &program,
            &format!("{}\n", stripped.join("\n")),
            &self.r_file(0),
        )
        .map_err(|error| format!("air: {error}"))?;
        let margin = lines
            .lines()
            .find(|line| !line.trim().is_empty())
            .and_then(|line| line.get(..indent))
            .unwrap_or_default();
        let reindented: Vec<String> = formatted
            .trim_end_matches('\n')
            .lines()
            .map(|line| {
                if line.is_empty() {
                    String::new()
                } else {
                    format!("{margin}{line}")
                }
            })
            .collect();
        let buffer = self.buffers.current_mut();
        let contents = format!(
            "{}{}{}",
            buffer.text_in(0..start),
            reindented.join("\n"),
            buffer.text_in(end..buffer.len_chars())
        );
        buffer.replace_text(&contents);
        buffer.deactivate_mark();
        self.show_message("Formatted the selection with air");
        Ok(())
    }

    fn apply_formatted(&mut self, index: usize, formatted: &str, tool: &str) {
        let Some(buffer) = self.buffers.get_mut(index) else {
            return;
        };
        if buffer.contents() == formatted {
            self.show_message("Already formatted");
            return;
        }
        let point = buffer.point();
        let line = buffer.line_of(point);
        let column = buffer.column(point);
        buffer.replace_text(formatted);
        let line = line.min(buffer.len_lines() - 1);
        let start = buffer.line_start(line);
        buffer.set_point(start + column.min(buffer.line_end(line) - start));
        self.show_message(format!("Formatted with {tool}"));
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

    const SPACE_ARROWS: fn(&Path, &str, &Path) -> Result<String, String> =
        |_, text, _| Ok(text.replace("<-", " <- "));

    fn with_air(air_program: &str) -> Config {
        Config {
            air_program: Some(air_program.to_owned()),
            ..Config::default()
        }
    }

    fn r(text: &str, point: usize) -> Editor {
        let mut editor = editor_with(text, point);
        editor.set_major_mode(Some(Language::R));
        editor.apply_config(&with_air(env!("CARGO")));
        editor.air_formatter = SPACE_ARROWS;
        editor
    }

    #[test]
    fn c_c_c_f_formats_r_with_air() {
        let mut editor = r("x<-1\ny<-2\n", 0);

        press(&mut editor, "C-c C-f");

        assert_eq!(editor.buffer().contents(), "x <- 1\ny <- 2\n");
        assert_eq!(editor.message(), Some("Formatted with air"));
    }

    #[test]
    fn a_selection_formats_only_its_lines_keeping_their_indentation() {
        let mut editor = r("f <- function() {\n  a<-1\n  b<-2\n}\nc<-3\n", 20);

        press(&mut editor, "C-SPC C-n C-c C-f");

        assert_eq!(
            editor.buffer().contents(),
            "f <- function() {\n  a <- 1\n  b <- 2\n}\nc<-3\n"
        );
        assert_eq!(editor.message(), Some("Formatted the selection with air"));
        assert!(editor.buffer().selection().is_none());
    }

    #[test]
    fn a_missing_air_program_is_named() {
        let mut editor = r("x<-1", 0);
        editor.apply_config(&with_air("oxmacs-no-such-air"));

        press(&mut editor, "C-c C-f");

        assert_eq!(editor.buffer().contents(), "x<-1");
        assert_eq!(
            editor.message(),
            Some("air-program not found: oxmacs-no-such-air")
        );
    }

    #[test]
    fn air_errors_leave_the_buffer_alone() {
        let mut editor = r("x <- (", 0);
        editor.air_formatter = |_, _, _| Err("unexpected end of input".to_owned());

        press(&mut editor, "C-c C-f");

        assert_eq!(editor.buffer().contents(), "x <- (");
        assert_eq!(editor.message(), Some("air: unexpected end of input"));
    }

    #[test]
    fn saving_formats_r_with_air_when_asked_to() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.R");
        fs::write(&path, "x<-1\n").unwrap();
        let mut editor = Editor::new();
        editor.apply_config(&Config {
            format_on_save: true,
            ..with_air(env!("CARGO"))
        });
        editor.air_formatter = SPACE_ARROWS;
        editor.open_file(&path);
        type_text(&mut editor, "y<-2");
        press(&mut editor, "RET");

        press(&mut editor, "C-x C-s");

        assert_eq!(fs::read_to_string(&path).unwrap(), "y <- 2\nx <- 1\n");
    }
}
