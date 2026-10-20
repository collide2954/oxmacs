use std::path::{Path, PathBuf};

use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::build::errors;
use crate::editor::page::Page;
use crate::external::diagnostics::{Diagnostic, Severity};
use crate::files::paths;
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*Diagnostics*";
const HELP: &str = "RET visits   SPC shows   g checks again   q quits";

#[derive(Debug, Default)]
pub(in crate::editor) struct DiagnosticsList {
    rows: Vec<(usize, usize)>,
    styles: Vec<LineStyle>,
}

impl DiagnosticsList {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    fn row(&self, line: usize) -> Option<usize> {
        self.rows
            .iter()
            .find(|(row, _)| *row == line)
            .map(|(_, diagnostic)| *diagnostic)
    }

    fn line_of(&self, diagnostic: usize) -> Option<usize> {
        self.rows
            .iter()
            .find(|(_, shown)| *shown == diagnostic)
            .map(|(line, _)| *line)
    }

    fn beyond(&self, line: usize, forward: bool) -> Option<usize> {
        let found = if forward {
            self.rows.iter().find(|(row, _)| *row > line)
        } else {
            self.rows.iter().rev().find(|(row, _)| *row < line)
        };
        found.map(|(_, diagnostic)| *diagnostic)
    }
}

fn shown_path(path: &Path, root: Option<&Path>, home: Option<&Path>) -> String {
    match root.and_then(|root| path.strip_prefix(root).ok()) {
        Some(relative) => relative
            .components()
            .collect::<PathBuf>()
            .display()
            .to_string(),
        None => paths::abbreviate(path, home),
    }
}

fn page(
    diagnostics: &[Diagnostic],
    root: Option<&Path>,
    home: Option<&Path>,
) -> (Page, Vec<(usize, usize)>) {
    let mut page = Page::default();
    let location = root
        .map(|root| format!("  {}", paths::abbreviate(root, home)))
        .unwrap_or_default();
    page.line(&[("Diagnostics", Some(Highlight::Heading)), (&location, None)]);
    page.line(&[(HELP, Some(Highlight::Comment))]);
    let mut rows = Vec::new();
    if diagnostics.is_empty() {
        page.line(&[]);
        page.line(&[("No problems", Some(Highlight::Success))]);
        return (page, rows);
    }
    let places: Vec<String> = diagnostics
        .iter()
        .map(|diagnostic| format!("{}:{}", diagnostic.start.0, diagnostic.start.1))
        .collect();
    let width = places.iter().map(String::len).max().unwrap_or(0);
    let mut file: Option<&Path> = None;
    for (index, (diagnostic, place)) in diagnostics.iter().zip(&places).enumerate() {
        if file != Some(diagnostic.path.as_path()) {
            file = Some(&diagnostic.path);
            page.line(&[]);
            page.heading(&shown_path(&diagnostic.path, root, home));
        }
        let (label, highlight) = match diagnostic.severity {
            Severity::Error => ("error  ", Highlight::Error),
            Severity::Warning => ("warning", Highlight::Warning),
        };
        rows.push((page.styles.len(), index));
        page.line(&[
            ("  ", None),
            (&format!("{place:<width$}"), Some(Highlight::Link)),
            ("  ", None),
            (label, Some(highlight)),
            ("  ", None),
            (&diagnostic.message, None),
        ]);
    }
    (page, rows)
}

