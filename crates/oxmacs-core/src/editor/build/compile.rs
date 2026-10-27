use std::path::PathBuf;

use crate::editor::build::run::{Run, RunSpec};
use crate::editor::prompts::minibuffer::Purpose;
use crate::editor::{Editor, JobEvent, JobId, Outcome};
use crate::external::tools;

pub(in crate::editor) const NAME: &str = "*compilation*";
const DEFAULT_COMMAND: &str = "cargo build";

impl Editor {
    pub(in crate::editor) fn compilation_index(&self) -> Option<usize> {
        self.buffers.find_special::<Run>(|run| run.spec.errors)
    }

    pub(in crate::editor) fn compilation(&self) -> Option<&Run> {
        self.buffers.special(self.compilation_index()?)
    }

    pub(in crate::editor) fn compilation_mut(&mut self) -> Option<&mut Run> {
        let index = self.compilation_index()?;
        self.buffers.special_mut(index)
    }

    pub(in crate::editor) fn compile(&mut self) {
        if self.open_minibuffer("Compile command: ", Purpose::Compile) {
            let command = self
                .builds
                .last_command()
                .unwrap_or(DEFAULT_COMMAND)
                .to_owned();
            self.prompts.set_input(&command);
        }
    }

    pub(in crate::editor) fn recompile(&mut self) {
        if self.run_again() {
            return;
        }
        if let Some(spec) = self.compilation().map(|run| run.spec.clone()) {
            self.run_in_buffer(spec);
            return;
        }
        let command = self
            .builds
            .last_command()
            .unwrap_or(DEFAULT_COMMAND)
            .to_owned();
        let directory = self.project_directory();
        self.start_compile(&command, directory);
    }

    pub(in crate::editor) fn cargo(&mut self, subcommand: &str) {
        let directory = self.project_directory();
        self.start_compile(&format!("cargo {subcommand}"), directory);
    }

    pub(in crate::editor) fn project_directory(&self) -> PathBuf {
        let directory = self.default_directory();
        tools::cargo_root(&directory).unwrap_or(directory)
    }

    #[cfg(test)]
    pub fn compile_in(&mut self, command: &str, directory: PathBuf) {
        self.start_compile(command, directory);
    }

    #[cfg(test)]
    pub fn add_error_pattern(&mut self, pattern: super::errors::ErrorPattern) {
        self.builds.add_pattern(pattern);
    }

    pub(in crate::editor) fn start_compile(&mut self, command: &str, directory: PathBuf) {
        let words = tools::split_command(command);
        let Some((program, args)) = words.split_first() else {
            self.show_message("No command to run");
            return;
        };
        self.builds.compiling(command);
        self.run_in_buffer(RunSpec {
            directory,
            kind: "compile",
            errors: true,
            noun: Some("Compilation"),
            ..RunSpec::new(NAME, program, args.to_vec())
        });
    }

    pub(in crate::editor) fn kill_compilation(&mut self) {
        let stopped = self.stop_run(0)
            || self
                .compilation_index()
                .is_some_and(|index| self.stop_run(index));
        if !stopped {
            self.show_message("Nothing is running here");
        }
    }

    pub fn job_output(&mut self, job: JobId, line: &str) {
        self.notify_job(job, JobEvent::Line(line));
    }

