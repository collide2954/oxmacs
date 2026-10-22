use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::build::compile::FINISHED;
use crate::editor::windows::tiling::Axis;
use crate::text::BufferId;
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
pub struct ErrorAt {
    pub path: String,
    pub row: usize,
    pub column: usize,
    pub link: Range<usize>,
}

pub type ErrorPattern = fn(&str) -> Option<ErrorAt>;

pub(in crate::editor) const BUILT_IN: &[ErrorPattern] = &[arrow_at, panic_at];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Location {
    start: usize,
    line: usize,
    end: usize,
    path: PathBuf,
    row: usize,
    column: usize,
    link: Range<usize>,
    message: Option<String>,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Errors {
    locations: Vec<Location>,
    current: Option<usize>,
    header: Option<(usize, String)>,
    open: bool,
}

impl Errors {
    pub(in crate::editor) fn scan(
        &mut self,
        line: usize,
        text: &str,
        directory: &Path,
        patterns: &[ErrorPattern],
    ) {
        if text.trim().is_empty() {
            self.header = None;
            self.open = false;
        } else if severity_of(text).is_some() {
            self.header = Some((line, text.to_owned()));
            self.open = false;
        } else if let Some(at) = patterns.iter().find_map(|pattern| pattern(text)) {
            let header = self.header.take();
            self.push(line, header, at, directory);
        } else if self.open
            && let Some(last) = self.locations.last_mut()
        {
            last.end = line;
        }
    }

    fn push(
        &mut self,
        line: usize,
        header: Option<(usize, String)>,
        at: ErrorAt,
        directory: &Path,
    ) {
        let (start, message) = header.map_or((line, None), |(start, text)| (start, Some(text)));
        self.locations.push(Location {
            start,
            line,
            end: line,
            path: directory.join(&at.path),
            row: at.row,
            column: at.column,
            link: at.link,
            message,
        });
        self.open = true;
    }

    pub(in crate::editor) fn line_style(&self, line: usize, text: &str) -> LineStyle {
        let current = self
            .current
            .and_then(|current| self.locations.get(current))
            .is_some_and(|location| location.start == line);
        let linked = self
            .locations
            .iter()
            .find(|location| location.line == line)
            .map(|location| location.link.clone());
        style(text, linked, current)
    }

    fn step(&self, forward: bool) -> Option<usize> {
        step(self.current, forward, self.locations.len())
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::editor) enum ErrorSource {
    #[default]
    Compilation,
    Lint,
}

pub(in crate::editor) fn step(
    current: Option<usize>,
    forward: bool,
    count: usize,
) -> Option<usize> {
    match (current, forward) {
        (None, true) => (count > 0).then_some(0),
        (None, false) => None,
        (Some(index), true) => Some(index + 1).filter(|next| *next < count),
        (Some(index), false) => index.checked_sub(1),
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

fn arrow_at(line: &str) -> Option<ErrorAt> {
    let rest = line.trim_start().strip_prefix("--> ")?;
    located(rest, line.len() - rest.len())
}

fn panic_at(line: &str) -> Option<ErrorAt> {
    let at = line.find(PANICKED)? + PANICKED.len();
    located(&line[at..], at)
}

fn located(rest: &str, start: usize) -> Option<ErrorAt> {
    let (path, row, column) = position(rest)?;
    let end = start + rest.trim_end().trim_end_matches(':').len();
    Some(ErrorAt {
        path: path.to_owned(),
        row,
        column,
        link: start..end,
    })
}

fn position(rest: &str) -> Option<(&str, usize, usize)> {
    let mut parts = rest.trim_end().trim_end_matches(':').rsplitn(3, ':');
    let column = parts.next()?.parse().ok()?;
    let row = parts.next()?.parse().ok()?;
    let path = parts.next().filter(|path| !path.is_empty())?;
    Some((path, row, column))
}

fn style(text: &str, link: Option<Range<usize>>, current: bool) -> LineStyle {
    let width = text.chars().count();
    let columns = |bytes: usize| text[..bytes].chars().count();
    let span = if let Some(severity) = severity_of(text) {
        let label = text.find(':').map_or(width, columns);
        severity.highlight().map(|highlight| (0..label, highlight))
    } else if let Some(link) = link.filter(|link| link.end <= text.len()) {
        Some((columns(link.start)..columns(link.end), Highlight::Link))
    } else if text == FINISHED {
        Some((0..width, Highlight::Success))
    } else if text.starts_with("Compilation ") {
        Some((0..width, Highlight::Error))
    } else {
        None
    };
    LineStyle {
        background: current.then_some(Highlight::Region),
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

    pub(in crate::editor) fn next_error(&mut self, forward: bool) {
        if self.errors_from == ErrorSource::Lint && !self.in_compilation() {
            self.next_lint(forward);
            return;
        }
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

    pub(in crate::editor) fn next_error_no_select(&mut self, forward: bool) {
        let origin = self.selected_window();
        self.next_error(forward);
        self.select_window(origin);
    }

    pub(in crate::editor) fn compilation_next_error(&mut self, forward: bool) {
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

    pub(in crate::editor) fn report_no_more_errors(&mut self, forward: bool) {
        self.show_message(if forward {
            "Moved past last error"
        } else {
            "Moved back before first error"
        });
    }

    pub(in crate::editor) fn compile_goto_error(&mut self) {
        match self
            .errors()
            .and_then(|errors| errors.covering(self.point_line()))
        {
            Some(index) => self.visit_error(index),
            None => self.show_message("No error here"),
        }
    }

    pub(in crate::editor) fn error_on_line(&self, line: usize) -> Option<usize> {
        if !self.in_compilation() {
            return None;
        }
        self.errors()?
            .locations
            .iter()
            .position(|location| location.line == line)
    }

    pub(in crate::editor) fn leave_window_showing(&mut self, output: BufferId) {
        if self.buffers.current().id() == output {
            if self.tiling().windows().len() == 1 {
                self.split_window(Axis::Columns);
            }
            self.other_window();
        }
    }

    pub(in crate::editor) fn visit_error(&mut self, index: usize) {
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
        self.leave_window_showing(output);
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
    use crate::editor::{Cell, Clicks, Effect, ErrorAt, Outcome};

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

    fn found(text: &str) -> Option<(String, usize, usize)> {
        BUILT_IN
            .iter()
            .find_map(|pattern| pattern(text))
            .map(|at| (at.path, at.row, at.column))
    }

    #[test]
    fn locations_come_from_arrows_and_panics() {
        assert_eq!(
            found("  --> src/main.rs:3:5"),
            Some(("src/main.rs".to_owned(), 3, 5))
        );
        assert_eq!(
            found(r"  --> C:\work\src\lib.rs:10:1"),
            Some((r"C:\work\src\lib.rs".to_owned(), 10, 1))
        );
        assert_eq!(
            found("thread 'main' panicked at src/main.rs:2:9:"),
            Some(("src/main.rs".to_owned(), 2, 9))
        );
        assert_eq!(found("   Compiling oxmacs v0.12.0"), None);
    }

    fn found_link(text: &str) -> Option<Range<usize>> {
        BUILT_IN
            .iter()
            .find_map(|pattern| pattern(text))
            .map(|at| at.link)
    }

    #[test]
    fn lines_are_colored_by_what_they_report() {
        assert_eq!(
            style("error[E0425]: cannot find", None, false).spans,
            [(0..12, Highlight::Error)]
        );
        let arrow = "  --> src/main.rs:3:5";
        assert_eq!(
            style(arrow, found_link(arrow), true),
            LineStyle {
                background: Some(Highlight::Region),
                spans: vec![(6..21, Highlight::Link)],
            }
        );
        let panic = "thread 'main' panicked at src/a.rs:2:9:";
        assert_eq!(
            style(panic, found_link(panic), false).spans,
            [(26..38, Highlight::Link)]
        );
        assert_eq!(
            style("Compilation finished", None, false).spans,
            [(0..20, Highlight::Success)]
        );
        assert_eq!(style("note: defined here", None, false).spans, []);
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
            errors.scan(line, text, directory, BUILT_IN);
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
                    link: 5..20,
                    message: Some("error[E0425]: cannot find".to_owned()),
                },
                Location {
                    start: 6,
                    line: 6,
                    end: 6,
                    path: directory.join("src/main.rs"),
                    row: 9,
                    column: 1,
                    link: 26..41,
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

    fn traceback(line: &str) -> Option<ErrorAt> {
        let rest = line.trim_start().strip_prefix("File \"")?;
        let (path, rest) = rest.split_once("\", line ")?;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let start = line.find("File \"")? + 6;
        Some(ErrorAt {
            path: path.to_owned(),
            row: digits.parse().ok()?,
            column: 1,
            link: start..start + path.len(),
        })
    }

    #[test]
    fn plugins_add_patterns_that_make_their_errors_clickable() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.py"), "x = 1\nprint(y)\n").unwrap();
        let mut editor = Editor::new();
        editor.add_error_pattern(traceback);

        editor.compile_in("python a.py", dir.path().to_owned());
        let job = editor
            .take_effects()
            .into_iter()
            .find_map(|effect| match effect {
                Effect::Spawn(job) => Some(job),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            (job.program.as_str(), job.args.as_slice()),
            ("python", &["a.py".to_owned()][..])
        );
        for line in [
            "Traceback (most recent call last):",
            "  File \"a.py\", line 2, in <module>",
            "NameError: name 'y' is not defined",
        ] {
            editor.job_output(job.id, line);
        }
        editor.job_finished(job.id, &Outcome::Exited(Some(1)));
        let window = editor.tiling().windows()[1];
        assert_eq!(
            editor.pane_of(window).unwrap().line_style(4).spans,
            [(8..12, Highlight::Link)]
        );

        press(&mut editor, "M-g n");

        assert_eq!(editor.buffer().name(), "a.py");
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 1);
    }

    #[test]
    fn the_current_error_is_marked_in_the_output() {
        let (_dir, mut editor) = project();

        press(&mut editor, "M-g n");

        let window = editor.tiling().windows()[1];
        let pane = editor.pane_of(window).unwrap();
        assert_eq!(pane.line_style(3).background, Some(Highlight::Region));
        assert_eq!(pane.line_style(4).background, None);
    }
}
