use std::path::{Path, PathBuf};

use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::compile::FINISHED;
use crate::editor::tiling::Axis;
use crate::theme::Highlight;

const PANICKED: &str = "panicked at ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    fn highlight(self) -> Option<Highlight> {
        match self {
            Self::Error => Some(Highlight::Error),
            Self::Warning => Some(Highlight::Warning),
            Self::Note => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Location {
    start: usize,
    line: usize,
    end: usize,
    path: PathBuf,
    row: usize,
    column: usize,
    message: Option<String>,
}

#[derive(Debug, Default)]
pub(super) struct Errors {
    locations: Vec<Location>,
    current: Option<usize>,
    header: Option<(usize, String)>,
    open: bool,
}

impl Errors {
    pub(super) fn scan(&mut self, line: usize, text: &str, directory: &Path) {
        if text.trim().is_empty() {
            self.header = None;
            self.open = false;
        } else if severity_of(text).is_some() {
            self.header = Some((line, text.to_owned()));
            self.open = false;
        } else if let Some(rest) = arrow(text) {
            let header = self.header.take();
            self.push(line, header, rest, directory);
        } else if let Some(rest) = panic(text) {
            self.push(line, None, rest, directory);
        } else if self.open
            && let Some(last) = self.locations.last_mut()
        {
            last.end = line;
        }
    }

    fn push(&mut self, line: usize, header: Option<(usize, String)>, rest: &str, directory: &Path) {
        let Some((path, row, column)) = position(rest) else {
            return;
        };
        let (start, message) = header.map_or((line, None), |(start, text)| (start, Some(text)));
        self.locations.push(Location {
            start,
            line,
            end: line,
            path: directory.join(path),
            row,
            column,
            message,
        });
        self.open = true;
    }

    pub(super) fn line_style(&self, line: usize, text: &str) -> LineStyle {
        let current = self
            .current
            .and_then(|current| self.locations.get(current))
            .is_some_and(|location| location.start == line);
        style(text, current)
    }

    fn step(&self, forward: bool) -> Option<usize> {
        match (self.current, forward) {
            (None, true) => (!self.locations.is_empty()).then_some(0),
            (None, false) => None,
            (Some(index), true) => Some(index + 1).filter(|next| *next < self.locations.len()),
            (Some(index), false) => index.checked_sub(1),
        }
    }

    fn beyond(&self, line: usize, forward: bool) -> Option<usize> {
        if forward {
            self.locations
                .iter()
                .position(|location| location.start > line)
        } else {
            self.locations
                .iter()
                .rposition(|location| location.start < line)
        }
    }

    fn covering(&self, line: usize) -> Option<usize> {
        self.locations
            .iter()
            .position(|location| (location.start..=location.end).contains(&line))
    }
}

fn severity_of(line: &str) -> Option<Severity> {
    [
        ("error", Severity::Error),
        ("warning", Severity::Warning),
        ("note", Severity::Note),
    ]
    .into_iter()
    .find_map(|(label, severity)| {
        let rest = line.strip_prefix(label)?;
        (rest.starts_with(':') || rest.starts_with('[')).then_some(severity)
    })
}

fn arrow(line: &str) -> Option<&str> {
    line.trim_start().strip_prefix("--> ")
}

fn panic(line: &str) -> Option<&str> {
    let at = line.find(PANICKED)?;
    Some(&line[at + PANICKED.len()..])
}

fn position(rest: &str) -> Option<(&str, usize, usize)> {
    let mut parts = rest.trim_end().trim_end_matches(':').rsplitn(3, ':');
    let column = parts.next()?.parse().ok()?;
    let row = parts.next()?.parse().ok()?;
    let path = parts.next().filter(|path| !path.is_empty())?;
    Some((path, row, column))
}

fn location_in(line: &str) -> Option<(&str, usize, usize)> {
    position(arrow(line).or_else(|| panic(line))?)
}

fn style(text: &str, current: bool) -> LineStyle {
    let width = text.chars().count();
    let columns = |bytes: usize| text[..bytes].chars().count();
    let span = if let Some(severity) = severity_of(text) {
        let label = text.find(':').map_or(width, columns);
        severity.highlight().map(|highlight| (0..label, highlight))
    } else if let Some((path, _, _)) = location_in(text) {
        let start = text.find(path).map_or(0, columns);
        let end = text.trim_end().trim_end_matches(':').chars().count();
        Some((start..end, Highlight::Link))
    } else if text == FINISHED {
        Some((0..width, Highlight::Success))
    } else if text.starts_with("Compilation ") {
        Some((0..width, Highlight::Error))
    } else {
        None
    };
    LineStyle {
        background: current.then_some(Highlight::SearchMatch),
        spans: span.into_iter().collect(),
    }
}

impl Editor {
    fn errors(&self) -> Option<&Errors> {
        self.compilation().map(|compilation| &compilation.errors)
    }

    fn in_compilation(&self) -> bool {
        matches!(self.buffers.mode(0), Some(Mode::Compilation(_)))
    }

    pub(super) fn next_error(&mut self, forward: bool) {
        let Some(errors) = self.errors() else {
            self.show_message("No error locations");
            return;
        };
        let target = if self.in_compilation() {
            errors.beyond(self.point_line(), forward)
        } else {
            errors.step(forward)
        };
        match target {
            Some(index) => self.visit_error(index),
            None => self.report_no_more_errors(forward),
        }
    }

    pub(super) fn next_error_no_select(&mut self, forward: bool) {
        let origin = self.selected_window();
        self.next_error(forward);
        self.select_window(origin);
    }

    pub(super) fn compilation_next_error(&mut self, forward: bool) {
        let line = self.point_line();
        let Some(errors) = self.errors() else {
            return;
        };
        match errors
            .beyond(line, forward)
            .and_then(|index| errors.locations.get(index))
        {
            Some(location) => {
                let start = location.start;
                let buffer = self.buffers.current_mut();
                buffer.set_point(buffer.line_start(start));
            }
            None => self.report_no_more_errors(forward),
        }
    }

    fn report_no_more_errors(&mut self, forward: bool) {
        self.show_message(if forward {
            "Moved past last error"
        } else {
            "Moved back before first error"
        });
    }

    pub(super) fn compile_goto_error(&mut self) {
        match self
            .errors()
            .and_then(|errors| errors.covering(self.point_line()))
        {
            Some(index) => self.visit_error(index),
            None => self.show_message("No error here"),
        }
    }

    pub(super) fn error_on_line(&self, line: usize) -> Option<usize> {
        if !self.in_compilation() {
            return None;
        }
        self.errors()?
            .locations
            .iter()
            .position(|location| location.line == line)
    }

    pub(super) fn visit_error(&mut self, index: usize) {
        let Some(compilation) = self.compilation_mut() else {
            return;
        };
        let Some(location) = compilation.errors.locations.get(index).cloned() else {
            return;
        };
        compilation.errors.current = Some(index);
        let Some(output) = self.compilation_index().and_then(|at| self.buffers.get(at)) else {
            return;
        };
        let (output, start) = (output.id(), output.line_start(location.start));
        self.point_windows_at(output, start);
        if self.buffers.current().id() == output {
            if self.tiling().windows().len() == 1 {
                self.split_window(Axis::Columns);
            }
            self.other_window();
        }
        if !location.path.is_file() {
            self.show_message(format!("No such file: {}", location.path.display()));
            return;
        }
        if self.open_file(&location.path) {
            self.go_to_line(location.row, location.column);
            if let Some(message) = location.message {
                self.show_message(message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::editor::test_support::{press, type_text};
    use crate::editor::{Cell, Clicks, Effect, Outcome};

    #[test]
    fn headers_set_the_severity() {
        assert_eq!(
            severity_of("error[E0425]: cannot find"),
            Some(Severity::Error)
        );
        assert_eq!(
            severity_of("warning: unused import"),
            Some(Severity::Warning)
        );
        assert_eq!(severity_of("note: defined here"), Some(Severity::Note));
        assert_eq!(severity_of("errors everywhere"), None);
        assert_eq!(severity_of("   Compiling oxmacs"), None);
    }

    #[test]
    fn locations_come_from_arrows_and_panics() {
        assert_eq!(
            location_in("  --> src/main.rs:3:5"),
            Some(("src/main.rs", 3, 5))
        );
        assert_eq!(
            location_in(r"  --> C:\work\src\lib.rs:10:1"),
            Some((r"C:\work\src\lib.rs", 10, 1))
        );
        assert_eq!(
            location_in("thread 'main' panicked at src/main.rs:2:9:"),
            Some(("src/main.rs", 2, 9))
        );
        assert_eq!(location_in("   Compiling oxmacs v0.12.0"), None);
    }

    #[test]
    fn lines_are_colored_by_what_they_report() {
        assert_eq!(
            style("error[E0425]: cannot find", false).spans,
            [(0..12, Highlight::Error)]
        );
        assert_eq!(
            style("  --> src/main.rs:3:5", true),
            LineStyle {
                background: Some(Highlight::SearchMatch),
                spans: vec![(6..21, Highlight::Link)],
            }
        );
        assert_eq!(
            style("thread 'main' panicked at src/a.rs:2:9:", false).spans,
            [(26..38, Highlight::Link)]
        );
        assert_eq!(
            style("Compilation finished", false).spans,
            [(0..20, Highlight::Success)]
        );
        assert_eq!(style("note: defined here", false).spans, []);
    }

    #[test]
    fn errors_span_from_their_header_to_the_next_blank_line() {
        let mut errors = Errors::default();
        let directory = Path::new("root");
        for (line, text) in [
            "warning: `x` generated 1 warning",
            "",
            "error[E0425]: cannot find",
            " --> src/main.rs:2:5",
            "  |",
            "",
            "thread 'main' panicked at src/main.rs:9:1:",
        ]
        .into_iter()
        .enumerate()
        {
            errors.scan(line, text, directory);
        }

        assert_eq!(
            errors.locations,
            [
                Location {
                    start: 2,
                    line: 3,
                    end: 4,
                    path: directory.join("src/main.rs"),
                    row: 2,
                    column: 5,
                    message: Some("error[E0425]: cannot find".to_owned()),
                },
                Location {
                    start: 6,
                    line: 6,
                    end: 6,
                    path: directory.join("src/main.rs"),
                    row: 9,
                    column: 1,
                    message: None,
                },
            ]
        );
        assert_eq!(errors.covering(4), Some(0));
        assert_eq!(errors.covering(5), None);
    }

    fn project() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src").join("main.rs"),
            "fn main() {\n    oops;\n}\n",
        )
        .unwrap();
        let mut editor = Editor::new();
        editor.open_file(&dir.path().join("src").join("main.rs"));
        press(&mut editor, "M-x");
        type_text(&mut editor, "compile");
        press(&mut editor, "RET RET");
        let job = editor
            .take_effects()
            .into_iter()
            .find_map(|effect| match effect {
                Effect::Spawn(job) => Some(job.id),
                _ => None,
            })
            .unwrap();
        for line in [
            "error[E0425]: cannot find value `oops` in this scope",
            " --> src/main.rs:2:5",
            "",
            "warning: unused variable",
            " --> src/main.rs:1:4",
        ] {
            editor.job_output(job, line);
        }
        editor.job_finished(job, &Outcome::Exited(Some(101)));
        (dir, editor)
    }

    fn compilation_point_line(editor: &Editor) -> usize {
        let window = editor.tiling().windows()[1];
        let pane = editor.pane_of(window).unwrap();
        pane.buffer().line_of(pane.point())
    }

    #[test]
    fn next_error_visits_each_location_in_turn() {
        let (_dir, mut editor) = project();

        press(&mut editor, "M-g n");
        assert_eq!(editor.mode_line(), "-- main.rs   L2 C4   (Rust)");
        assert_eq!(
            editor.message(),
            Some("error[E0425]: cannot find value `oops` in this scope")
        );
        assert_eq!(compilation_point_line(&editor), 3);

        press(&mut editor, "M-g M-n");
        assert_eq!(editor.mode_line(), "-- main.rs   L1 C3   (Rust)");
        assert_eq!(compilation_point_line(&editor), 6);

        press(&mut editor, "C-x `");
        assert_eq!(editor.message(), Some("Moved past last error"));

        press(&mut editor, "M-g p");
        assert_eq!(editor.mode_line(), "-- main.rs   L2 C4   (Rust)");
    }

    #[test]
    fn return_visits_the_error_at_point_in_the_other_window() {
        let (_dir, mut editor) = project();
        press(&mut editor, "C-x o M-< M-n M-n C-n");

        press(&mut editor, "RET");

        assert_eq!(editor.mode_line(), "-- main.rs   L1 C3   (Rust)");
    }

    #[test]
    fn n_and_p_show_errors_without_leaving_the_output() {
        let (_dir, mut editor) = project();
        press(&mut editor, "C-x o M-<");
        let output = editor.selected_window();

        press(&mut editor, "n n");

        assert_eq!(editor.selected_window(), output);
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 6);
        let other = editor.tiling().windows()[0];
        assert_eq!(
            editor.pane_of(other).unwrap().mode_line(),
            "-- main.rs   L1 C3   (Rust)"
        );
    }

    #[test]
    fn clicking_a_location_visits_it() {
        let (_dir, mut editor) = project();
        press(&mut editor, "C-x o M-<");

        editor.click(Cell { row: 4, column: 3 }, Clicks::Single, false);

        assert_eq!(editor.mode_line(), "-- main.rs   L2 C4   (Rust)");
    }

    #[test]
    fn the_current_error_is_marked_in_the_output() {
        let (_dir, mut editor) = project();

        press(&mut editor, "M-g n");

        let window = editor.tiling().windows()[1];
        let pane = editor.pane_of(window).unwrap();
        assert_eq!(pane.line_style(3).background, Some(Highlight::SearchMatch));
        assert_eq!(pane.line_style(4).background, None);
    }
}
