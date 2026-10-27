use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::config::RustLint;
use crate::editor::build::Finished;
use crate::editor::decorations::{Lasts, Look, Owner};
use crate::editor::{Editor, JobEvent, JobId, JobSpec, Outcome};
use crate::external::diagnostics::{self, Diagnostic, LintLine, Severity};
use crate::external::tools;
use crate::syntax::Language;
use crate::text::Buffer;

#[derive(Clone, Copy)]
pub struct Linter {
    pub start: fn(&Editor, &Path) -> Option<LintRun>,
    pub read: fn(&str, &Path) -> LintLine,
    pub complete: fn(&Outcome) -> bool,
}

impl fmt::Debug for Linter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Linter").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintRun {
    pub root: PathBuf,
    pub program: String,
    pub args: Vec<String>,
    pub label: &'static str,
    pub name: String,
}

fn position(buffer: &Buffer, (row, column): (usize, usize)) -> usize {
    let line = row
        .saturating_sub(1)
        .min(buffer.line_of(buffer.len_chars()));
    (buffer.line_start(line) + column.saturating_sub(1)).min(buffer.line_end(line))
}

fn subcommand(tool: RustLint) -> Option<&'static str> {
    match tool {
        RustLint::Check => Some("check"),
        RustLint::Clippy => Some("clippy"),
        RustLint::Off => None,
    }
}

fn running(tool: RustLint) -> &'static str {
    match tool {
        RustLint::Clippy => "Clippy",
        RustLint::Check | RustLint::Off => "Checking",
    }
}

fn cargo_lint(editor: &Editor, path: &Path) -> Option<LintRun> {
    let tool = editor.settings.rust_lint;
    let subcommand = subcommand(tool)?;
    let root = path.parent().and_then(tools::cargo_root)?;
    Some(LintRun {
        root,
        program: "cargo".to_owned(),
        args: [
            subcommand,
            "--workspace",
            "--all-targets",
            "--message-format=json",
        ]
        .map(str::to_owned)
        .to_vec(),
        label: running(tool),
        name: format!("cargo {subcommand}"),
    })
}

pub(in crate::editor) const CARGO_LINT: Linter = Linter {
    start: cargo_lint,
    read: diagnostics::parse,
    complete: |_| false,
};

impl Editor {
    #[cfg(test)]
    pub fn add_linter(&mut self, language: Language, linter: Linter) {
        self.builds.add_linter(language, linter);
    }

    pub fn diagnostics_revision(&self) -> u64 {
        self.builds.revision()
    }

    pub fn diagnostic_counts(&self) -> Vec<(PathBuf, usize, usize)> {
        self.builds.counts()
    }

    pub fn replace_diagnostics(&mut self, path: &Path, found: Vec<Diagnostic>) {
        self.builds.replace(path, found);
        self.diagnostics_changed();
    }

    fn diagnostics_changed(&mut self) {
        self.undecorate(|decoration, _| matches!(decoration.owner, Owner::Lint(_)));
        for index in 0..self.buffers.len() {
            self.mark_buffer(index);
        }
        self.render_diagnostics_list();
    }

    pub(in crate::editor) fn request_lint(&mut self, language: Language, path: &Path) {
        if !self.serves(language.name(), path) {
            self.builds.request(language, path);
        }
    }

    pub(in crate::editor) fn start_due_lint(&mut self) {
        let Some(due) = self.builds.take_due() else {
            return;
        };
        let Some(run) = (due.2.start)(self, &due.1) else {
            return;
        };
        for stale in self.builds.stale(due.0) {
            self.kill_job(stale);
        }
        let spec = JobSpec {
            program: run.program.clone(),
            args: run.args.clone(),
            directory: run.root.clone(),
            env: Vec::new(),
            label: "lint",
            name: run.name.clone(),
            buffer: None,
            interactive: false,
            input: None,
        };
        let job = self.start_job(spec, Self::lint_event);
        self.builds.started(job, due, run);
    }

    fn lint_event(&mut self, job: JobId, event: JobEvent<'_>) {
        match event {
            JobEvent::Line(line) | JobEvent::Output(line) => self.builds.output(job, line),
            JobEvent::Finished(outcome) => self.lint_finished(job, outcome),
        }
    }

