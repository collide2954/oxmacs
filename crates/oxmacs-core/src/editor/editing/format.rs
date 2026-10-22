use std::env;
use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::command::Command;
use crate::editor::Editor;
use crate::external::managed::AIR;
use crate::external::tools;

#[derive(Debug, Clone, Copy)]
pub struct FormatRequest<'a> {
    pub text: &'a str,
    pub path: &'a Path,
    pub directory: &'a Path,
}

#[derive(Clone, Copy)]
pub struct Formatter {
    pub name: &'static str,
    pub format: fn(&Editor, &FormatRequest<'_>) -> Result<String, String>,
    pub regions: bool,
}

impl fmt::Debug for Formatter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Formatter")
            .field("name", &self.name)
            .field("regions", &self.regions)
            .finish_non_exhaustive()
    }
}

pub(in crate::editor) const RUSTFMT: Formatter = Formatter {
    name: "rustfmt",
    format: |_, request| {
        let edition = tools::cargo_edition(request.directory);
        tools::rustfmt(request.text, request.directory, edition)
            .map_err(|error| format!("rustfmt: {error}"))
    },
    regions: false,
};

pub(in crate::editor) const AIR_FORMAT: Formatter = Formatter {
    name: "air",
    format: |editor, request| {
        let program = editor.air_program()?;
        tools::air_format(&program, request.text, request.path)
            .map_err(|error| format!("air: {error}"))
    },
    regions: true,
};

impl Editor {
    pub fn add_formatter(&mut self, language: crate::syntax::Language, formatter: Formatter) {
        self.formatters.retain(|(known, _)| *known != language);
        self.formatters.push((language, formatter));
        if let Some(scope) = self.registered(language).and_then(|known| known.scope) {
            self.bind(scope, "C-c C-f", Command::FormatBuffer).ok();
        }
    }

    pub(in crate::editor) fn formatter_for(&self, index: usize) -> Option<Formatter> {
        let language = self.buffers.language(index)?;
        self.formatters
            .iter()
            .find(|(known, _)| *known == language)
            .map(|(_, formatter)| *formatter)
    }

    pub(in crate::editor) fn format_buffer(&mut self) {
        let Some(formatter) = self.formatter_for(0) else {
            self.show_message("No formatter for this buffer");
            return;
        };
        let result = match self.buffers.current().selection() {
            Some(range) if !range.is_empty() && formatter.regions => {
                self.format_region(formatter, range)
            }
            _ => self.format_buffer_at(0),
        };
        if let Err(error) = result {
            self.show_message(error);
        }
    }

    pub(in crate::editor) fn format_buffer_at(&mut self, index: usize) -> Result<(), String> {
        let formatter = self
            .formatter_for(index)
            .ok_or_else(|| "No formatter for this buffer".to_owned())?;
        let Some(text) = self.buffers.get(index).map(crate::text::Buffer::contents) else {
            return Ok(());
        };
        let directory = self.file_directory(index);
        let path = self.format_path(index);
        let request = FormatRequest {
            text: &text,
            path: &path,
            directory: &directory,
        };
        let output = (formatter.format)(self, &request)?;
        self.apply_formatted(index, &output, formatter.name);
        Ok(())
    }

    fn file_directory(&self, index: usize) -> PathBuf {
        self.buffers
            .get(index)
            .and_then(|buffer| buffer.file())
            .and_then(|file| file.path.parent())
            .filter(|directory| directory.is_dir())
            .map_or_else(|| self.working_directory.clone(), Path::to_path_buf)
    }

