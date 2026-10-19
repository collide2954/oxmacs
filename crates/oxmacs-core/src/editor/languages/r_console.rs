use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{env, fs, io, process};

use crate::editor::buffers::Mode;
use crate::editor::languages::r_complete::Asked;
use crate::editor::languages::r_signature::Call;
use crate::editor::prompts::input_line::InputLine;
use crate::editor::special::processes::Kind;
use crate::editor::{Editor, Effect, Job, JobId, Outcome};
use crate::external::r::{self, Piece, Stream};
use crate::external::tools;
use crate::syntax::Language;
use crate::text::{Buffer, BufferId};

const MIN_WIDTH: usize = 10;
const MISSING: &str = "R not found; install it from https://cran.r-project.org or set r-program";

#[derive(Debug)]
pub(in crate::editor) struct Console {
    job: Option<JobId>,
    helper: Option<PathBuf>,
    input: InputLine,
    stream: Stream,
    expected: VecDeque<Option<String>>,
    waiting: bool,
    asked: Option<Asked>,
    width: Option<usize>,
    auto: Option<(BufferId, u64)>,
    signatures: HashMap<String, String>,
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

impl Console {
    fn new(job: JobId, helper: PathBuf, input: InputLine) -> Self {
        Self {
            job: Some(job),
            helper: Some(helper),
            input,
            stream: Stream::default(),
            expected: VecDeque::new(),
            waiting: false,
            asked: None,
            width: None,
            auto: None,
            signatures: HashMap::new(),
        }
    }

    fn restart(&mut self, job: JobId, helper: PathBuf) {
        self.job = Some(job);
        self.helper = Some(helper);
        self.stream = Stream::default();
        self.expected.clear();
        self.waiting = false;
        self.asked = None;
        self.width = None;
        self.auto = None;
        self.signatures.clear();
    }
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Sessions {
    focus: Option<BufferId>,
    plots: Option<BufferId>,
    links: Vec<(BufferId, BufferId)>,
    pub(super) signature_shown: Option<Call>,
    pub(super) definition: Option<(String, (BufferId, usize))>,
}

impl Editor {
    fn consoles(&self) -> Vec<BufferId> {
        (0..self.buffers.len())
            .filter(|index| matches!(self.buffers.mode(*index), Some(Mode::Console(_))))
            .filter_map(|index| self.buffers.get(index).map(Buffer::id))
            .collect()
    }

    pub(super) fn session_id(&self) -> Option<BufferId> {
        let consoles = self.consoles();
        let known = |id: &BufferId| consoles.contains(id);
        if let Some(focus) = self.r_sessions.focus.filter(known) {
            return Some(focus);
        }
        let current = self.buffers.current().id();
        let own = match self.buffers.mode(0) {
            Some(Mode::Console(_)) => Some(current),
            Some(Mode::Plots(plots)) => plots.session(),
            Some(Mode::Environment(view)) => view.session(),
            Some(Mode::Table(view)) => view.session(),
            _ => self.linked_session(current),
        };
        own.filter(known).or_else(|| consoles.first().copied())
    }

    pub(super) fn linked_session(&self, script: BufferId) -> Option<BufferId> {
        self.r_sessions
            .links
            .iter()
            .find(|(linked, _)| *linked == script)
            .map(|(_, console)| *console)
    }

    pub(super) fn r_plots_focus(&self) -> Option<BufferId> {
        self.r_sessions.plots
    }

    pub(super) fn with_plots<T>(&mut self, id: BufferId, act: impl FnOnce(&mut Self) -> T) -> T {
        let previous = self.r_sessions.plots.replace(id);
        let result = act(self);
        self.r_sessions.plots = previous;
        result
    }

    pub(super) fn r_focused(&self) -> bool {
        self.r_sessions.focus.is_some()
    }

    pub(super) fn console_index(&self) -> Option<usize> {
        let id = self.session_id()?;
        self.buffers.position(|buffer| buffer.id() == id)
    }

    pub(super) fn with_session<T>(&mut self, id: BufferId, act: impl FnOnce(&mut Self) -> T) -> T {
        let previous = self.r_sessions.focus.replace(id);
        let result = act(self);
        self.r_sessions.focus = previous;
        result
    }

