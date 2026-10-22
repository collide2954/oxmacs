mod buffers;
mod bundled;
mod commands;
mod hooks;
pub mod idle;
mod log;
mod mode;
mod modes;
mod natives;
mod plugins;
mod prompts;
pub mod shapes;
pub mod takeover;
#[cfg(test)]
mod testing;
mod tools;
mod transients;
pub mod views;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ox::{Error, Isolate};
use oxmacs_core::editor::Editor;
use oxmacs_core::text::BufferId;
pub use plugins::Bundle;

const BUDGET: Duration = Duration::from_millis(4);
const PRELUDE: &str = include_str!("prelude.ox");

pub fn home() -> Option<PathBuf> {
    std::env::var_os("OXMACS_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".oxmacs")))
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
        let mut isolate = Isolate::new();
        natives::install(&mut isolate);
        commands::install(&mut isolate);
        plugins::install(&mut isolate);
        views::install(&mut isolate);
        buffers::install(&mut isolate);
        hooks::install(&mut isolate);
        transients::install(&mut isolate);
        modes::install(&mut isolate);
        takeover::install(&mut isolate);
        idle::install(&mut isolate);
        let answers = prompts::Inbox::default();
        prompts::install(&mut isolate, &answers);
        if let Err(error) = isolate.extend_prelude("oxmacs/prelude.ox", PRELUDE) {
            panic!("The oxmacs prelude failed to load: {error}");
        }
        Self {
            isolate,
            bundled,
            home: None,
            answers,
        }
    }

    pub fn start(&mut self, editor: &mut Editor, home: Option<&Path>) {
        self.home = home.map(Path::to_path_buf);
        tools::install(editor);
        hooks::watch(editor);
        plugins::discover(&mut self.isolate, editor, self.bundled, home);
        self.run_init(editor, home);
        plugins::wake_at_startup(&self.isolate, editor);
        plugins::wake_waiting(&mut self.isolate, editor);
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
    }

    pub fn end_take_over(&mut self, editor: &mut Editor) {
        takeover::finish(&mut self.isolate, editor);
    }

    pub fn press(&mut self, editor: &mut Editor, buffer: BufferId, slot: usize) {
        views::press(&mut self.isolate, editor, buffer, slot);
    }

    fn run_init(&mut self, editor: &mut Editor, home: Option<&Path>) -> bool {
        let Some(path) = home.map(|home| home.join("init.ox")) else {
            return false;
        };
        let Ok(source) = fs::read_to_string(&path) else {
            return false;
        };
        let name = path.display().to_string();
        if let Err(error) = self.isolate.eval_named_with(editor, &name, &source) {
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
        } else if self.isolate.pending() > 0 {
            Pace::Waiting
        } else {
            Pace::Idle
        }
    }

    pub fn tick(&mut self, editor: &mut Editor) -> Pace {
        let started = self.isolate.profiling().then(Instant::now);
        self.handle_requests(editor);
        for plugin in modes::wakes(editor) {
            plugins::request_wake(editor, &plugin);
        }
        plugins::wake_waiting(&mut self.isolate, editor);
        hooks::run(&mut self.isolate, editor);
        buffers::watch(&mut self.isolate, editor);
        commands::run_waiting(&mut self.isolate, editor);
        transients::run_waiting(&mut self.isolate, editor);
        takeover::run_endings(&mut self.isolate, editor);
        prompts::deliver(&mut self.isolate, &self.answers);
        if self.isolate.pending() > 0 {
            let report = self.isolate.run_with(editor, BUDGET);
            let failed = report
                .finished
                .into_iter()
                .filter_map(|(_, result)| result.err());
            for error in report.unhandled.into_iter().chain(failed) {
                log::report(editor, &error, None);
            }
        }
        buffers::watch(&mut self.isolate, editor);
        let highlighting = self.isolate.profiling().then(Instant::now);
        modes::highlight(&mut self.isolate, editor);
        self.timed("highlight", highlighting);
        let refreshing = self.isolate.profiling().then(Instant::now);
        views::refresh(&mut self.isolate, editor);
        self.timed("views", refreshing);
        self.timed("tick", started);
        self.pace()
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;

    use super::{Ox, Pace, log};

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

    fn press(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    fn run(editor: &mut Editor, ox: &mut Ox, command: &str) {
        let id = editor.command_named(command).unwrap();
        editor.execute(id);
        ox.tick(editor);
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
        press(&mut editor, "C-c g");
        ox.tick(&mut editor);

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

    fn home_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        for (name, text) in files {
            let path = home.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        home
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
        press(&mut editor, "C-c g");
        ox.tick(&mut editor);

        assert_eq!(editor.message(), Some("Hello"));
        assert_eq!(editor.buffer_named(log::LOG), None);
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
        ox.tick(&mut editor);

        assert_eq!(editor.message(), Some("Ox: lost"));
    }
}