impl Editor {
    fn diagnostics_index(&self) -> Option<usize> {
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::Diagnostics(_)))
    }

    fn diagnostics_list(&self) -> Option<&DiagnosticsList> {
        match self.buffers.mode(0)? {
            Mode::Diagnostics(list) => Some(list),
            _ => None,
        }
    }

    pub(in crate::editor) fn in_diagnostics_list(&self) -> bool {
        self.diagnostics_list().is_some()
    }

    pub(in crate::editor) fn list_diagnostics(&mut self) {
        if self.diagnostics_index().is_none() {
            self.beside(|editor| {
                let mut buffer = Buffer::with_text(NAME, "");
                buffer.set_read_only(true);
                editor.add_buffer_with(buffer, Mode::Diagnostics(DiagnosticsList::default()));
            });
        }
        self.render_diagnostics_list();
        self.show_beside(Self::diagnostics_index);
    }

    pub(in crate::editor) fn render_diagnostics_list(&mut self) {
        let Some(index) = self.diagnostics_index() else {
            return;
        };
        let (page, rows) = page(
            &self.lints.diagnostics,
            self.lints.root.as_deref(),
            self.home.as_deref(),
        );
        if let Some(buffer) = self.buffers.get_mut(index) {
            page.fill(buffer);
        }
        if let Some(Mode::Diagnostics(list)) = self.buffers.mode_mut(index) {
            list.rows = rows;
            list.styles = page.styles;
        }
    }

    pub(in crate::editor) fn goto_diagnostic(&mut self) {
        let line = self.point_line();
        match self.diagnostics_list().and_then(|list| list.row(line)) {
            Some(index) => self.visit_diagnostic(index),
            None => self.show_message("No diagnostic here"),
        }
    }

    pub(in crate::editor) fn show_diagnostic(&mut self) {
        let origin = self.selected_window();
        self.goto_diagnostic();
        self.select_window(origin);
    }

    pub(in crate::editor) fn diagnostic_on_line(&self, line: usize) -> Option<usize> {
        self.diagnostics_list()?.row(line)
    }

    pub(in crate::editor) fn recheck(&mut self) {
        match self.lints.last.clone() {
            Some(last) => self.lints.due = Some(last),
            None => self.show_message("Nothing has been checked yet"),
        }
    }

    pub(in crate::editor) fn next_lint(&mut self, forward: bool) {
        let line = self.point_line();
        let target = match self.diagnostics_list() {
            Some(list) => list.beyond(line, forward),
            None => errors::step(self.lints.current, forward, self.lints.diagnostics.len()),
        };
        match target {
            Some(index) => self.visit_diagnostic(index),
            None => self.report_no_more_errors(forward),
        }
    }

    fn visit_diagnostic(&mut self, index: usize) {
        let Some(diagnostic) = self.lints.diagnostics.get(index).cloned() else {
            return;
        };
        self.lints.current = Some(index);
        if let Some(at) = self.diagnostics_index()
            && let Some(buffer) = self.buffers.get(at)
            && let Some(Mode::Diagnostics(list)) = self.buffers.mode(at)
        {
            let id = buffer.id();
            let start = list
                .line_of(index)
                .map_or(0, |line| buffer.line_start(line));
            self.point_windows_at(id, start);
            self.leave_window_showing(id);
        }
        if !diagnostic.path.is_file() {
            self.show_message(format!("No such file: {}", diagnostic.path.display()));
            return;
        }
        if self.open_file(&diagnostic.path) {
            match self.diagnostic_start(index) {
                Some(start) => {
                    self.buffers.current_mut().set_point(start);
                    self.reveal_point();
                }
                None => self.go_to_line(diagnostic.start.0, diagnostic.start.1),
            }
            self.show_message(diagnostic.message);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::editor::build::lint::fixtures::{
        FINISHED, linting, project, report, reported, spawned,
    };
    use crate::editor::test_support::{lines, press, type_text};
    use crate::editor::{Cell, Clicks, Editor, Effect, Outcome};
    use crate::external::diagnostics::sample;

    fn listed() -> (tempfile::TempDir, Editor) {
        let (dir, mut editor) = project();
        reported(&mut editor);
        press(&mut editor, "C-c !");
        (dir, editor)
    }

    fn on(editor: &mut Editor, text: &str) {
        let line = lines(editor)
            .iter()
            .position(|line| line.starts_with(text))
            .unwrap_or_else(|| panic!("{text} missing"));
        let buffer = editor.buffers.current_mut();
        buffer.set_point(buffer.line_start(line));
    }

    fn file(name: &str) -> String {
        Path::new("src").join(name).display().to_string()
    }

    #[test]
    fn c_c_bang_lists_problems_by_file() {
        let (_dir, editor) = listed();

        assert_eq!(editor.buffer().name(), "*Diagnostics*");
        let lines = lines(&editor);
        assert!(lines[0].starts_with("Diagnostics  "), "{}", lines[0]);
        assert_eq!(
            lines[3..],
            [
                file("main.rs"),
                "  1:1   error    bad".to_owned(),
                "  1:4   warning  meh".to_owned(),
                "  1:14  warning  at the end".to_owned(),
                String::new(),
                file("other.rs"),
                "  2:1   warning  elsewhere".to_owned(),
            ]
        );
    }

    #[test]
    fn return_visits_the_problem_at_point() {
        let (_dir, mut editor) = listed();
        on(&mut editor, "  1:4");

        press(&mut editor, "RET");

        assert_eq!(editor.mode_line(), "-- main.rs   L1 C3   (Rust)   E1 W2");
        assert_eq!(editor.message(), Some("meh"));
    }

    #[test]
    fn space_shows_a_problem_without_leaving_the_list() {
        let (_dir, mut editor) = listed();
        on(&mut editor, "  2:1");
        let list = editor.selected_window();

        press(&mut editor, "SPC");

        assert_eq!(editor.selected_window(), list);
        let other = editor.tiling().windows()[0];
        assert!(
            editor
                .pane_of(other)
                .unwrap()
                .mode_line()
                .starts_with("-- other.rs   L2 C0")
        );
    }

    #[test]
    fn clicking_a_problem_visits_it() {
        let (_dir, mut editor) = listed();
        let row = lines(&editor)
            .iter()
            .position(|line| line.starts_with("  1:14"))
            .unwrap();

        editor.click(Cell { row, column: 3 }, Clicks::Single, false);

        assert_eq!(editor.mode_line(), "-- main.rs   L1 C13   (Rust)   E1 W2");
    }

    #[test]
    fn next_error_steps_through_the_latest_lint() {
        let (_dir, mut editor) = project();
        reported(&mut editor);

        press(&mut editor, "M-g n");
        assert_eq!(editor.message(), Some("bad"));
        press(&mut editor, "M-g n");
        assert_eq!(editor.mode_line(), "-- main.rs   L1 C3   (Rust)   E1 W2");
        press(&mut editor, "M-g n M-g n");
        assert!(editor.mode_line().starts_with("-- other.rs   L2 C0"));
        press(&mut editor, "M-g n");
        assert_eq!(editor.message(), Some("Moved past last error"));
        press(&mut editor, "M-g p");
        assert_eq!(editor.message(), Some("at the end"));
    }

    #[test]
    fn a_later_compile_takes_next_error_back() {
        let (_dir, mut editor) = project();
        reported(&mut editor);
        press(&mut editor, "M-x");
        type_text(&mut editor, "compile");
        press(&mut editor, "RET RET");
        let job = spawned(&mut editor).pop().unwrap().id;
        for line in ["error[E0425]: cannot find value", " --> src/other.rs:2:4"] {
            editor.job_output(job, line);
        }
        editor.job_finished(job, &Outcome::Exited(Some(101)));

        press(&mut editor, "M-g n");

        assert!(editor.mode_line().starts_with("-- other.rs   L2 C3"));
    }

    #[test]
    fn the_list_follows_new_results() {
        let (_dir, mut editor) = listed();
        press(&mut editor, "C-x o");

        let job = linting(&mut editor);
        report(&mut editor, job, &[FINISHED.to_owned()]);
        press(&mut editor, "C-c !");

        assert_eq!(lines(&editor)[3], "No problems");
        let job = linting_from_list(&mut editor);
        report(
            &mut editor,
            job,
            &[
                sample("warning", "src/main.rs", (1, 2), (1, 3), "again"),
                FINISHED.to_owned(),
            ],
        );
        assert_eq!(lines(&editor)[4], "  1:2  warning  again");
    }

    fn linting_from_list(editor: &mut Editor) -> crate::editor::JobId {
        press(editor, "g");
        editor
            .take_effects()
            .into_iter()
            .find_map(|effect| match effect {
                Effect::Spawn(job) => Some(job.id),
                _ => None,
            })
            .unwrap()
    }
}