    fn console_of_job(&self, job: JobId) -> Option<BufferId> {
        (0..self.buffers.len()).find_map(|index| match self.buffers.mode(index) {
            Some(Mode::Console(console)) if console.job == Some(job) => {
                self.buffers.get(index).map(Buffer::id)
            }
            _ => None,
        })
    }

    fn link_to(&mut self, console: BufferId) {
        if self.buffers.language(0) != Some(Language::R) {
            return;
        }
        let script = self.buffers.current().id();
        self.r_sessions
            .links
            .retain(|(linked, _)| *linked != script);
        self.r_sessions.links.push((script, console));
    }

    pub(super) fn link_script(&mut self) {
        if let Some(console) = self.session_id() {
            self.link_to(console);
        }
    }

    fn session_name(&self) -> String {
        let directory = self.default_directory();
        let folder = directory.file_name().map_or_else(
            || "R".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let names = self.buffer_names();
        (1..=names.len() + 1)
            .map(|number| {
                if number == 1 {
                    format!("*R:{folder}*")
                } else {
                    format!("*R:{folder}:{number}*")
                }
            })
            .find(|name| !names.contains(&name.as_str()))
            .unwrap_or_default()
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
    pub(super) fn r_running(&self) -> bool {
        self.console().is_some_and(|console| console.job.is_some())
    }

    pub(super) fn r_waiting(&self) -> bool {
        self.console().is_some_and(|console| console.waiting)
    }

    pub(super) fn console_input_start(&self) -> Option<usize> {
        let Some(Mode::Console(console)) = self.buffers.mode(0) else {
            return None;
        };
        console.input.start(self.buffers.current())
    }

    pub(super) fn set_r_asked(&mut self, asked: Option<Asked>) {
        if let Some(console) = self.console_mut() {
            console.asked = asked;
        }
    }

    pub(super) fn cached_signature(&self, name: &str) -> Option<String> {
        self.console()
            .and_then(|console| console.signatures.get(name).cloned())
    }

    pub(super) fn cache_signature(&mut self, name: &str, text: &str) {
        if let Some(console) = self.console_mut() {
            console.signatures.insert(name.to_owned(), text.to_owned());
        }
    }

    pub(super) fn r_asking(&self) -> bool {
        self.console()
            .is_some_and(|console| console.asked.is_some())
    }

    pub(super) fn r_auto_mark(&self) -> Option<(BufferId, u64)> {
        self.console().and_then(|console| console.auto)
    }

    pub(super) fn set_r_auto_mark(&mut self, mark: (BufferId, u64)) {
        if let Some(console) = self.console_mut() {
            console.auto = Some(mark);
        }
    }

    pub(super) fn take_r_asked(&mut self) -> Option<Asked> {
        self.console_mut().and_then(|console| console.asked.take())
    }

    pub(in crate::editor) fn run_r(&mut self) {
        if !self.r_running() && !self.start_r() {
            return;
        }
        self.show_console(true);
    }

    pub(super) fn show_console(&mut self, select: bool) {
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

    pub(super) fn start_r(&mut self) -> bool {
        let existing = self.session_id();
        let Some(console) = self.launch_r(existing) else {
            return false;
        };
        self.link_to(console);
        true
    }

    pub(in crate::editor) fn r_new_session(&mut self) {
        let Some(console) = self.launch_r(None) else {
            return;
        };
        self.link_to(console);
        self.with_session(console, |editor| editor.show_console(true));
    }

    fn launch_r(&mut self, existing: Option<BufferId>) -> Option<BufferId> {
        let Some(path) = (self.find_r)(self.config.r_program.as_deref()) else {
            self.show_message(MISSING);
            return None;
        };
        let helper = match write_helper() {
            Ok(helper) => helper,
            Err(error) => {
                self.show_message(format!("Cannot prepare R: {error}"));
                return None;
            }
        };
        let job = JobId(self.next_job);
        self.next_job += 1;
        let plots =
            env::temp_dir()
                .join("oxmacs")
                .join(format!("plots-{}-{}", process::id(), job.0));
        let mut variables = vec![
            (r::PROFILE_VARIABLE.to_owned(), helper.display().to_string()),
            (r::PLOTS_VARIABLE.to_owned(), plots.display().to_string()),
        ];
        if let Some(original) = env::var_os(r::PROFILE_VARIABLE) {
            variables.push((
                r::ORIGINAL_PROFILE_VARIABLE.to_owned(),
                original.to_string_lossy().into_owned(),
            ));
        }
        let session = Job {
            id: job,
            program: path.display().to_string(),
            args: tools::r_arguments(),
            directory: self.default_directory(),
            env: variables,
        };
        if let Some(id) = existing
            && let Some(index) = self.buffers.position(|buffer| buffer.id() == id)
            && let Some(Mode::Console(console)) = self.buffers.mode_mut(index)
        {
            console.restart(job, helper);
            self.start_tracked_session(session, id);
            return Some(id);
        }
        let mut buffer = Buffer::new(self.session_name());
        let id = buffer.id();
        let input = InputLine::new(&mut buffer);
        self.beside(|editor| {
            editor.add_buffer_with(buffer, Mode::Console(Console::new(job, helper, input)));
        });
        self.start_tracked_session(session, id);
        Some(id)
    }

    fn start_tracked_session(&mut self, session: Job, console: BufferId) {
        let name = self
            .buffers
            .position(|buffer| buffer.id() == console)
            .and_then(|index| self.buffers.get(index))
            .map_or_else(|| "R".to_owned(), |buffer| buffer.name().to_owned());
        self.track_job(&session, Kind::RSession, name, Some(console));
        self.effects.push(Effect::StartSession(session));
    }

    pub fn session_output(&mut self, job: JobId, chunk: &str) {
        if let Some(console) = self.console_of_job(job) {
            self.with_session(console, |editor| editor.process_output(chunk));
        }
    }

    fn process_output(&mut self, chunk: &str) {
        let Some(console) = self.console_mut() else {
            return;
        };
        let mut idle = false;
        for piece in console.stream.feed(chunk) {
            match piece {
                Piece::Text(text) => {
                    self.console_insert(&text);
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
                    match console.expected.pop_front() {
                        Some(None) => {}
                        Some(Some(line)) => {
                            self.console_insert(&format!("{prompt} {line}\n"));
                        }
                        None => {
                            console.waiting = true;
                            idle = true;
                            self.console_insert(&format!("{prompt} "));
                        }
                    }
                }
                Piece::Marker { kind, payload } => self.r_marker(&kind, &payload),
            }
        }
        if idle {
            if let Some(console) = self.console_mut() {
                console.signatures.clear();
            }
            self.r_sessions.signature_shown = None;
            self.r_environment_due();
        }
        self.fit_r_width();
    }

    pub(in crate::editor) fn fit_r_widths(&mut self) {
        for console in self.consoles() {
            self.with_session(console, Self::fit_r_width);
        }
    }
    fn fit_r_width(&mut self) {
        let Some(console) = self.console() else {
            return;
        };
        if console.job.is_none() || console.helper.is_some() {
            return;
        }
        let known = console.width;
        let Some(width) = self
            .console_index()
            .and_then(|index| self.buffers.get(index))
            .and_then(|buffer| self.width_showing(buffer.id()))
            .filter(|width| *width >= MIN_WIDTH && Some(*width) != known)
        else {
            return;
        };
        if self.r_request(&format!("getOption(\"oxmacs\")$width({width}L)"))
            && let Some(console) = self.console_mut()
        {
            console.width = Some(width);
        }
    }

    pub(in crate::editor) fn console_finished(&mut self, job: JobId, outcome: &Outcome) -> bool {
        let Some(console) = self.console_of_job(job) else {
            return false;
        };
        self.with_session(console, |editor| editor.report_end(outcome));
        true
    }

    fn report_end(&mut self, outcome: &Outcome) {
        let summary = match outcome {
            Outcome::Exited(_) => "Process R finished".to_owned(),
            Outcome::Failed(error) => format!("Process R failed to start: {error}"),
        };
        self.console_insert(&format!("\n{summary}\n"));
        if let Some(console) = self.console_mut() {
            console.job = None;
        }
        self.show_message(summary);
    }

    fn console_insert(&mut self, text: &str) {
        let Some(mark) = self.console().map(|console| console.input.mark()) else {
            return;
        };
        let Some(buffer) = self
            .console_index()
            .and_then(|index| self.buffers.get_mut(index))
        else {
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
        let input = &self.console()?.input;
        let buffer = self.buffers.current();
        Some((input.start(buffer)?, input.text(buffer)?))
    }

    fn replace_console_input(&mut self, text: &str) {
        let Some(index) = self.console_index() else {
            return;
        };
        if let Some((buffer, Mode::Console(console))) = self.buffers.entry_mut(index) {
            console.input.replace(buffer, text);
        }
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
        let mark = console.input.mark();
        console.input.remember(&input);
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
        let unseen = lines - usize::from(console.waiting);
        console.expected.extend(std::iter::repeat_n(None, unseen));
        console.waiting = false;
        self.effects.push(Effect::Send(job, code.to_owned()));
    }

    pub(in crate::editor) fn r_evaluate(&mut self, code: &str) {
        let code = code.trim_end();
        if code.is_empty() {
            return;
        }
        let Some(console) = self.console_mut() else {
            return;
        };
        let Some(job) = console.job else {
            return;
        };
        let mut lines = code.lines();
        let first = if console.waiting { lines.next() } else { None };
        console
            .expected
            .extend(lines.map(|line| Some(line.to_owned())));
        console.waiting = false;
        if let Some(first) = first {
            self.console_insert(&format!("{first}\n"));
        }
        self.effects.push(Effect::Send(job, format!("{code}\n")));
    }

    pub(in crate::editor) fn r_request(&mut self, call: &str) -> bool {
        let Some(console) = self.console_mut() else {
            return false;
        };
        let Some(job) = console.job else {
            return false;
        };
        console.expected.push_back(None);
        self.effects.push(Effect::Send(job, format!("{call}\n")));
        true
    }

    fn r_marker(&mut self, kind: &str, payload: &str) {
        match kind {
            "message" => self.show_message(payload),
            "help" => self.show_r_help(payload),
            "complete" => self.r_completions(payload),
            "plot" => self.r_plot(payload),
            "zoom" => self.r_plot_zoomed(payload),
            "gallery" => self.r_plot_thumbnails(payload),
            "environment" => self.r_environment_arrived(payload),
            "table" => self.r_table_arrived(payload),
            "object" => self.r_object_arrived(payload),
            "signature" => self.r_signature_arrived(payload),
            "definition" => self.r_definition_arrived(payload),
            _ => {}
        }
    }

    pub(in crate::editor) fn recall_input(&mut self, older: bool) {
        let input = match self.buffers.entry_mut(0) {
            Some((buffer, Mode::Console(console))) => Some((buffer, &mut console.input)),
            Some((buffer, Mode::Chat(view))) => Some((buffer, &mut view.input)),
            _ => None,
        };
        let Some((buffer, input)) = input else {
            return;
        };
        match input.recall(older) {
            Some(text) => input.replace(buffer, &text),
            None => self.show_message("No history"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::editor::test_support::{
        R_CONSOLE, R_FOUND, R_PLOTS, lines, press, r_folder, r_script, r_sessions, type_text,
    };
    use crate::editor::{Editor, Effect, Job, JobId, Outcome};
    use crate::external::r;
    use crate::external::tools;

    const PROMPT: &str = "\u{1}>\u{2}\u{3}";
    const CONTINUE: &str = "\u{1}+\u{2}\u{3}";

    fn started() -> (Editor, JobId) {
        let mut editor = Editor::new();
        editor.find_r = R_FOUND;
        editor.set_working_directory(r_folder());
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
        editor.find_r = R_FOUND;
        editor.set_working_directory(r_folder());

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
        assert_eq!(editor.buffer().name(), R_CONSOLE);
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
        editor.find_r = R_FOUND;
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
        editor.find_r = |_| None;

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
    fn code_from_a_script_is_echoed_line_by_line_as_r_reads_it() {
        let (mut editor, job) = started();
        editor.session_output(job, PROMPT);

        editor.r_evaluate("x <- 1\nf <- function() {\n1\n}");

        assert_eq!(sent(&mut editor), ["x <- 1\nf <- function() {\n1\n}\n"]);
        assert_eq!(contents(&editor), "> x <- 1\n");
        editor.session_output(job, &format!("{PROMPT}{CONTINUE}{CONTINUE}{PROMPT}"));
        assert_eq!(
            contents(&editor),
            "> x <- 1\n> f <- function() {\n+ 1\n+ }\n> "
        );
    }

    #[test]
    fn code_sent_while_r_is_busy_waits_for_its_prompt() {
        let (mut editor, job) = started();

        editor.r_evaluate("1 + 1");
        editor.session_output(job, "R version 4.6.1\n");
        editor.session_output(job, PROMPT);
        editor.session_output(job, &format!("[1] 2\n{PROMPT}"));

        assert_eq!(contents(&editor), "R version 4.6.1\n> 1 + 1\n[1] 2\n> ");
    }

    #[test]
    fn r_learns_the_console_width_when_ready_and_when_it_changes() {
        let (mut editor, job) = started();
        editor.resize_pane(editor.selected_window(), 20, 61);
        editor.take_effects();

        editor.session_output(job, PROMPT);
        assert_eq!(sent(&mut editor), ["getOption(\"oxmacs\")$width(60L)\n"]);
        editor.session_output(job, PROMPT);
        assert_eq!(contents(&editor), "> ");

        editor.resize_pane(editor.selected_window(), 20, 71);
        assert_eq!(sent(&mut editor), ["getOption(\"oxmacs\")$width(70L)\n"]);
        editor.resize_pane(editor.selected_window(), 30, 71);
        assert_eq!(sent(&mut editor), Vec::<String>::new());
    }

    fn sends(editor: &mut Editor) -> Vec<(JobId, String)> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Send(job, text) => Some((job, text)),
                _ => None,
            })
            .collect()
    }

    fn new_session(editor: &mut Editor) -> JobId {
        press(editor, "M-x");
        type_text(editor, "r-new-session");
        press(editor, "RET");
        r_sessions(editor)[0]
    }

    fn visit(editor: &mut Editor, name: &str) {
        press(editor, "C-x b");
        type_text(editor, name);
        press(editor, "RET");
    }

    #[test]
    fn sessions_are_named_after_their_folder_and_numbered_when_they_clash() {
        let mut editor = r_script("x\n", 0);
        press(&mut editor, "C-c C-z");
        assert_eq!(editor.buffer().name(), R_CONSOLE);
        press(&mut editor, "C-c C-z");

        new_session(&mut editor);

        assert_eq!(editor.buffer().name(), "*R:analysis:2*");
    }

    #[test]
    fn each_script_sends_to_the_session_it_last_used() {
        let mut editor = r_script("a <- 1\n", 0);
        press(&mut editor, "C-RET");
        let first = r_sessions(&mut editor)[0];
        visit(&mut editor, "b");
        press(&mut editor, "M-x");
        type_text(&mut editor, "r-mode");
        press(&mut editor, "RET");
        type_text(&mut editor, "b");
        press(&mut editor, "C-RET");
        assert_eq!(sends(&mut editor), [(first, "b\n".to_owned())]);

        let second = new_session(&mut editor);
        visit(&mut editor, "b");
        press(&mut editor, "M-< C-RET");
        assert_eq!(sends(&mut editor), [(second, "b\n".to_owned())]);

        visit(&mut editor, "t");
        press(&mut editor, "M-< C-RET");
        assert_eq!(sends(&mut editor), [(first, "a <- 1\n".to_owned())]);
    }

    #[test]
    fn output_and_plots_go_to_the_session_that_made_them() {
        let mut editor = r_script("x\n", 0);
        press(&mut editor, "C-c C-z");
        let first = r_sessions(&mut editor)[0];
        press(&mut editor, "C-c C-z");
        let second = new_session(&mut editor);

        editor.session_output(first, PROMPT);
        editor.session_output(second, "two\n");
        editor.session_output(
            second,
            &format!("\u{1}plot\u{2}new\t1\t1\t1\t672\t480\tC:/p.png\u{3}{PROMPT}"),
        );

        assert_eq!(contents(&editor), "two\n> ");
        assert!(editor.buffer_names().contains(&"*R:analysis:2 plots*"));
        assert!(!editor.buffer_names().contains(&R_PLOTS));
        visit(&mut editor, R_CONSOLE);
        assert_eq!(contents(&editor), "> ");
    }

    #[test]
    fn the_end_of_r_is_reported() {
        let (mut editor, job) = started();

        editor.job_finished(job, &Outcome::Exited(Some(0)));

        assert!(contents(&editor).ends_with("Process R finished\n"));
        assert_eq!(editor.message(), Some("Process R finished"));
    }
}