    fn format_path(&self, index: usize) -> PathBuf {
        if let Some(file) = self.buffers.get(index).and_then(|buffer| buffer.file()) {
            return file.path.clone();
        }
        let extension = self
            .buffers
            .language(index)
            .and_then(|language| language.extensions().first().copied())
            .unwrap_or("txt");
        self.working_directory.join(format!("untitled.{extension}"))
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

    fn format_region(&mut self, formatter: Formatter, range: Range<usize>) -> Result<(), String> {
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
        let text = format!("{}\n", stripped.join("\n"));
        let directory = self.file_directory(0);
        let path = self.format_path(0);
        let request = FormatRequest {
            text: &text,
            path: &path,
            directory: &directory,
        };
        let output = (formatter.format)(self, &request)?;
        let margin = lines
            .lines()
            .find(|line| !line.trim().is_empty())
            .and_then(|line| line.get(..indent))
            .unwrap_or_default();
        let reindented: Vec<String> = output
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
        self.show_message(format!("Formatted the selection with {}", formatter.name));
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

    use super::{FormatRequest, Formatter};
    use crate::config::Config;
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, press, type_text};
    use crate::syntax::{Indentation, Language, LanguageSpec, Outline};

    const SQUEEZE: Formatter = Formatter {
        name: "rustfmt",
        format: |_, request| Ok(request.text.replace("  ", " ")),
        regions: false,
    };

    fn rust(text: &str, point: usize) -> Editor {
        let mut editor = editor_with(text, point);
        editor.set_major_mode(Some(Language::RUST));
        editor.add_formatter(Language::RUST, SQUEEZE);
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
    fn formatter_errors_leave_the_buffer_alone() {
        let mut editor = rust("fn  a(", 0);
        editor.add_formatter(
            Language::RUST,
            Formatter {
                name: "rustfmt",
                format: |_, _| Err("rustfmt: error: expected `{`".to_owned()),
                regions: false,
            },
        );

        press(&mut editor, "C-c C-f");

        assert_eq!(editor.buffer().contents(), "fn  a(");
        assert_eq!(editor.message(), Some("rustfmt: error: expected `{`"));
    }

    #[test]
    fn buffers_without_a_formatter_say_so() {
        let mut editor = editor_with("a  b", 0);

        press(&mut editor, "M-x");
        type_text(&mut editor, "format-buffer");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer().contents(), "a  b");
        assert_eq!(editor.message(), Some("No formatter for this buffer"));
    }

    static SHOUTY: LanguageSpec = LanguageSpec {
        name: "Shouty",
        extensions: &["shout"],
        file_names: &[],
        aliases: &[],
        line_comment: None,
        pairs: &[],
        indentation: Indentation::KEEP,
        grammar: None,
        grammars: &[],
        outline: Outline::NONE,
        features: &[],
    };

    #[test]
    fn languages_from_plugins_format_through_their_formatter() {
        let shouty = Language::new(&SHOUTY);
        let mut editor = editor_with("hello", 0);
        editor.add_language(shouty, None);
        editor.add_formatter(
            shouty,
            Formatter {
                name: "shout",
                format: |_, request: &FormatRequest<'_>| {
                    let file = request.path.file_name().unwrap_or_default();
                    Ok(format!(
                        "{} {}",
                        request.text.to_uppercase(),
                        file.display()
                    ))
                },
                regions: false,
            },
        );
        editor.set_major_mode(Some(shouty));

        press(&mut editor, "M-x");
        type_text(&mut editor, "format-buffer");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer().contents(), "HELLO untitled.shout");
        assert_eq!(editor.message(), Some("Formatted with shout"));
    }

    #[test]
    fn languages_from_plugins_format_on_c_c_c_f_in_their_scope() {
        let shouty = Language::new(&SHOUTY);
        let mut editor = editor_with("hello", 0);
        let scope = editor.add_scope("Shouty buffers");
        editor.add_language(shouty, Some(scope));
        editor.add_formatter(
            shouty,
            Formatter {
                name: "shout",
                format: |_, request: &FormatRequest<'_>| Ok(request.text.to_uppercase()),
                regions: false,
            },
        );
        editor.set_major_mode(Some(shouty));

        press(&mut editor, "C-c C-f");

        assert_eq!(editor.buffer().contents(), "HELLO");
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
        editor.add_formatter(Language::RUST, SQUEEZE);
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

    const SPACE_ARROWS: Formatter = Formatter {
        name: "air",
        format: |_, request| Ok(request.text.replace("<-", " <- ")),
        regions: true,
    };

    fn with_air(air_program: &str) -> Config {
        Config {
            air_program: Some(air_program.to_owned()),
            ..Config::default()
        }
    }

    fn r(text: &str, point: usize) -> Editor {
        let mut editor = editor_with(text, point);
        editor.set_major_mode(Some(Language::R));
        editor.add_formatter(Language::R, SPACE_ARROWS);
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
    fn formatters_without_regions_format_the_whole_buffer() {
        let mut editor = rust("fn  a() {}\nfn  b() {}", 0);

        press(&mut editor, "C-SPC C-e C-c C-f");

        assert_eq!(editor.buffer().contents(), "fn a() {}\nfn b() {}");
    }

    #[test]
    fn a_missing_air_program_is_named() {
        let mut editor = editor_with("x<-1", 0);
        editor.set_major_mode(Some(Language::R));
        editor.apply_config(&with_air("oxmacs-no-such-air"));

        press(&mut editor, "C-c C-f");

        assert_eq!(editor.buffer().contents(), "x<-1");
        assert_eq!(
            editor.message(),
            Some("air-program not found: oxmacs-no-such-air")
        );
    }

    #[test]
    fn saving_formats_r_with_air_when_asked_to() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.R");
        fs::write(&path, "x<-1\n").unwrap();
        let mut editor = Editor::new();
        editor.apply_config(&Config {
            format_on_save: true,
            ..Config::default()
        });
        editor.add_formatter(Language::R, SPACE_ARROWS);
        editor.open_file(&path);
        type_text(&mut editor, "y<-2");
        press(&mut editor, "RET");

        press(&mut editor, "C-x C-s");

        assert_eq!(fs::read_to_string(&path).unwrap(), "y <- 2\nx <- 1\n");
    }
}
