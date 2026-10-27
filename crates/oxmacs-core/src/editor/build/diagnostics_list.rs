use std::path::Path;
use std::sync::Arc;

use crate::editor::Editor;
use crate::editor::special::places::{Place, Places};
use crate::external::diagnostics::Severity;
use crate::files::paths;
use crate::theme::Highlight;

const NAME: &str = "*Diagnostics*";

fn listed(editor: &Editor) -> Places {
    let places = editor
        .builds
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            let label = match diagnostic.severity {
                Severity::Error => ("error".to_owned(), Highlight::Error),
                Severity::Warning => ("warning".to_owned(), Highlight::Warning),
            };
            Place {
                label: Some(label),
                text: Some(diagnostic.message.clone()),
                ..Place::at(&diagnostic.path, diagnostic.start.0, diagnostic.start.1)
            }
        })
        .collect();
    let root = editor.builds.root().map(Path::to_path_buf);
    let title = root.as_deref().map_or_else(
        || "Diagnostics".to_owned(),
        |root| {
            format!(
                "Diagnostics  {}",
                paths::abbreviate(root, editor.home.as_deref())
            )
        },
    );
    Places {
        title,
        places,
        root,
        empty: Some("No problems".to_owned()),
        on_visit: Some(Arc::new(Editor::visit_diagnostic)),
        on_refresh: Some(Arc::new(Editor::recheck)),
        ..Places::default()
    }
}

impl Editor {
    pub(in crate::editor) fn list_diagnostics(&mut self) {
        let list = listed(self);
        self.show_places(NAME, list);
    }

    pub(in crate::editor) fn render_diagnostics_list(&mut self) {
        let list = listed(self);
        self.refresh_places(NAME, list);
    }

    pub(in crate::editor) fn recheck(&mut self) {
        if !self.builds.recheck() {
            self.show_message("Nothing has been checked yet");
        }
    }

    pub(in crate::editor) fn next_lint(&mut self, forward: bool) {
        let line = self.point_line();
        let target = match self.places_view(NAME) {
            Some((0, view)) => view.beyond(line, forward),
            _ => self.builds.step(forward),
        };
        match target {
            Some(index) => self.visit_diagnostic(index),
            None => self.report_no_more_errors(forward),
        }
    }

    fn visit_diagnostic(&mut self, index: usize) {
        let Some(diagnostic) = self.builds.visit(index) else {
            return;
        };
        if let Some((at, view)) = self.places_view(NAME)
            && let Some(buffer) = self.buffers.get(at)
        {
            let id = buffer.id();
            let start = view
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
    use crate::editor::{Cell, Clicks, Editor, Effect, Outcome};
    use crate::external::diagnostics::sample;
    use crate::testing::{lines, press, type_text};

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
                "     1:1    error    bad".to_owned(),
                "     1:4    warning  meh".to_owned(),
                "     1:14   warning  at the end".to_owned(),
                String::new(),
                file("other.rs"),
                "     2:1    warning  elsewhere".to_owned(),
            ]
        );
    }

    #[test]
    fn return_visits_the_problem_at_point() {
        let (_dir, mut editor) = listed();
        on(&mut editor, "     1:4 ");

        press(&mut editor, "RET");

        assert_eq!(editor.mode_line(), "-- main.rs   L1 C3   (Rust)   E1 W2");
        assert_eq!(editor.message(), Some("meh"));
    }

    #[test]
    fn space_shows_a_problem_without_leaving_the_list() {
        let (_dir, mut editor) = listed();
        on(&mut editor, "     2:1 ");
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
            .position(|line| line.starts_with("     1:14"))
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
        assert_eq!(lines(&editor)[4], "     1:2    warning  again");
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
