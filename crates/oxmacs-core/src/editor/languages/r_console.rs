use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{env, fs, io, process};

use crate::editor::buffers::Mode;
use crate::editor::{Editor, Effect, Job, JobId, Outcome};
use crate::external::r::{self, Piece, Stream};
use crate::external::tools::{self, Probe};
use crate::text::{Buffer, Marker};

const NAME: &str = "*R*";
const MISSING: &str = "R not found; install it from https://cran.r-project.org or set r-program";

#[derive(Debug)]
pub(in crate::editor) struct Console {
    job: Option<JobId>,
    helper: Option<PathBuf>,
    mark: Marker,
    stream: Stream,
    queued: usize,
    waiting: bool,
    history: Vec<String>,
    recall: Option<usize>,
}

static HELPERS: AtomicU64 = AtomicU64::new(0);

fn write_helper() -> io::Result<PathBuf> {
    let directory = env::temp_dir().join("oxmacs");
    fs::create_dir_all(&directory)?;
    let count = HELPERS.fetch_add(1, Ordering::Relaxed);
    let path = directory.join(format!("r-{}-{count}.R", process::id()));
    fs::write(&path, r::HELPER)?;
    Ok(path)
}

fn without_prompt(line: &str) -> &str {
    line.strip_prefix("> ")
        .or_else(|| line.strip_prefix("+ "))
        .unwrap_or(line)
}

