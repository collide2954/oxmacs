use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{env, fs, io, process};

use crate::editor::buffers::Mode;
use crate::editor::languages::r_complete::Asked;
use crate::editor::languages::r_signature::Call;
use crate::editor::{ConsoleSpec, Editor, JobSpec};
use crate::external::r;
use crate::external::tools;
use crate::syntax::Language;
use crate::text::{Buffer, BufferId};

const LABEL: &str = "R";
const MIN_WIDTH: usize = 10;
const MISSING: &str = "R not found; install it from https://cran.r-project.org or set r-program";

#[derive(Debug, Default)]
struct Extras {
    helper: Option<PathBuf>,
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

#[derive(Debug, Default)]
pub(in crate::editor) struct Sessions {
    focus: Option<BufferId>,
    plots: Option<BufferId>,
    links: Vec<(BufferId, BufferId)>,
    pub(super) signature_shown: Option<Call>,
    pub(super) definition: Option<(String, (BufferId, usize))>,
    launched: u64,
    extras: Vec<(BufferId, Extras)>,
}

impl Editor {
    fn r_consoles(&self) -> Vec<BufferId> {
        self.consoles(LABEL)
    }

    pub(super) fn session_id(&self) -> Option<BufferId> {
        let consoles = self.r_consoles();
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

    fn extras(&self) -> Option<&Extras> {
        let id = self.session_id()?;
        self.r_sessions
            .extras
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, extras)| extras)
    }

    fn extras_mut(&mut self) -> Option<&mut Extras> {
        let id = self.session_id()?;
        self.r_sessions
            .extras
            .iter_mut()
            .find(|(known, _)| *known == id)
            .map(|(_, extras)| extras)
    }

    pub(super) fn r_running(&self) -> bool {
        self.session_id().is_some_and(|id| self.console_running(id))
    }

    pub(super) fn r_waiting(&self) -> bool {
        self.session_id().is_some_and(|id| self.console_waiting(id))
    }

    pub(super) fn set_r_asked(&mut self, asked: Option<Asked>) {
        if let Some(extras) = self.extras_mut() {
            extras.asked = asked;
        }
    }

    pub(super) fn cached_signature(&self, name: &str) -> Option<String> {
        self.extras()
            .and_then(|extras| extras.signatures.get(name).cloned())
    }

    pub(super) fn cache_signature(&mut self, name: &str, text: &str) {
        if let Some(extras) = self.extras_mut() {
            extras.signatures.insert(name.to_owned(), text.to_owned());
        }
    }

    pub(super) fn r_asking(&self) -> bool {
        self.extras().is_some_and(|extras| extras.asked.is_some())
    }

    pub(super) fn r_auto_mark(&self) -> Option<(BufferId, u64)> {
        self.extras().and_then(|extras| extras.auto)
    }

    pub(super) fn set_r_auto_mark(&mut self, mark: (BufferId, u64)) {
        if let Some(extras) = self.extras_mut() {
            extras.auto = Some(mark);
        }
    }

    pub(super) fn take_r_asked(&mut self) -> Option<Asked> {
        self.extras_mut().and_then(|extras| extras.asked.take())
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
        self.r_sessions.launched += 1;
        let plots = env::temp_dir().join("oxmacs").join(format!(
            "plots-{}-{}",
            process::id(),
            self.r_sessions.launched
        ));
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
        let restarted = existing.filter(|id| self.console_at(*id).is_some());
        let name = restarted
            .and_then(|id| self.buffer_by_id(id))
            .map_or_else(|| self.session_name(), |buffer| buffer.name().to_owned());
        let spec = ConsoleSpec {
            name: name.clone(),
            label: LABEL,
            job: JobSpec {
                program: path.display().to_string(),
                args: tools::r_arguments(),
                directory: self.default_directory(),
                env: variables,
                label: "R session",
                name,
                buffer: None,
                interactive: true,
            },
            on_marker: |editor, id, kind, payload| {
                editor.with_session(id, |editor| editor.r_marker(kind, payload));
            },
            on_idle: |editor, id| editor.with_session(id, Self::r_idle),
        };
        let console = self.start_console(spec, restarted);
        self.r_sessions
            .extras
            .retain(|(known, _)| *known != console);
        self.r_sessions.extras.push((
            console,
            Extras {
                helper: Some(helper),
                ..Extras::default()
            },
        ));
        Some(console)
    }

    fn r_idle(&mut self) {
        if let Some(extras) = self.extras_mut() {
            if let Some(helper) = extras.helper.take() {
                let _ = fs::remove_file(helper);
            }
            extras.signatures.clear();
        }
        self.r_sessions.signature_shown = None;
        self.r_environment_due();
        self.fit_r_width();
    }

    pub(in crate::editor) fn fit_r_widths(&mut self) {
        for console in self.r_consoles() {
            self.with_session(console, Self::fit_r_width);
        }
    }

    fn fit_r_width(&mut self) {
        let Some(id) = self.session_id() else {
            return;
        };
        let ready = self.extras().is_some_and(|extras| extras.helper.is_none());
        if !self.console_running(id) || !ready {
            return;
        }
        let known = self.extras().and_then(|extras| extras.width);
        let Some(width) = self
            .width_showing(id)
            .filter(|width| *width >= MIN_WIDTH && Some(*width) != known)
        else {
            return;
        };
        if self.r_request(&format!("getOption(\"oxmacs\")$width({width}L)"))
            && let Some(extras) = self.extras_mut()
        {
            extras.width = Some(width);
        }
    }

    pub(in crate::editor) fn r_evaluate(&mut self, code: &str) {
        if let Some(id) = self.session_id() {
            self.console_evaluate(id, code);
        }
    }

    pub(in crate::editor) fn r_request(&mut self, call: &str) -> bool {
        self.session_id()
            .is_some_and(|id| self.console_request(id, call))
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
