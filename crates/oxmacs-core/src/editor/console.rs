use std::collections::VecDeque;

use crate::command::ScopeId;
use crate::editor::buffers::Mode;
use crate::editor::prompts::input_line::InputLine;
use crate::editor::{Editor, JobEvent, JobId, JobSpec, Outcome};
use crate::external::console::{Piece, Stream};
use crate::text::{Buffer, BufferId};

pub type ConsoleHook = fn(&mut Editor, BufferId);
pub type MarkerHook = fn(&mut Editor, BufferId, &str, &str);

const PROMPTS: [&str; 4] = [">>> ", "... ", "> ", "+ "];

#[derive(Debug, Clone)]
pub struct ConsoleSpec {
    pub name: String,
    pub label: &'static str,
    pub job: JobSpec,
    pub on_marker: MarkerHook,
    pub on_idle: ConsoleHook,
    pub scope: Option<ScopeId>,
}

#[derive(Debug)]
pub(in crate::editor) struct Console {
    pub(in crate::editor) job: Option<JobId>,
    pub(in crate::editor) input: InputLine,
    stream: Stream,
    expected: VecDeque<Option<String>>,
    pub(in crate::editor) waiting: bool,
    pub(in crate::editor) label: &'static str,
    on_marker: MarkerHook,
    on_idle: ConsoleHook,
    pub(in crate::editor) scope: Option<ScopeId>,
}

fn without_prompt(line: &str) -> &str {
    PROMPTS
        .iter()
        .find_map(|prompt| line.strip_prefix(prompt))
        .unwrap_or(line)
}

impl Editor {
    pub fn start_console(&mut self, spec: ConsoleSpec, existing: Option<BufferId>) -> BufferId {
        let (id, buffer) = if let Some(id) = existing.filter(|id| self.console_at(*id).is_some()) {
            (id, None)
        } else {
            let buffer = Buffer::new(spec.name.clone());
            (buffer.id(), Some(buffer))
        };
        let job = JobSpec {
            buffer: Some(id),
            interactive: true,
            ..spec.job
        };
        let job = self.start_job(job, Self::console_event);
        match buffer {
            Some(mut buffer) => {
                let input = InputLine::new(&mut buffer);
                let console = Console {
                    job: Some(job),
                    input,
                    stream: Stream::default(),
                    expected: VecDeque::new(),
                    waiting: false,
                    label: spec.label,
                    on_marker: spec.on_marker,
                    on_idle: spec.on_idle,
                    scope: spec.scope,
                };
                self.beside(|editor| editor.add_buffer_with(buffer, Mode::Console(console)));
            }
            None => {
                if let Some(console) = self.console_at_mut(id) {
                    console.job = Some(job);
                    console.stream = Stream::default();
                    console.expected.clear();
                    console.waiting = false;
                }
            }
        }
        id
    }

    pub fn consoles(&self, label: &str) -> Vec<BufferId> {
        (0..self.buffers.len())
            .filter(|index| {
                matches!(self.buffers.mode(*index), Some(Mode::Console(console)) if console.label == label)
            })
            .filter_map(|index| self.buffers.get(index).map(Buffer::id))
            .collect()
    }