impl Editor {
    fn console_index(&self) -> Option<usize> {
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::Console(_)))
    }

    fn console_mut(&mut self) -> Option<&mut Console> {
        let index = self.console_index()?;
        match self.buffers.mode_mut(index)? {
            Mode::Console(console) => Some(console),
            _ => None,
        }
    }

    fn console(&self) -> Option<&Console> {
        match self.buffers.mode(self.console_index()?)? {
            Mode::Console(console) => Some(console),
            _ => None,
        }
    }

    fn r_running(&self) -> bool {
        self.console().is_some_and(|console| console.job.is_some())
    }

    pub(in crate::editor) fn run_r(&mut self) {
        if !self.r_running() && !self.start_r() {
            return;
        }
        self.show_console(true);
    }

    fn show_console(&mut self, select: bool) {
        let Some(id) = self
            .console_index()
            .and_then(|index| self.buffers.get(index))
            .map(Buffer::id)
        else {
            return;
        };
        if self.window_showing(id).is_none() {
            self.beside(|editor| {
                if let Some(index) = editor.console_index() {
                    editor.select_buffer(index);
                }
            });
        }
        if select && let Some(window) = self.window_showing(id) {
            self.select_window(window);
        }
    }

    fn start_r(&mut self) -> bool {
        let Probe::Found { path, .. } = (self.probe_r)(self.config.r_program.as_deref()) else {
            self.show_message(MISSING);
            return false;
        };
        let helper = match write_helper() {
            Ok(helper) => helper,
            Err(error) => {
                self.show_message(format!("Cannot prepare R: {error}"));
                return false;
            }
        };
        let mut variables = vec![(r::PROFILE_VARIABLE.to_owned(), helper.display().to_string())];
        if let Some(original) = env::var_os(r::PROFILE_VARIABLE) {
            variables.push((
                r::ORIGINAL_PROFILE_VARIABLE.to_owned(),
                original.to_string_lossy().into_owned(),
            ));
        }
        let job = JobId(self.next_job);
        self.next_job += 1;
        self.effects.push(Effect::StartSession(Job {
            id: job,
            program: path.display().to_string(),
            args: tools::r_arguments(),
            directory: self.default_directory(),
            env: variables,
        }));
        if let Some(console) = self.console_mut() {
            console.job = Some(job);
            console.helper = Some(helper);
            console.stream = Stream::default();
            console.queued = 0;
            console.waiting = false;
            return true;
        }
        let mut buffer = Buffer::new(NAME);
        let mark = buffer.add_marker(0);
        self.beside(|editor| {
            editor.add_buffer_with(
                buffer,
                Mode::Console(Console {
                    job: Some(job),
                    helper: Some(helper),
                    mark,
                    stream: Stream::default(),
                    queued: 0,
                    waiting: false,
                    history: Vec::new(),
                    recall: None,
                }),
            );
        });
        true
    }

    fn console_job(&self, job: JobId) -> Option<usize> {
        self.console_index().filter(|_| {
            self.console()
                .is_some_and(|console| console.job == Some(job))
        })
    }

    pub fn session_output(&mut self, job: JobId, chunk: &str) {
        let Some(index) = self.console_job(job) else {
            return;
        };
        let Some(console) = self.console_mut() else {
            return;
        };
        for piece in console.stream.feed(chunk) {
            match piece {
                Piece::Text(text) => {
                    self.console_insert(index, &text);
                    if let Some(console) = self.console_mut() {
                        console.waiting = false;
                    }
                }
                Piece::Prompt(prompt) => {
                    let Some(console) = self.console_mut() else {
                        return;
                    };
                    if let Some(helper) = console.helper.take() {
                        let _ = fs::remove_file(helper);
                    }
                    if console.queued > 0 {
                        console.queued -= 1;
                    } else {
                        console.waiting = true;
                        self.console_insert(index, &format!("{prompt} "));
                    }
                }
                Piece::Marker { .. } => {}
            }
        }
    }

    pub(in crate::editor) fn console_finished(&mut self, job: JobId, outcome: &Outcome) -> bool {
        let Some(index) = self.console_job(job) else {
            return false;
        };
        let summary = match outcome {
            Outcome::Exited(_) => "Process R finished".to_owned(),
            Outcome::Failed(error) => format!("Process R failed to start: {error}"),
        };
        self.console_insert(index, &format!("\n{summary}\n"));
        if let Some(console) = self.console_mut() {
            console.job = None;
        }
        self.show_message(summary);
        true
    }

    fn console_insert(&mut self, index: usize, text: &str) {
        let Some(mark) = self.console().map(|console| console.mark) else {
            return;
        };
        let Some(buffer) = self.buffers.get_mut(index) else {
            return;
        };
        let at = buffer.marker(mark).unwrap_or_else(|| buffer.len_chars());
        buffer.insert_at(at, text);
        let after = at + text.chars().count();
        buffer.set_marker(mark, after);
        let id = buffer.id();
        self.move_windows(id, |point, _| (point == at).then_some(after));
    }

    fn console_input(&self) -> Option<(usize, String)> {
        let console = self.console()?;
        let buffer = self.buffers.current();
        let start = buffer.marker(console.mark)?;
        Some((start, buffer.text_in(start..buffer.len_chars())))
    }

    fn replace_console_input(&mut self, text: &str) {
        let Some((start, _)) = self.console_input() else {
            return;
        };
        let buffer = self.buffers.current_mut();
        buffer.delete_range(start..buffer.len_chars());
        buffer.insert_at(start, text);
        buffer.set_point(buffer.len_chars());
    }

    pub(in crate::editor) fn console_send_input(&mut self) {
        let Some((start, input)) = self.console_input() else {
            return;
        };
        let buffer = self.buffers.current();
        if buffer.point() < start {
            let line = buffer.line_text(buffer.line_of(buffer.point()));
            let old = without_prompt(&line).to_owned();
            self.replace_console_input(&old);
            return;
        }
        let end = buffer.len_chars();
        self.buffers.current_mut().insert_at(end, "\n");
        self.buffers.current_mut().set_point(end + 1);
        let Some(console) = self.console_mut() else {
            return;
        };
        let after = end + 1;
        let mark = console.mark;
        if !input.trim().is_empty() && console.history.last() != Some(&input) {
            console.history.push(input.clone());
        }
        console.recall = None;
        self.buffers.current_mut().set_marker(mark, after);
        self.send_to_r(&format!("{input}\n"));
    }

    fn send_to_r(&mut self, code: &str) {
        let Some(console) = self.console_mut() else {
            return;
        };
        let Some(job) = console.job else {
            self.show_message("R is not running; M-x run-r starts it");
            return;
        };
        let lines = code.lines().count().max(1);
        console.queued += lines - usize::from(console.waiting);
        console.waiting = false;
        self.effects.push(Effect::Send(job, code.to_owned()));
    }

    pub(in crate::editor) fn console_history(&mut self, older: bool) {
        let Some(console) = self.console_mut() else {
            return;
        };
        if console.history.is_empty() {
            self.show_message("No history");
            return;
        }
        let last = console.history.len() - 1;
        let next = match (console.recall, older) {
            (None, true) => Some(last),
            (None, false) => None,
            (Some(index), true) => Some(index.saturating_sub(1)),
            (Some(index), false) => (index < last).then_some(index + 1),
        };
        console.recall = next;
        let text = next
            .and_then(|index| console.history.get(index))
            .cloned()
            .unwrap_or_default();
        self.replace_console_input(&text);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::editor::test_support::{lines, press, type_text};
    use crate::editor::{Editor, Effect, Job, JobId, Outcome};
    use crate::external::r;
    use crate::external::tools::{self, Probe};

    const PROMPT: &str = "\u{1}>\u{2}\u{3}";

    fn found(_: Option<&str>) -> Probe {
        Probe::Found {
            path: PathBuf::from("R-bin"),
            version: "R version 4.6.1".to_owned(),
        }
    }

    fn started() -> (Editor, JobId) {
        let mut editor = Editor::new();
        editor.probe_r = found;
        press(&mut editor, "M-x");
        type_text(&mut editor, "run-r");
        press(&mut editor, "RET");
        let job = sessions(&mut editor).pop().unwrap().id;
        (editor, job)
    }

    fn sessions(editor: &mut Editor) -> Vec<Job> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::StartSession(job) => Some(job),
                _ => None,
            })
            .collect()
    }

    fn sent(editor: &mut Editor) -> Vec<String> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Send(_, text) => Some(text),
                _ => None,
            })
            .collect()
    }

    fn contents(editor: &Editor) -> String {
        editor.buffer().contents()
    }

    #[test]
    fn run_r_starts_r_with_the_helper_as_its_profile() {
        let mut editor = Editor::new();
        editor.probe_r = found;

        press(&mut editor, "M-x");
        type_text(&mut editor, "run-r");
        press(&mut editor, "RET");

        let job = sessions(&mut editor).pop().unwrap();
        assert_eq!(job.program, "R-bin");
        assert_eq!(job.args, tools::r_arguments());
        let (_, profile) = job
            .env
            .iter()
            .find(|(name, _)| name == r::PROFILE_VARIABLE)
            .unwrap();
        assert_eq!(fs::read_to_string(profile).unwrap(), r::HELPER);
        assert_eq!(editor.buffer().name(), "*R*");
        assert_eq!(editor.tiling().windows().len(), 2);
    }

    #[test]
    fn each_session_gets_its_own_helper() {
        let first = super::write_helper().unwrap();
        let second = super::write_helper().unwrap();

        assert_ne!(first, second);
        for path in [first, second] {
            assert_eq!(fs::read_to_string(path).unwrap(), r::HELPER);
        }
    }

    #[test]
    fn the_helper_is_removed_once_r_is_ready() {
        let mut editor = Editor::new();
        editor.probe_r = found;
        press(&mut editor, "M-x");
        type_text(&mut editor, "run-r");
        press(&mut editor, "RET");
        let job = sessions(&mut editor).pop().unwrap();
        let (_, profile) = job
            .env
            .iter()
            .find(|(name, _)| name == r::PROFILE_VARIABLE)
            .unwrap();

        editor.session_output(job.id, PROMPT);

        assert!(!std::path::Path::new(profile).exists());
    }

    #[test]
    fn missing_r_is_reported() {
        let mut editor = Editor::new();
        editor.probe_r = |_| Probe::Missing;

        press(&mut editor, "M-x");
        type_text(&mut editor, "run-r");
        press(&mut editor, "RET");

        assert_eq!(
            editor.message(),
            Some("R not found; install it from https://cran.r-project.org or set r-program")
        );
        assert_eq!(sessions(&mut editor), []);
    }

    #[test]
    fn typed_input_is_sent_and_answered_after_it() {
        let (mut editor, job) = started();
        editor.session_output(job, PROMPT);
        assert_eq!(contents(&editor), "> ");

        type_text(&mut editor, "1 + 1");
        press(&mut editor, "RET");
        assert_eq!(sent(&mut editor), ["1 + 1\n"]);

        editor.session_output(job, &format!("[1] 2\n{PROMPT}"));
        assert_eq!(contents(&editor), "> 1 + 1\n[1] 2\n> ");
        assert_eq!(editor.buffer().point(), editor.buffer().len_chars());
    }

    #[test]
    fn history_recalls_earlier_input() {
        let (mut editor, job) = started();
        editor.session_output(job, PROMPT);
        for input in ["a", "b"] {
            type_text(&mut editor, input);
            press(&mut editor, "RET");
            editor.session_output(job, PROMPT);
        }

        press(&mut editor, "M-p");
        assert_eq!(lines(&editor).last().unwrap(), "> b");
        press(&mut editor, "M-p");
        assert_eq!(lines(&editor).last().unwrap(), "> a");
        press(&mut editor, "M-n");
        assert_eq!(lines(&editor).last().unwrap(), "> b");
    }

    #[test]
    fn return_on_old_input_copies_it_to_the_prompt() {
        let (mut editor, job) = started();
        editor.session_output(job, PROMPT);
        type_text(&mut editor, "x");
        press(&mut editor, "RET");
        editor.session_output(job, &format!("[1] 1\n{PROMPT}"));
        editor.take_effects();

        press(&mut editor, "M-< RET");

        assert_eq!(contents(&editor), "> x\n[1] 1\n> x");
        assert_eq!(sent(&mut editor), Vec::<String>::new());
    }

    #[test]
    fn prompts_for_lines_already_shown_are_hidden() {
        let (mut editor, job) = started();
        editor.session_output(job, PROMPT);
        type_text(&mut editor, "f <- function() {");
        press(&mut editor, "C-j");
        type_text(&mut editor, "}");
        press(&mut editor, "RET");

        editor.session_output(job, &format!("\u{1}+\u{2}\u{3}{PROMPT}"));

        assert_eq!(contents(&editor), "> f <- function() {\n}\n> ");
    }

    #[test]
    fn the_end_of_r_is_reported() {
        let (mut editor, job) = started();

        editor.job_finished(job, &Outcome::Exited(Some(0)));

        assert!(contents(&editor).ends_with("Process R finished\n"));
        assert_eq!(editor.message(), Some("Process R finished"));
    }
}