    fn lint_finished(&mut self, job: JobId, outcome: &Outcome) {
        let languages: HashMap<PathBuf, Option<Language>> = self
            .builds
            .diagnostics()
            .iter()
            .map(|diagnostic| (diagnostic.path.clone(), self.language_for(&diagnostic.path)))
            .collect();
        let language_of = |path: &Path| languages.get(path).copied().flatten();
        match self.builds.finished(job, outcome, language_of) {
            Finished::Changed => self.diagnostics_changed(),
            Finished::Failed(message) => self.show_message(message),
            Finished::Unknown => {}
        }
    }

    pub(in crate::editor) fn mark_buffer(&mut self, index: usize) {
        let Some(buffer) = self.buffers.get(index) else {
            return;
        };
        let Some(identity) = buffer.file().map(|file| file.identity().to_path_buf()) else {
            return;
        };
        let marked: Vec<(usize, Range<usize>, Severity)> = self
            .builds
            .on_file(&identity)
            .map(|(diagnostic, found)| {
                let start = position(buffer, found.start);
                let end = position(buffer, found.end).max(start);
                (diagnostic, start..end, found.severity)
            })
            .collect();
        let id = buffer.id();
        for (diagnostic, range, severity) in marked {
            self.decorate(
                id,
                range,
                Look::Wave(severity),
                Lasts::Cleared,
                Owner::Lint(diagnostic),
            );
        }
    }

    fn marks_in<'a>(
        &'a self,
        buffer: &'a Buffer,
    ) -> impl Iterator<Item = (Range<usize>, &'a Diagnostic)> {
        self.decorations_of(buffer)
            .filter_map(|(decoration, span)| match decoration.owner {
                Owner::Lint(diagnostic) => Some((span, self.builds.diagnostic(diagnostic)?)),
                _ => None,
            })
    }

    pub(in crate::editor) fn diagnostic_start(&self, diagnostic: usize) -> Option<usize> {
        let buffer = self.buffers.current();
        self.decorations_of(buffer)
            .find(|(decoration, _)| decoration.owner == Owner::Lint(diagnostic))
            .map(|(_, span)| span.start)
    }

    pub fn echo(&self) -> Option<&str> {
        self.message().or_else(|| self.diagnostic_at_point())
    }

    pub fn diagnostic_at_point(&self) -> Option<&str> {
        let buffer = self.buffers.current();
        let point = buffer.point();
        self.marks_in(buffer)
            .filter(|(span, _)| (span.start..=span.end).contains(&point))
            .map(|(_, diagnostic)| diagnostic)
            .min_by_key(|diagnostic| diagnostic.severity)
            .map(|diagnostic| diagnostic.message.as_str())
    }

    pub(in crate::editor) fn lint_status(&self, identity: &Path) -> Option<String> {
        self.builds.status(identity, self.language_for(identity))
    }
}
#[cfg(test)]
pub(crate) mod fixtures {
    use std::fs;

    use crate::editor::{Editor, Effect, Job, JobId, Outcome};
    use crate::external::diagnostics::sample;
    use crate::testing::{press, type_text};

    pub(crate) const FINISHED: &str = r#"{"reason":"build-finished","success":false}"#;

