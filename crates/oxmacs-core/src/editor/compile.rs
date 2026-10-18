use std::path::PathBuf;

use crate::editor::buffers::Mode;
use crate::editor::minibuffer::Purpose;
use crate::editor::{Editor, Effect};
use crate::files::paths;
use crate::text::Buffer;
use crate::tools;

pub(super) const NAME: &str = "*compilation*";
const DEFAULT_COMMAND: &str = "cargo build";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: JobId,
    pub program: String,
    pub args: Vec<String>,
    pub directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Exited(Option<i32>),
    Failed(String),
}

#[derive(Debug)]
pub(super) struct Compilation {
    command: String,
    directory: PathBuf,
    job: Option<JobId>,
    killed: bool,
}

impl Editor {
    fn compilation_index(&self) -> Option<usize> {
        (0..self.buffers.len())
            .find(|index| matches!(self.buffers.mode(*index), Some(Mode::Compilation(_))))
    }

    fn compilation(&self) -> Option<&Compilation> {
        match self.buffers.mode(self.compilation_index()?)? {
            Mode::Compilation(compilation) => Some(compilation),
            _ => None,
        }
    }

    fn compilation_mut(&mut self) -> Option<&mut Compilation> {
        let index = self.compilation_index()?;
        match self.buffers.mode_mut(index)? {
            Mode::Compilation(compilation) => Some(compilation),
            _ => None,
        }
    }

    pub(super) fn compile(&mut self) {
        if self.open_minibuffer("Compile command: ", Purpose::Compile) {
            let command = self
                .last_compile
                .clone()
                .unwrap_or_else(|| DEFAULT_COMMAND.to_owned());
            self.set_minibuffer_input(&command);
        }
    }

    pub(super) fn recompile(&mut self) {
        let (command, directory) = match self.compilation() {
            Some(compilation) => (compilation.command.clone(), compilation.directory.clone()),
            None => (
                self.last_compile
                    .clone()
                    .unwrap_or_else(|| DEFAULT_COMMAND.to_owned()),
                self.project_directory(),
            ),
        };
        self.start_compile(command, directory);
    }

    pub(super) fn project_directory(&self) -> PathBuf {
        let directory = self.default_directory();
        tools::cargo_root(&directory).unwrap_or(directory)
    }

    pub(super) fn start_compile(&mut self, command: String, directory: PathBuf) {
        let words = tools::split_command(&command);
        let Some((program, args)) = words.split_first() else {
            self.show_message("No command to run");
            return;
        };
        if let Some(running) = self.compilation().and_then(|compilation| compilation.job) {
            self.effects.push(Effect::Kill(running));
        }
        let id = JobId(self.next_job);
        self.next_job += 1;
        self.last_compile = Some(command.clone());
        let shown = paths::abbreviate(&directory, self.home.as_deref());
        let header = format!("Directory: {shown}\n$ {command}\n\n");
        let compilation = Compilation {
            command,
            directory: directory.clone(),
            job: Some(id),
            killed: false,
        };
        match self.compilation_index() {
            Some(index) => {
                if let Some(buffer) = self.buffers.get_mut(index) {
                    buffer.set_text(&header);
                    buffer.set_point(buffer.len_chars());
                }
                self.buffers.set_mode(index, Mode::Compilation(compilation));
                let shown = self.buffers.get(index).map(Buffer::id);
                if shown.and_then(|id| self.window_showing(id)).is_none() {
                    self.beside(|editor| {
                        if let Some(index) = editor.compilation_index() {
                            editor.select_buffer(index);
                        }
                    });
                }
            }
            None => self.beside(|editor| {
                let mut buffer = Buffer::with_text(NAME, &header);
                buffer.set_read_only(true);
                buffer.set_point(buffer.len_chars());
                editor.add_buffer_with(buffer, Mode::Compilation(compilation));
            }),
        }
        self.effects.push(Effect::Spawn(Job {
            id,
            program: program.clone(),
            args: args.to_vec(),
            directory,
        }));
    }

    pub(super) fn kill_compilation(&mut self) {
        let Some(compilation) = self
            .compilation_mut()
            .filter(|compilation| compilation.job.is_some())
        else {
            self.show_message("No compilation process running");
            return;
        };
        compilation.killed = true;
        if let Some(job) = compilation.job {
            self.effects.push(Effect::Kill(job));
        }
    }

    fn compilation_for(&self, job: JobId) -> Option<usize> {
        self.compilation_index().filter(|_| {
            self.compilation()
                .is_some_and(|compilation| compilation.job == Some(job))
        })
    }

    pub fn job_output(&mut self, job: JobId, line: &str) {
        if let Some(index) = self.compilation_for(job) {
            self.append_output(index, &format!("{line}\n"));
        }
    }

    pub fn job_finished(&mut self, job: JobId, outcome: &Outcome) {
        let Some(index) = self.compilation_for(job) else {
            return;
        };
        let killed = self
            .compilation()
            .is_some_and(|compilation| compilation.killed);
        let summary = match outcome {
            _ if killed => "Compilation killed".to_owned(),
            Outcome::Exited(Some(0)) => "Compilation finished".to_owned(),
            Outcome::Exited(Some(code)) => {
                format!("Compilation exited abnormally with code {code}")
            }
            Outcome::Exited(None) => "Compilation exited abnormally".to_owned(),
            Outcome::Failed(error) => format!("Compilation failed to start: {error}"),
        };
        self.append_output(index, &format!("\n{summary}\n"));
        if let Some(compilation) = self.compilation_mut() {
            compilation.job = None;
        }
        self.show_message(summary);
    }

    fn append_output(&mut self, index: usize, text: &str) {
        let Some(buffer) = self.buffers.get_mut(index) else {
            return;
        };
        let end = buffer.len_chars();
        buffer.insert_at(end, text);
        let id = buffer.id();
        self.follow_end(id, end);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::{Job, JobId, Outcome};
    use crate::editor::test_support::{press, type_text};
    use crate::editor::{Editor, Effect};

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
        assert_eq!(editor.take_effects(), [Effect::Kill(second.id)]);
        editor.job_finished(second.id, &Outcome::Exited(None));
        assert_eq!(editor.message(), Some("Compilation killed"));
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