    pub fn job_finished(&mut self, job: JobId, outcome: &Outcome) {
        self.job_ended(job, outcome);
        self.notify_job(job, JobEvent::Finished(outcome));
    }
}
#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::editor::{Editor, Effect, Job, JobId, Outcome, ProcessStatus};
    use crate::testing::{press, type_text};

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

    fn compiling(editor: &mut Editor) -> Job {
        press(editor, "M-x");
        type_text(editor, "compile");
        press(editor, "RET RET");
        spawned(editor).pop().unwrap()
    }

    fn compilation_text(editor: &Editor) -> String {
        let window = editor.tiling().windows()[1];
        editor.pane_of(window).unwrap().buffer().contents()
    }

    #[test]
    fn compile_suggests_cargo_build() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "compile");
        press(&mut editor, "RET");

        let minibuffer = editor.minibuffer().unwrap();
        assert_eq!(minibuffer.prompt(), "Compile command: ");
        assert_eq!(minibuffer.input().contents(), "cargo build");
    }

    #[test]
    fn compiling_runs_the_command_and_shows_its_buffer_beside() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src").join("main.rs"), "").unwrap();
        let mut editor = Editor::new();
        editor.open_file(&dir.path().join("src").join("main.rs"));
        let origin = editor.selected_window();

        let job = compiling(&mut editor);

        assert_eq!(job.program, "cargo");
        assert_eq!(job.args, ["build"]);
        assert_eq!(job.directory, PathBuf::from(dir.path()));
        assert_eq!(editor.selected_window(), origin);
        assert!(compilation_text(&editor).ends_with("$ cargo build\n\n"));
    }

    #[test]
    fn output_and_the_outcome_are_appended() {
        let mut editor = Editor::new();
        let job = compiling(&mut editor);

        editor.job_output(job.id, "   Compiling oxmacs");
        editor.job_finished(job.id, &Outcome::Exited(Some(101)));

        let text = compilation_text(&editor);
        assert!(text.contains("   Compiling oxmacs\n"));
        assert!(text.ends_with("\nCompilation exited abnormally with code 101\n"));
        assert_eq!(
            editor.message(),
            Some("Compilation exited abnormally with code 101")
        );
    }

    #[test]
    fn output_for_other_jobs_is_ignored() {
        let mut editor = Editor::new();
        compiling(&mut editor);

        editor.job_output(JobId(99), "stray");

        assert!(!compilation_text(&editor).contains("stray"));
    }

    #[test]
    fn the_compilation_window_follows_its_output() {
        let mut editor = Editor::new();
        let job = compiling(&mut editor);
        editor.job_output(job.id, "one");

        let window = editor.tiling().windows()[1];
        let pane = editor.pane_of(window).unwrap();
        assert_eq!(pane.point(), pane.buffer().len_chars());
    }

    #[test]
    fn g_reruns_and_c_c_c_k_kills() {
        let mut editor = Editor::new();
        let first = compiling(&mut editor);
        press(&mut editor, "C-x o");

        press(&mut editor, "g");
        let second = spawned(&mut editor).pop().unwrap();
        assert_eq!(second.args, ["build"]);
        assert_ne!(second.id, first.id);

        press(&mut editor, "C-c C-k");
        assert_eq!(editor.take_effects(), []);
        assert_eq!(editor.job_status(second.id), Some(ProcessStatus::Stopping));
        editor.job_finished(second.id, &Outcome::Exited(None));
        assert_eq!(editor.message(), Some("Compilation killed"));
    }

    #[test]
    fn rust_buffers_run_cargo_subcommands_directly() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        let mut editor = Editor::new();
        editor.open_file(&dir.path().join("main.rs"));

        for (keys, subcommand) in [
            ("C-c C-c C-k", "check"),
            ("C-c C-c C-b", "build"),
            ("C-c C-c C-t", "test"),
            ("C-c C-c C-r", "run"),
            ("C-c C-c C-l", "clippy"),
        ] {
            press(&mut editor, keys);
            let job = spawned(&mut editor).pop().unwrap();
            assert_eq!(
                (job.program.as_str(), job.args),
                ("cargo", vec![subcommand.to_owned()])
            );
            assert_eq!(job.directory, PathBuf::from(dir.path()));
        }
        assert_eq!(editor.builds.last_command(), Some("cargo clippy"));
    }

    #[test]
    fn the_last_command_is_suggested_next_time() {
        let mut editor = Editor::new();
        press(&mut editor, "M-x");
        type_text(&mut editor, "compile");
        press(&mut editor, "RET C-a C-k");
        type_text(&mut editor, "cargo test");
        press(&mut editor, "RET");

        press(&mut editor, "M-x");
        type_text(&mut editor, "compile");
        press(&mut editor, "RET");

        assert_eq!(
            editor.minibuffer().unwrap().input().contents(),
            "cargo test"
        );
    }
}
