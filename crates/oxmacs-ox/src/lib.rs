mod buffers;
mod bundled;
pub mod checker;
mod commands;
mod consoles;
mod editing;
mod handles;
mod help;
mod hooks;
pub mod idle;
mod keepass;
mod layout;
mod log;
mod manual;
mod mode;
mod modes;
mod natives;
mod plugins;
mod processes;
mod prompts;
mod servers;
mod settings;
pub mod shapes;
mod stacktrace;
pub mod takeover;
#[cfg(test)]
mod testing;
mod tools;
mod transients;
pub mod views;
mod xref;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use ox::task::Wake;
use ox::{Error, Isolate};
use oxmacs_core::config::Config;
use oxmacs_core::editor::Editor;
use oxmacs_core::text::BufferId;
pub use plugins::Bundle;

const BUDGET: Duration = Duration::from_millis(4);
const QUICK: Duration = Duration::from_millis(250);
const PATIENT: Duration = Duration::from_secs(10);
const PRELUDE: &str = include_str!("prelude.ox");
const DOCS: &str = include_str!("docs.ox");

pub fn home() -> Option<PathBuf> {
    std::env::var_os("OXMACS_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".oxmacs")))
}

fn editor_isolate(answers: &prompts::Inbox) -> Isolate {
    let mut isolate = Isolate::new();
    natives::install(&mut isolate);
    settings::install(&mut isolate);
    commands::install(&mut isolate);
    plugins::install(&mut isolate);
    views::install(&mut isolate);
    buffers::install(&mut isolate);
    handles::install(&mut isolate);
    editing::install(&mut isolate);
    hooks::install(&mut isolate);
    processes::install(&mut isolate);
    consoles::install(&mut isolate);
    layout::install(&mut isolate);
    servers::install(&mut isolate);
    transients::install(&mut isolate);
    modes::install(&mut isolate);
    takeover::install(&mut isolate);
    idle::install(&mut isolate);
    prompts::install(&mut isolate, answers);
    keepass::install(&mut isolate);
    manual::install(&mut isolate);
    isolate.add_native_docs(DOCS);
    if let Err(error) = isolate.extend_prelude("oxmacs/prelude.ox", PRELUDE) {
        panic!("The oxmacs prelude failed to load: {error}");
    }
    isolate
}

pub fn checking_isolate() -> Isolate {
    editor_isolate(&prompts::Inbox::default())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    Idle,
    Waiting,
    Running,
}

#[derive(Debug)]
pub struct Ox {
    isolate: Isolate,
    bundled: &'static [Bundle],
    home: Option<PathBuf>,
    answers: prompts::Inbox,
    evaluation: tools::Evaluation,
    checker: checker::Checker,
    help: help::Help,
}

impl Default for Ox {
    fn default() -> Self {
        Self::new()
    }
}

impl Ox {
    pub fn new() -> Self {
        Self::with_bundled(bundled::BUNDLED)
    }

    pub fn with_bundled(bundled: &'static [Bundle]) -> Self {
        let answers = prompts::Inbox::default();
        Self {
            isolate: editor_isolate(&answers),
            bundled,
            home: None,
            answers,
            evaluation: tools::Evaluation::default(),
            checker: checker::Checker::default(),
            help: help::Help::default(),
        }
    }

    pub fn start(&mut self, editor: &mut Editor, home: Option<&Path>) {
        self.home = home.map(Path::to_path_buf);
        editor.let_host_play_macros();
        editor.set_init_path(home.map(|home| home.join("init.ox")));
        tools::install(editor);
        hooks::watch(editor);
        plugins::discover(&mut self.isolate, editor, self.bundled, home);
        self.run_init(editor, home);
        plugins::wake_at_startup(&self.isolate, editor);
        views::refresh(&mut self.isolate, editor);
    }

    pub fn frame(&mut self, editor: &mut Editor, now: Instant, size: (f32, f32), font_size: f32) {
        let started = self.isolate.profiling().then(Instant::now);
        takeover::frame(&mut self.isolate, editor, now, size, font_size);
        self.timed("frame", started);
    }

    fn timed(&mut self, kind: &str, started: Option<Instant>) {
        if let Some(started) = started {
            self.isolate.record_tick(kind, started.elapsed());
        }
    }

    pub fn idle(&mut self, editor: &mut Editor, since: Instant, now: Instant) {
        idle::run(&mut self.isolate, editor, since, now);
        self.checker
            .run(editor, now.saturating_duration_since(since));
    }

    pub fn idle_wait(&self, editor: &Editor, since: Instant) -> Option<Duration> {
        let check = self.checker.wait(editor);
        idle::next(editor, since).into_iter().chain(check).min()
    }

    pub fn end_take_over(&mut self, editor: &mut Editor) {
        takeover::finish(&mut self.isolate, editor);
    }

    pub fn press(&mut self, editor: &mut Editor, buffer: BufferId, slot: usize) {
        views::press(&mut self.isolate, editor, buffer, slot);
    }

    pub fn view_sized(&mut self, editor: &mut Editor, buffer: BufferId, width: u32, height: u32) {
        views::sized(&mut self.isolate, editor, buffer, (width, height));
    }

    fn run_init(&mut self, editor: &mut Editor, home: Option<&Path>) -> bool {
        let Some(home) = home else {
            return false;
        };
        if editor.config() != &Config::default() {
            editor.reconfigure(&Config::default());
        }
        plugins::reset_configs(self.isolate.heap_mut(), editor);
        let custom = self.run_file(editor, &home.join("custom.ox"));
        settings::mark_custom(editor);
        plugins::mark_custom(self.isolate.heap_mut(), editor);
        let init = self.run_file(editor, &home.join("init.ox"));
        settings::mark_saved(editor);
        plugins::mark_saved(self.isolate.heap_mut(), editor);
        hooks::settings_changed(editor);
        custom && init
    }

    fn run_file(&mut self, editor: &mut Editor, path: &Path) -> bool {
        let Ok(source) = fs::read_to_string(path) else {
            return true;
        };
        let name = path.display().to_string();
        if let Err(error) = self.isolate.within(PATIENT, |isolate| {
            isolate.eval_named_with(editor, &name, &source)
        }) {
            log::report(editor, &error, Some((&name, &source)));
            log::show(editor);
            return false;
        }
        true
    }

    pub fn eval(&mut self, editor: &mut Editor, source: &str) -> Result<String, Error> {
        let value = self.isolate.eval_with(editor, source)?;
        Ok(self.isolate.write(value))
    }

    pub fn pace(&self) -> Pace {
        if self.isolate.runnable() > 0 {
            Pace::Running
        } else if self.isolate.pending() > 0 || self.checker.busy() {
            Pace::Waiting
        } else {
            Pace::Idle
        }
    }

    pub fn next_wake(&self) -> Wake {
        if self.checker.busy() {
            return Wake::Soon;
        }
        self.isolate.next_wake()
    }

    pub fn tick(&mut self, editor: &mut Editor) -> Pace {
        let started = self.isolate.profiling().then(Instant::now);
        self.checker.collect(editor);
        self.drain(editor);
        while editor.macro_playing() {
            editor.play_macro_key();
            if self.drain(editor) {
                editor.stop_macro();
            }
        }
        buffers::watch(&mut self.isolate, editor);
        let highlighting = self.isolate.profiling().then(Instant::now);
        modes::highlight(&mut self.isolate, editor);
        self.timed("highlight", highlighting);
        let refreshing = self.isolate.profiling().then(Instant::now);
        views::refresh(&mut self.isolate, editor);
        self.timed("views", refreshing);
        help::auto_complete(&self.isolate, editor, self.home.as_deref(), &mut self.help);
        help::eldoc(&self.isolate, editor, self.home.as_deref(), &mut self.help);
        self.timed("tick", started);
        if tools::requests_waiting(editor) {
            return Pace::Running;
        }
        self.pace()
    }

    fn drain(&mut self, editor: &mut Editor) -> bool {
        let mut failed = false;
        hooks::release_saves(&mut self.isolate, editor);
        self.handle_requests(editor);
        for plugin in modes::wakes(editor)
            .into_iter()
            .chain(plugins::woken_by_modes(editor))
        {
            plugins::request_wake(editor, &plugin);
        }
        plugins::wake_waiting(&mut self.isolate, editor);
        hooks::run(&mut self.isolate, editor);
        consoles::run(&mut self.isolate, editor);
        buffers::watch(&mut self.isolate, editor);
        commands::run_waiting(&mut self.isolate, editor);
        transients::run_waiting(&mut self.isolate, editor);
        takeover::run_endings(&mut self.isolate, editor);
        prompts::deliver(&mut self.isolate, &self.answers);
        manual::deliver(&mut self.isolate, editor);
        let deadline = Instant::now() + BUDGET;
        while self.isolate.pending() > 0 {
            let before = self.evaluating();
            let report = self
                .isolate
                .run_with(editor, deadline.saturating_duration_since(Instant::now()));
            let evaluated = self.evaluated(editor, report.finished);
            for error in report.unhandled.into_iter().chain(evaluated) {
                failed = true;
                log::report(editor, &error, None);
            }
            self.advance(editor);
            let after = self.evaluating();
            if after.is_none() || after == before || Instant::now() >= deadline {
                break;
            }
        }
        failed
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use super::{Ox, Pace, log};
    use crate::testing::{home_with, press, settle};

    #[test]
    fn ox_code_shows_messages_in_the_echo_area() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        let shown = ox
            .eval(&mut editor, r#"(message "Hello from {"Ox"}") 42"#)
            .unwrap();

        assert_eq!(shown, "42");
        assert_eq!(editor.message(), Some("Hello from Ox"));
    }

    #[test]
    fn everything_the_editor_adds_to_ox_is_documented() {
        let problems = super::checking_isolate().undocumented();

        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn errors_come_back_with_their_message() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        let error = ox.eval(&mut editor, "(/ 1 0)").unwrap_err();

        assert_eq!(error.message, "Division by zero");
    }

    #[test]
    fn spawned_tasks_run_when_the_editor_ticks() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        assert_eq!(ox.pace(), Pace::Idle);

        ox.eval(&mut editor, r#"(spawn (message "later"))"#)
            .unwrap();
        assert_eq!(ox.pace(), Pace::Running);
        assert_eq!(editor.message(), None);

        assert_eq!(ox.tick(&mut editor), Pace::Idle);
        assert_eq!(editor.message(), Some("later"));
    }

    #[test]
    fn tasks_waiting_on_promises_ask_for_slow_ticks() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.eval(
            &mut editor,
            "(def gate (promise)) (spawn (message (await gate)))",
        )
        .unwrap();

        assert_eq!(ox.tick(&mut editor), Pace::Waiting);
        ox.eval(&mut editor, r#"(deliver gate "opened")"#).unwrap();
        assert_eq!(ox.tick(&mut editor), Pace::Idle);
        assert_eq!(editor.message(), Some("opened"));
    }

    fn run(editor: &mut Editor, ox: &mut Ox, command: &str) {
        let id = editor.command_named(command).unwrap();
        editor.execute(id);
        settle(editor, ox);
    }

    #[test]
    fn commands_defined_in_ox_run_from_the_editor() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        let defined = ox
            .eval(
                &mut editor,
                r#"(defcommand greet "Say hello" (message "Hello"))"#,
            )
            .unwrap();
        run(&mut editor, &mut ox, "greet");

        assert_eq!(defined, "greet");
        assert_eq!(editor.message(), Some("Hello"));
    }

    #[test]
    fn redefining_a_command_replaces_what_it_does() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.eval(
            &mut editor,
            r#"(defcommand greet "Say hello" (message "Hello"))"#,
        )
        .unwrap();
        ox.eval(&mut editor, r#"(defcommand greet "Say hi" (message "Hi"))"#)
            .unwrap();
        run(&mut editor, &mut ox, "greet");

        assert_eq!(editor.message(), Some("Hi"));
    }

    #[test]
    fn errors_in_commands_reach_the_echo_area() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.eval(&mut editor, r#"(defcommand broken "Fail" (/ 1 0))"#)
            .unwrap();
        run(&mut editor, &mut ox, "broken");

        assert_eq!(editor.message(), Some("Ox: Division by zero"));
    }

    #[test]
    fn keys_bound_in_ox_run_commands() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.eval(
            &mut editor,
            r#"(defcommand greet "Say hello" (message "Hello")) (bind-key "C-c g" 'greet)"#,
        )
        .unwrap();
        press(&mut editor, &mut ox, "C-c g");

        assert_eq!(editor.message(), Some("Hello"));
    }

    #[test]
    fn binding_keys_reports_mistakes() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        let unknown = ox
            .eval(&mut editor, r#"(bind-key "C-c g" 'nothing)"#)
            .unwrap_err();
        assert_eq!(unknown.message, "There is no command named nothing");
        ox.eval(
            &mut editor,
            r#"(defcommand greet "Say hello" (message "Hello"))"#,
        )
        .unwrap();
        let malformed = ox
            .eval(&mut editor, r#"(bind-key "C-c NOPE" 'greet)"#)
            .unwrap_err();
        assert!(
            malformed.message.starts_with("C-c NOPE"),
            "{}",
            malformed.message
        );
    }

    #[test]
    fn init_ox_runs_when_the_editor_starts() {
        let home = home_with(&[(
            "init.ox",
            r#"(defcommand greet "Say hello" (message "Hello")) (bind-key "C-c g" 'greet)"#,
        )]);
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.start(&mut editor, Some(home.path()));
        press(&mut editor, &mut ox, "C-c g");

        assert_eq!(editor.message(), Some("Hello"));
        assert_eq!(editor.buffer_named(log::LOG), None);
    }

    #[test]
    fn custom_ox_runs_before_init_ox_and_init_ox_wins() {
        let home = home_with(&[
            ("custom.ox", "(settings {tab-width: 2 clock: false})"),
            ("init.ox", "(settings {tab-width: 4})"),
        ]);
        let mut editor = Editor::new();

        Ox::new().start(&mut editor, Some(home.path()));

        assert_eq!(editor.config().tab_width, 4);
        assert!(!editor.config().clock);
        assert_eq!(
            editor.init_path(),
            Some(home.path().join("init.ox").as_path())
        );
    }

    #[test]
    fn a_failing_custom_ox_is_logged_and_init_ox_still_runs() {
        let home = home_with(&[
            ("custom.ox", "(settings {tab-width: 0})"),
            ("init.ox", "(settings {clock: false})"),
        ]);
        let mut editor = Editor::new();

        Ox::new().start(&mut editor, Some(home.path()));

        assert!(!editor.config().clock);
        let log = editor.buffer_named(log::LOG).unwrap();
        let text = editor.buffer_by_id(log).unwrap().contents();
        assert!(
            text.contains("custom.ox:1:1: tab-width must be at least 1"),
            "{text}"
        );
    }

    #[test]
    fn a_failing_init_ox_keeps_what_ran_and_shows_the_log() {
        let home = home_with(&[(
            "init.ox",
            "(defcommand greet \"Say hello\" (message \"Hello\"))\n(/ 1 0)\n(defcommand later \"Never\" (message \"no\"))",
        )]);
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.start(&mut editor, Some(home.path()));

        assert!(editor.command_named("greet").is_some());
        assert!(editor.command_named("later").is_none());
        let log = editor.buffer_named(log::LOG).unwrap();
        assert!(editor.buffer_shown(log));
        let text = editor.buffer_by_id(log).unwrap().contents();
        assert!(text.contains("init.ox:2:1: Division by zero"), "{text}");
    }

    #[test]
    fn starting_without_a_home_or_init_ox_is_quiet() {
        let home = home_with(&[]);
        let mut editor = Editor::new();

        Ox::new().start(&mut editor, Some(home.path()));
        Ox::new().start(&mut editor, None);

        assert_eq!(editor.buffer_named(log::LOG), None);
        assert_eq!(editor.message(), None);
    }

    #[test]
    fn failures_nothing_waits_for_reach_the_echo_area() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ox.eval(&mut editor, r#"(spawn (raise (error "lost")))"#)
            .unwrap();
        settle(&mut editor, &mut ox);

        assert_eq!(editor.message(), Some("Ox: lost"));
    }
}