    pub(in crate::editor) fn console_at(&self, id: BufferId) -> Option<&Console> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        match self.buffers.mode(index)? {
            Mode::Console(console) => Some(console),
            _ => None,
        }
    }

    pub(in crate::editor) fn console_at_mut(&mut self, id: BufferId) -> Option<&mut Console> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        match self.buffers.mode_mut(index)? {
            Mode::Console(console) => Some(console),
            _ => None,
        }
    }

    pub fn console_running(&self, id: BufferId) -> bool {
        self.console_at(id)
            .is_some_and(|console| console.job.is_some())
    }

    pub fn console_waiting(&self, id: BufferId) -> bool {
        self.console_at(id).is_some_and(|console| console.waiting)
    }

    pub fn console_history(&self, id: BufferId) -> Vec<String> {
        self.console_at(id)
            .map(|console| console.input.history().to_vec())
            .unwrap_or_default()
    }

    pub fn set_console_history(&mut self, id: BufferId, history: Vec<String>) {
        if let Some(console) = self.console_at_mut(id) {
            console.input.set_history(history);
        }
    }

    fn console_of_job(&self, job: JobId) -> Option<BufferId> {
        (0..self.buffers.len()).find_map(|index| match self.buffers.mode(index) {
            Some(Mode::Console(console)) if console.job == Some(job) => {
                self.buffers.get(index).map(Buffer::id)
            }
            _ => None,
        })
    }

    fn console_event(&mut self, job: JobId, event: JobEvent<'_>) {
        let Some(id) = self.console_of_job(job) else {
            return;
        };
        match event {
            JobEvent::Line(chunk) | JobEvent::Output(chunk) => self.console_output(id, chunk),
            JobEvent::Finished(outcome) => self.console_ended(id, outcome),
        }
    }

    fn console_output(&mut self, id: BufferId, chunk: &str) {
        let Some(console) = self.console_at_mut(id) else {
            return;
        };
        let (on_marker, on_idle) = (console.on_marker, console.on_idle);
        let mut idle = false;
        for piece in console.stream.feed(chunk) {
            match piece {
                Piece::Text(text) => {
                    self.console_insert(id, &text);
                    if let Some(console) = self.console_at_mut(id) {
                        console.waiting = false;
                    }
                }
                Piece::Prompt(shown) => {
                    let Some(console) = self.console_at_mut(id) else {
                        return;
                    };
                    match console.expected.pop_front() {
                        Some(None) => {}
                        Some(Some(line)) => self.console_insert(id, &format!("{shown}{line}\n")),
                        None => {
                            console.waiting = true;
                            idle = true;
                            self.console_insert(id, &shown);
                        }
                    }
                }
                Piece::Marker { kind, payload } => on_marker(self, id, &kind, &payload),
            }
        }
        if idle {
            on_idle(self, id);
        }
    }

    fn console_ended(&mut self, id: BufferId, outcome: &Outcome) {
        let label = self.console_at(id).map_or("", |console| console.label);
        let summary = match outcome {
            Outcome::Exited(_) => format!("Process {label} finished"),
            Outcome::Failed(error) => format!("Process {label} failed to start: {error}"),
        };
        self.console_insert(id, &format!("\n{summary}\n"));
        if let Some(console) = self.console_at_mut(id) {
            console.job = None;
        }
        self.show_message(summary);
    }

    pub fn console_insert(&mut self, id: BufferId, text: &str) {
        let Some(mark) = self.console_at(id).map(|console| console.input.mark()) else {
            return;
        };
        let Some(buffer) = self
            .buffers
            .position(|buffer| buffer.id() == id)
            .and_then(|index| self.buffers.get_mut(index))
        else {
            return;
        };
        let at = buffer.marker(mark).unwrap_or_else(|| buffer.len_chars());
        buffer.insert_at(at, text);
        let after = at + text.chars().count();
        buffer.set_marker(mark, after);
        self.move_windows(id, |point, _| (point == at).then_some(after));
    }

    fn console_send(&mut self, id: BufferId, code: &str) {
        let Some(console) = self.console_at_mut(id) else {
            return;
        };
        let Some(job) = console.job else {
            let label = console.label;
            self.show_message(format!("{label} is not running"));
            return;
        };
        let lines = code.lines().count().max(1);
        let unseen = lines - usize::from(console.waiting);
        console.expected.extend(std::iter::repeat_n(None, unseen));
        console.waiting = false;
        self.send_job(job, code);
    }

    pub fn console_evaluate(&mut self, id: BufferId, code: &str) {
        let code = code.trim_end();
        if code.is_empty() {
            return;
        }
        let Some(console) = self.console_at_mut(id) else {
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
            self.console_insert(id, &format!("{first}\n"));
        }
        self.send_job(job, format!("{code}\n"));
    }

    pub fn console_request(&mut self, id: BufferId, call: &str) -> bool {
        let Some(console) = self.console_at_mut(id) else {
            return false;
        };
        let Some(job) = console.job else {
            return false;
        };
        console.expected.push_back(None);
        self.send_job(job, format!("{call}\n"));
        true
    }

    pub fn console_input_start(&self) -> Option<usize> {
        let Some(Mode::Console(console)) = self.buffers.mode(0) else {
            return None;
        };
        console.input.start(self.buffers.current())
    }

    pub(in crate::editor) fn console_send_input(&mut self) {
        let Some(Mode::Console(console)) = self.buffers.mode(0) else {
            return;
        };
        let buffer = self.buffers.current();
        let (Some(start), Some(input)) = (console.input.start(buffer), console.input.text(buffer))
        else {
            return;
        };
        let id = buffer.id();
        if buffer.point() < start {
            let line = buffer.line_text(buffer.line_of(buffer.point()));
            let old = without_prompt(&line).to_owned();
            if let Some((buffer, Mode::Console(console))) = self.buffers.entry_mut(0) {
                console.input.replace(buffer, &old);
            }
            return;
        }
        let end = buffer.len_chars();
        self.buffers.current_mut().insert_at(end, "\n");
        self.buffers.current_mut().set_point(end + 1);
        if let Some((buffer, Mode::Console(console))) = self.buffers.entry_mut(0) {
            console.input.remember(&input);
            buffer.set_marker(console.input.mark(), end + 1);
        }
        self.console_send(id, &format!("{input}\n"));
    }

    fn input_start_here(&mut self) -> Option<usize> {
        let (buffer, mode) = self.buffers.entry_mut(0)?;
        let start = match mode {
            Mode::Console(console) => console.input.start(buffer),
            Mode::Plugin(view) => view.input_line().and_then(|input| input.start(buffer)),
            _ => None,
        }?;
        let point = buffer.point();
        (point > start && buffer.line_of(point) == buffer.line_of(start)).then_some(start)
    }

    pub(in crate::editor) fn beginning_of_line_or_input(&mut self) {
        match self.input_start_here() {
            Some(start) => self.move_point(move |_, _| start),
            None => self.move_point(crate::text::motion::beginning_of_line),
        }
    }

    pub(in crate::editor) fn recall_input(&mut self, older: bool) {
        let input = match self.buffers.entry_mut(0) {
            Some((buffer, Mode::Console(console))) => Some((buffer, &mut console.input)),
            Some((buffer, Mode::Plugin(view))) => view.input_line().map(|input| (buffer, input)),
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
    use std::path::PathBuf;

    use super::*;
    use crate::command::Command;
    use crate::editor::Effect;
    use crate::editor::test_support::{press, type_text};

    #[derive(Debug, Default)]
    struct Heard(Vec<String>);

    fn heard(editor: &mut Editor, id: BufferId, kind: &str, payload: &str) {
        let name = editor
            .buffer_by_id(id)
            .map(|buffer| buffer.name().to_owned())
            .unwrap_or_default();
        if let Some(state) = editor.plugin_state_mut::<Heard>() {
            state.0.push(format!("{name} {kind} {payload}"));
        }
    }

    fn idle(editor: &mut Editor, _id: BufferId) {
        if let Some(state) = editor.plugin_state_mut::<Heard>() {
            state.0.push("idle".to_owned());
        }
    }

    fn spec() -> ConsoleSpec {
        ConsoleSpec {
            name: "*Shell*".to_owned(),
            label: "Shell",
            job: JobSpec {
                program: "shell".to_owned(),
                args: vec!["-i".to_owned()],
                directory: PathBuf::from("."),
                env: Vec::new(),
                label: "shell",
                name: "*Shell*".to_owned(),
                buffer: None,
                interactive: false,
            },
            on_marker: heard,
            on_idle: idle,
            scope: None,
        }
    }

    const PROMPT: &str = "\u{1}>\u{2}$ \u{3}";

    fn started() -> (Editor, BufferId, JobId) {
        start(Editor::new(), spec())
    }

    fn start(mut editor: Editor, spec: ConsoleSpec) -> (Editor, BufferId, JobId) {
        editor.set_plugin_state(Heard::default());
        let id = editor.start_console(spec, None);
        let job = editor
            .take_effects()
            .into_iter()
            .find_map(|effect| match effect {
                Effect::StartSession(job) => Some(job.id),
                _ => None,
            })
            .unwrap();
        let window = editor
            .tiling()
            .windows()
            .into_iter()
            .find(|window| {
                editor
                    .pane_of(*window)
                    .is_some_and(|pane| pane.buffer().id() == id)
            })
            .unwrap();
        editor.select_window(window);
        (editor, id, job)
    }

    fn contents(editor: &Editor, id: BufferId) -> String {
        editor
            .buffer_by_id(id)
            .map(Buffer::contents)
            .unwrap_or_default()
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

    fn heard_so_far(editor: &Editor) -> Vec<String> {
        editor
            .plugin_state::<Heard>()
            .map(|state| state.0.clone())
            .unwrap_or_default()
    }

    #[test]
    fn consoles_open_beside_and_show_output_and_prompts() {
        let (mut editor, id, job) = started();

        editor.session_output(job, &format!("hello\n{PROMPT}"));

        assert_eq!(editor.buffer().id(), id);
        assert_eq!(editor.tiling().windows().len(), 2);
        assert_eq!(contents(&editor, id), "hello\n$ ");
        assert!(editor.console_waiting(id));
        assert_eq!(editor.consoles("Shell"), [id]);
        assert_eq!(heard_so_far(&editor), ["idle"]);
    }

    #[test]
    fn typed_input_is_sent_and_remembered() {
        let (mut editor, id, job) = started();
        editor.session_output(job, PROMPT);

        type_text(&mut editor, "ls");
        press(&mut editor, "RET");

        assert_eq!(sent(&mut editor), ["ls\n"]);
        assert_eq!(contents(&editor, id), "$ ls\n");
        press(&mut editor, "M-p");
        assert!(contents(&editor, id).ends_with("ls"));
    }

    #[test]
    fn the_start_of_the_input_line_is_after_the_prompt() {
        let (mut editor, id, job) = started();
        editor.session_output(job, &format!("old output\n{PROMPT}"));

        type_text(&mut editor, "ls");
        press(&mut editor, "C-a x");

        assert_eq!(contents(&editor, id), "old output\n$ xls");
        press(&mut editor, "C-p C-e C-a");
        assert_eq!(editor.buffer().point(), 0);
    }

    #[test]
    fn evaluated_code_is_echoed_after_each_prompt() {
        let (mut editor, id, job) = started();
        editor.session_output(job, PROMPT);

        editor.console_evaluate(id, "a\nb\n");
        editor.session_output(job, &format!("{PROMPT}{PROMPT}"));

        assert_eq!(sent(&mut editor), ["a\nb\n"]);
        assert_eq!(contents(&editor, id), "$ a\n$ b\n$ ");
    }

    #[test]
    fn requests_and_markers_stay_out_of_the_transcript() {
        let (mut editor, id, job) = started();
        editor.session_output(job, PROMPT);

        assert!(editor.console_request(id, "complete()"));
        editor.session_output(job, &format!("\u{1}complete\u{2}ls\tlsof\u{3}{PROMPT}"));

        assert_eq!(sent(&mut editor), ["complete()\n"]);
        assert_eq!(contents(&editor, id), "$ ");
        assert_eq!(heard_so_far(&editor), ["idle", "*Shell* complete ls\tlsof"]);
    }

    #[test]
    fn the_end_of_the_process_is_reported() {
        let (mut editor, id, job) = started();

        editor.job_finished(job, &Outcome::Exited(Some(0)));

        assert!(!editor.console_running(id));
        assert!(contents(&editor, id).ends_with("Process Shell finished\n"));
        assert_eq!(editor.message(), Some("Process Shell finished"));
        type_text(&mut editor, "x");
        press(&mut editor, "RET");
        assert_eq!(editor.message(), Some("Shell is not running"));
    }

    #[test]
    fn consoles_with_keys_of_their_own_leave_out_the_r_keys() {
        let mut editor = Editor::new();
        let scope = editor.add_scope("Shell console");
        editor.bind(scope, "RET", Command::ComintSendInput).unwrap();
        let (mut editor, _, job) = start(
            editor,
            ConsoleSpec {
                scope: Some(scope),
                ..spec()
            },
        );
        editor.session_output(job, PROMPT);

        type_text(&mut editor, "ls");
        press(&mut editor, "RET C-c C-v");

        assert_eq!(sent(&mut editor), ["ls\n"]);
        assert_eq!(editor.message(), Some("C-c C-v is undefined"));
    }

    #[test]
    fn history_can_be_read_and_given_back() {
        let (mut editor, id, job) = started();
        editor.session_output(job, PROMPT);
        type_text(&mut editor, "ls");
        press(&mut editor, "RET");
        assert_eq!(editor.console_history(id), ["ls"]);

        editor.set_console_history(id, vec!["pwd".to_owned(), "cd".to_owned()]);
        editor.session_output(job, PROMPT);

        press(&mut editor, "M-p");
        assert!(contents(&editor, id).ends_with("$ cd"));
        assert_eq!(editor.console_history(id), ["pwd", "cd"]);
    }

    #[test]
    fn restarting_keeps_the_buffer() {
        let (mut editor, id, job) = started();
        editor.job_finished(job, &Outcome::Exited(Some(0)));

        let again = editor.start_console(spec(), Some(id));

        assert_eq!(again, id);
        assert!(editor.console_running(id));
        assert_eq!(editor.consoles("Shell"), [id]);
    }
}