    pub(crate) fn project() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src").join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(dir.path().join("src").join("other.rs"), "\nfn f() {}\n").unwrap();
        let mut editor = Editor::new();
        editor.open_file(&dir.path().join("src").join("main.rs"));
        (dir, editor)
    }

    pub(crate) fn save(editor: &mut Editor) {
        press(editor, "C-e");
        type_text(editor, " ");
        press(editor, "C-x C-s");
    }

    pub(crate) fn spawned(editor: &mut Editor) -> Vec<Job> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Spawn(job) => Some(job),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn linting(editor: &mut Editor) -> JobId {
        save(editor);
        spawned(editor).pop().unwrap().id
    }

    pub(crate) fn report(editor: &mut Editor, job: JobId, lines: &[String]) {
        for line in lines {
            editor.job_output(job, line);
        }
        editor.job_finished(job, &Outcome::Exited(Some(101)));
    }

    pub(crate) fn reported(editor: &mut Editor) {
        let job = linting(editor);
        report(
            editor,
            job,
            &[
                sample("error", "src/main.rs", (1, 1), (1, 3), "bad"),
                sample("warning", "src/main.rs", (1, 4), (1, 8), "meh"),
                sample("warning", "src/main.rs", (1, 14), (1, 14), "at the end"),
                sample("warning", "src/other.rs", (2, 1), (2, 3), "elsewhere"),
                FINISHED.to_owned(),
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use std::path::{Path, PathBuf};

    use serde_json::json;

    use super::fixtures::{FINISHED, linting, project, report, reported, save, spawned};
    use crate::config::Config;
    use crate::editor::{Editor, Job, LintLine, LintRun, Linter, Outcome, ProcessStatus};
    use crate::external::diagnostics::{Diagnostic, Severity, sample};
    use crate::syntax::{Indentation, Language, LanguageSpec, Outline};
    use crate::testing::{press, type_text};

    fn waves(editor: &Editor, line: usize) -> Vec<(std::ops::Range<usize>, Severity)> {
        editor
            .pane()
            .line_decorations(line, std::time::Instant::now())
            .waves
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

    fn shout_line(line: &str, root: &Path) -> LintLine {
        let mut parts = line.splitn(4, ':');
        let (Some(file), Some(row), Some(column), Some(message)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return LintLine::Other;
        };
        let (Ok(row), Ok(column)) = (row.parse::<usize>(), column.parse::<usize>()) else {
            return LintLine::Other;
        };
        LintLine::Diagnostic(Diagnostic {
            path: root.join(file),
            start: (row, column),
            end: (row, column + 1),
            severity: Severity::Error,
            message: message.trim().to_owned(),
        })
    }

    const SHOUT_LINT: Linter = Linter {
        start: |_, path| {
            Some(LintRun {
                root: path.parent()?.to_owned(),
                program: "shoutlint".to_owned(),
                args: vec![path.display().to_string()],
                label: "Shouting",
                name: "shoutlint".to_owned(),
            })
        },
        read: shout_line,
        complete: |outcome| matches!(outcome, Outcome::Exited(Some(0 | 1))),
    };

    fn shouting(editor: &mut Editor, dir: &Path) -> PathBuf {
        let shouty = Language::new(&SHOUTY);
        editor.add_language(shouty, None);
        editor.add_linter(shouty, SHOUT_LINT);
        let path = dir.join("a.shout");
        fs::write(&path, "hello there\n").unwrap();
        editor.open_file(&path);
        path
    }

    #[test]
    fn languages_from_plugins_lint_their_files_on_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        let path = shouting(&mut editor, dir.path());

        save(&mut editor);

        let jobs = spawned(&mut editor);
        assert_eq!(jobs[0].program, "shoutlint");
        assert_eq!(jobs[0].args, [path.display().to_string()]);
        assert_eq!(jobs[0].directory, dir.path());
        assert_eq!(editor.pane().status().lint.as_deref(), Some("Shouting"));
        editor.job_output(jobs[0].id, "a.shout:1:1: too quiet");
        editor.job_finished(jobs[0].id, &Outcome::Exited(Some(1)));
        assert_eq!(editor.pane().status().lint.as_deref(), Some("E1"));
        press(&mut editor, "C-a");
        assert_eq!(editor.diagnostic_at_point(), Some("too quiet"));
    }

    #[test]
    fn a_failed_check_keeps_earlier_results_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        shouting(&mut editor, dir.path());
        save(&mut editor);
        let first = spawned(&mut editor)[0].id;
        editor.job_output(first, "a.shout:1:1: too quiet");
        editor.job_finished(first, &Outcome::Exited(Some(1)));

        save(&mut editor);
        let second = spawned(&mut editor)[0].id;
        editor.job_output(second, "error: bad settings");
        editor.job_finished(second, &Outcome::Exited(Some(2)));

        assert_eq!(editor.message(), Some("shoutlint: error: bad settings"));
        assert_eq!(editor.pane().status().lint.as_deref(), Some("E1"));
    }

    #[test]
    fn each_language_keeps_its_own_results() {
        let (dir, mut editor) = project();
        reported(&mut editor);
        shouting(&mut editor, dir.path());
        save(&mut editor);
        let job = spawned(&mut editor)[0].id;

        editor.job_finished(job, &Outcome::Exited(Some(0)));
        editor.open_file(&dir.path().join("src").join("main.rs"));

        assert_eq!(editor.pane().status().lint.as_deref(), Some("E1 W2"));
    }
    #[test]
    fn saving_a_rust_file_checks_its_project() {
        let (dir, mut editor) = project();

        save(&mut editor);

        let jobs = spawned(&mut editor);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].program, "cargo");
        assert_eq!(
            jobs[0].args,
            [
                "check",
                "--workspace",
                "--all-targets",
                "--message-format=json"
            ]
        );
        assert_eq!(jobs[0].directory, dir.path());
        assert_eq!(
            editor.mode_line(),
            "-- main.rs   L1 C13   (Rust)   Checking"
        );
    }

    #[test]
    fn clippy_can_replace_check_and_lints_can_be_turned_off() {
        for (setting, expected) in [("clippy", Some("clippy")), ("off", None)] {
            let (_dir, mut editor) = project();
            editor.apply_config(&Config::default().with("rust-lint", json!(setting)).unwrap());

            save(&mut editor);

            let jobs = spawned(&mut editor);
            assert_eq!(
                jobs.first().map(|job| job.args[0].as_str()),
                expected,
                "{setting}"
            );
        }
    }

    #[test]
    fn results_are_counted_in_the_mode_line() {
        let (_dir, mut editor) = project();
        let job = linting(&mut editor);

        report(
            &mut editor,
            job,
            &[
                sample("error", "src/main.rs", (1, 1), (1, 3), "bad"),
                sample("warning", "src/main.rs", (1, 4), (1, 8), "meh"),
                sample("warning", "src/other.rs", (1, 1), (1, 2), "elsewhere"),
                FINISHED.to_owned(),
            ],
        );
        assert_eq!(editor.mode_line(), "-- main.rs   L1 C13   (Rust)   E1 W1");

        let job = linting(&mut editor);
        report(&mut editor, job, &[FINISHED.to_owned()]);
        assert_eq!(editor.mode_line(), "-- main.rs   L1 C14   (Rust)");
    }

    #[test]
    fn saving_again_restarts_the_lint() {
        let (_dir, mut editor) = project();
        let first = linting(&mut editor);

        save(&mut editor);
        editor.take_effects();

        assert_eq!(editor.job_status(first), Some(ProcessStatus::Stopping));
        report(
            &mut editor,
            first,
            &[
                sample("error", "src/main.rs", (1, 1), (1, 3), "stale"),
                FINISHED.to_owned(),
            ],
        );
        assert_eq!(
            editor.mode_line(),
            "-- main.rs   L1 C14   (Rust)   Checking"
        );
    }

    #[test]
    fn cargo_failures_are_reported_and_keep_earlier_results() {
        let (_dir, mut editor) = project();
        let job = linting(&mut editor);
        report(
            &mut editor,
            job,
            &[
                sample("error", "src/main.rs", (1, 1), (1, 3), "bad"),
                FINISHED.to_owned(),
            ],
        );

        let job = linting(&mut editor);
        report(
            &mut editor,
            job,
            &["error: failed to parse manifest at `Cargo.toml`".to_owned()],
        );

        assert_eq!(
            editor.message(),
            Some("cargo check: error: failed to parse manifest at `Cargo.toml`")
        );
        assert_eq!(editor.mode_line(), "-- main.rs   L1 C14   (Rust)   E1");
    }

    #[test]
    fn only_rust_files_are_linted() {
        let (dir, mut editor) = project();
        editor.open_file(&dir.path().join("notes.txt"));

        save(&mut editor);

        assert_eq!(spawned(&mut editor), Vec::<Job>::new());
    }

    #[test]
    fn results_underline_their_ranges() {
        let (_dir, mut editor) = project();

        reported(&mut editor);

        assert_eq!(
            waves(&editor, 0),
            [
                (0..2, Severity::Error),
                (3..7, Severity::Warning),
                (13..14, Severity::Warning)
            ]
        );
        assert_eq!(waves(&editor, 1), []);
    }

    #[test]
    fn plugins_can_replace_the_diagnostics_of_one_file() {
        let (dir, mut editor) = project();
        reported(&mut editor);
        let main = dir.path().join("src").join("main.rs");
        let found = |start, end, severity, message: &str| Diagnostic {
            path: main.clone(),
            start,
            end,
            severity,
            message: message.to_owned(),
        };

        editor.replace_diagnostics(
            &main,
            vec![
                found((1, 4), (1, 8), Severity::Error, "from a plugin"),
                found((1, 10), (1, 11), Severity::Warning, "and another"),
            ],
        );

        assert_eq!(
            waves(&editor, 0),
            [(3..7, Severity::Error), (9..10, Severity::Warning)]
        );
        assert_eq!(editor.pane().status().lint.as_deref(), Some("E1 W1"));
        press(&mut editor, "C-a C-f C-f C-f C-f");
        assert_eq!(editor.diagnostic_at_point(), Some("from a plugin"));
        editor.open_file(&dir.path().join("src").join("other.rs"));
        assert_eq!(waves(&editor, 1), [(0..2, Severity::Warning)]);

        editor.replace_diagnostics(&main, Vec::new());
        editor.open_file(&main);
        assert_eq!(waves(&editor, 0), []);
        assert_eq!(editor.pane().status().lint, None);
    }

    #[test]
    fn diagnostics_are_counted_per_file_and_each_change_bumps_a_revision() {
        let (dir, mut editor) = project();
        let main = dir.path().join("src").join("main.rs");
        let other = dir.path().join("src").join("other.rs");
        let found = |path: &Path, severity| Diagnostic {
            path: path.to_path_buf(),
            start: (1, 1),
            end: (1, 2),
            severity,
            message: "x".to_owned(),
        };
        let before = editor.diagnostics_revision();

        editor.replace_diagnostics(
            &main,
            vec![
                found(&main, Severity::Error),
                found(&main, Severity::Error),
                found(&main, Severity::Warning),
            ],
        );
        editor.replace_diagnostics(&other, vec![found(&other, Severity::Warning)]);

        assert_eq!(editor.diagnostics_revision(), before + 2);
        assert_eq!(
            editor.diagnostic_counts(),
            [(main.clone(), 2, 1), (other.clone(), 0, 1)]
        );
    }

    #[test]
    fn underlines_follow_edits() {
        let (_dir, mut editor) = project();
        reported(&mut editor);

        press(&mut editor, "C-a");
        type_text(&mut editor, "//");

        assert_eq!(waves(&editor, 0)[1], (5..9, Severity::Warning));
    }

    #[test]
    fn files_opened_later_are_marked_too() {
        let (dir, mut editor) = project();
        fs::write(dir.path().join("src").join("other.rs"), "\nfn f() {}\n").unwrap();
        reported(&mut editor);

        editor.open_file(&dir.path().join("src").join("other.rs"));

        assert_eq!(waves(&editor, 1), [(0..2, Severity::Warning)]);
    }

    #[test]
    fn the_message_at_point_is_echoed() {
        let (_dir, mut editor) = project();
        reported(&mut editor);

        press(&mut editor, "C-a C-f");
        assert_eq!(editor.diagnostic_at_point(), Some("bad"));

        press(&mut editor, "C-f C-f C-f");
        assert_eq!(editor.diagnostic_at_point(), Some("meh"));

        press(&mut editor, "C-f C-f C-f C-f C-f");
        assert_eq!(editor.diagnostic_at_point(), None);
    }

    #[test]
    fn the_echo_area_falls_back_to_the_diagnostic_at_point() {
        let (_dir, mut editor) = project();
        reported(&mut editor);

        press(&mut editor, "C-a");
        assert_eq!(editor.message(), None);
        assert_eq!(editor.echo(), Some("bad"));

        press(&mut editor, "C-x C-q");
        assert_eq!(editor.echo(), editor.message());
    }
}
