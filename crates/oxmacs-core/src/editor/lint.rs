use std::path::{Path, PathBuf};

use crate::config::RustLint;
use crate::diagnostics::{self, Diagnostic, Output, Severity};
use crate::editor::{Editor, Effect, Job, JobId, Outcome};
use crate::tools;

#[derive(Debug, Default)]
pub(super) struct Lints {
    due: Option<PathBuf>,
    run: Option<Run>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug)]
struct Run {
    job: JobId,
    root: PathBuf,
    tool: RustLint,
    found: Vec<Diagnostic>,
    built: bool,
    failure: Option<String>,
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

impl Editor {
    pub(super) fn request_lint(&mut self, path: &Path) {
        if subcommand(self.settings.rust_lint).is_some()
            && let Some(root) = path.parent().and_then(tools::cargo_root)
        {
            self.lints.due = Some(root);
        }
    }

    pub(super) fn start_due_lint(&mut self) {
        let Some(root) = self.lints.due.take() else {
            return;
        };
        let tool = self.settings.rust_lint;
        let Some(subcommand) = subcommand(tool) else {
            return;
        };
        if let Some(run) = self.lints.run.take() {
            self.effects.push(Effect::Kill(run.job));
        }
        let job = JobId(self.next_job);
        self.next_job += 1;
        self.effects.push(Effect::Spawn(Job {
            id: job,
            program: "cargo".to_owned(),
            args: [
                subcommand,
                "--workspace",
                "--all-targets",
                "--message-format=json",
            ]
            .map(str::to_owned)
            .to_vec(),
            directory: root.clone(),
        }));
        self.lints.run = Some(Run {
            job,
            root,
            tool,
            found: Vec::new(),
            built: false,
            failure: None,
        });
    }

    pub(super) fn lint_output(&mut self, job: JobId, line: &str) -> bool {
        let Some(run) = self.lints.run.as_mut().filter(|run| run.job == job) else {
            return false;
        };
        match diagnostics::parse(line, &run.root) {
            Output::Diagnostic(diagnostic) => {
                if !run.found.contains(&diagnostic) {
                    run.found.push(diagnostic);
                }
            }
            Output::Finished => run.built = true,
            Output::Other => {
                if run.failure.is_none() && line.starts_with("error") {
                    run.failure = Some(line.to_owned());
                }
            }
        }
        true
    }

    pub(super) fn lint_finished(&mut self, job: JobId, outcome: &Outcome) -> bool {
        let Some(run) = self.lints.run.take_if(|run| run.job == job) else {
            return false;
        };
        let name = subcommand(run.tool).unwrap_or_default();
        match outcome {
            Outcome::Exited(_) if run.built => {
                self.lints
                    .diagnostics
                    .retain(|diagnostic| !diagnostic.path.starts_with(&run.root));
                self.lints.diagnostics.extend(run.found);
                self.lints.diagnostics.sort();
            }
            Outcome::Exited(_) => {
                let reason = run.failure.unwrap_or_else(|| "failed".to_owned());
                self.show_message(format!("cargo {name}: {reason}"));
            }
            Outcome::Failed(error) => self.show_message(format!("cargo {name}: {error}")),
        }
        true
    }

    pub(super) fn lint_status(&self, path: &Path) -> Option<String> {
        if let Some(run) = self
            .lints
            .run
            .as_ref()
            .filter(|run| path.starts_with(&run.root))
        {
            return Some(running(run.tool).to_owned());
        }
        let count = |severity| {
            self.lints
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.path == path && diagnostic.severity == severity)
                .count()
        };
        let counts: Vec<String> = [
            ("E", count(Severity::Error)),
            ("W", count(Severity::Warning)),
        ]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(label, count)| format!("{label}{count}"))
        .collect();
        (!counts.is_empty()).then(|| counts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::config::Config;
    use crate::diagnostics::sample;
    use crate::editor::test_support::{press, type_text};
    use crate::editor::{Editor, Effect, Job, JobId, Outcome};

    const FINISHED: &str = r#"{"reason":"build-finished","success":false}"#;

    fn project() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src").join("main.rs"), "fn main() {}\n").unwrap();
        let mut editor = Editor::new();
        editor.open_file(&dir.path().join("src").join("main.rs"));
        (dir, editor)
    }

    fn save(editor: &mut Editor) {
        press(editor, "C-e");
        type_text(editor, " ");
        press(editor, "C-x C-s");
    }

    fn spawned(editor: &mut Editor) -> Vec<Job> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Spawn(job) => Some(job),
                _ => None,
            })
            .collect()
    }

    fn linting(editor: &mut Editor) -> JobId {
        save(editor);
        spawned(editor).pop().unwrap().id
    }

    fn report(editor: &mut Editor, job: JobId, lines: &[String]) {
        for line in lines {
            editor.job_output(job, line);
        }
        editor.job_finished(job, &Outcome::Exited(Some(101)));
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
            editor.apply_config(&Config::parse(&format!("rust-lint = \"{setting}\"")).unwrap());

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

        let effects = editor.take_effects();
        assert!(effects.contains(&Effect::Kill(first)), "{effects:?}");
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
}
