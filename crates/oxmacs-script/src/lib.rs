pub mod api;
pub mod ide;
mod runtime;
#[cfg(test)]
mod testing;
pub mod view;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use ox_lang::task::Wake;
use ox_lang::{Error, Isolate};
use oxmacs_core::config::Config;
use oxmacs_core::editor::Editor;
use oxmacs_core::text::BufferId;
pub use runtime::plugins::Bundle;

const BUDGET: Duration = Duration::from_millis(4);
const QUICK: Duration = Duration::from_millis(250);
const PATIENT: Duration = Duration::from_secs(10);
const PRELUDE: &str = oxmacs_api::PRELUDE;
const DOCS: &str = oxmacs_api::DOCS;

fn install_editor_api(isolate: &mut Isolate, answers: &api::prompts::Inbox) {
    oxmacs_api::add_editor_modules(isolate);
    api::natives::install(isolate);
    view::overlays::install(isolate);
    api::settings::install(isolate);
    api::commands::install(isolate);
    runtime::plugins::install(isolate);
    view::views::install(isolate);
    view::book::install(isolate);
    view::pdf::install(isolate);
    api::buffers::install(isolate);
    api::handles::install(isolate);
    api::editing::install(isolate);
    api::hooks::install(isolate);
    api::flashes::install(isolate);
    api::keystrokes::install(isolate);
    api::processes::install(isolate);
    api::consoles::install(isolate);
    api::layout::install(isolate);
    api::servers::install(isolate);
    api::transients::install(isolate);
    runtime::modes::install(isolate);
    view::takeover::install(isolate);
    api::games::install(isolate);
    rodeo::script::install(isolate);
    view::tabbar::install(isolate);
    api::idle::install(isolate);
    api::prompts::install(isolate, answers);
    api::keepass::install(isolate);
    ide::manual::install(isolate);
    isolate.add_native_docs(DOCS);
    oxmacs_api::extend_with_prelude(isolate);
    isolate.set_worker_setup(worker_api);
}

fn worker_api(isolate: &mut Isolate) {
    ox_library::install(isolate);
    install_editor_api(isolate, &api::prompts::Inbox::default());
}

fn editor_isolate(answers: &api::prompts::Inbox) -> Isolate {
    let mut isolate = ox_library::isolate();
    install_editor_api(&mut isolate, answers);
    isolate
}

pub fn isolate() -> Isolate {
    editor_isolate(&api::prompts::Inbox::default())
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
    answers: api::prompts::Inbox,
    evaluation: ide::repl::Evaluation,
    checker: ide::checker::Checker,
    help: ide::help::Help,
}

impl Default for Ox {
    fn default() -> Self {
        Self::new()
    }
}

impl Ox {
    pub fn new() -> Self {
        Self::with_bundled(runtime::bundled::BUNDLED)
    }

    pub fn with_bundled(bundled: &'static [Bundle]) -> Self {
        let answers = api::prompts::Inbox::default();
        Self {
            isolate: editor_isolate(&answers),
            bundled,
            home: None,
            answers,
            evaluation: ide::repl::Evaluation::default(),
            checker: ide::checker::Checker::default(),
            help: ide::help::Help::default(),
        }
    }

    pub fn start(&mut self, editor: &mut Editor, home: Option<&Path>) {
        self.start_with(editor, home, true);
    }

    pub fn start_without_init(&mut self, editor: &mut Editor, home: Option<&Path>) {
        self.start_with(editor, home, false);
    }

    fn start_with(&mut self, editor: &mut Editor, home: Option<&Path>, init: bool) {
        self.home = home.map(Path::to_path_buf);
        editor.let_host_play_macros();
        editor.set_init_path(home.map(|home| home.join("init.ox")));
        ide::repl::install(editor);
        api::hooks::watch(editor);
        api::games::watch(editor);
        runtime::plugins::discover(&mut self.isolate, editor, self.bundled, home);
        if init {
            self.run_init(editor, home);
        }
        runtime::plugins::wake_at_startup(&self.isolate, editor);
        view::views::refresh(&mut self.isolate, editor);
    }

    pub fn frame(&mut self, editor: &mut Editor, now: Instant, size: (f32, f32), font_size: f32) {
        let started = self.isolate.profiling().then(Instant::now);
        view::takeover::frame(&mut self.isolate, editor, now, size, font_size);
        self.timed("frame", started);
    }

    pub fn games_frame(&mut self, editor: &mut Editor, now: Instant) {
        let started = self.isolate.profiling().then(Instant::now);
        api::games::frame(&mut self.isolate, editor, now);
        self.timed("game", started);
    }

    fn timed(&mut self, kind: &str, started: Option<Instant>) {
        if let Some(started) = started {
            self.isolate.record_tick(kind, started.elapsed());
        }
    }

    pub fn idle(&mut self, editor: &mut Editor, since: Instant, now: Instant) {
        let rested = now.saturating_duration_since(since);
        api::idle::run(&mut self.isolate, editor, since, now);
        if rested >= ide::help::SIGNATURE_PAUSE {
            ide::help::echo_signature(&self.isolate, editor, self.home.as_deref(), &mut self.help);
        }
        self.checker.run(editor, rested, self.home.as_deref());
    }

    pub fn idle_wait(&self, editor: &Editor, since: Instant) -> Option<Duration> {
        let check = self.checker.wait(editor);
        let signature = ide::help::signature_wait(editor, &self.help);
        api::idle::next(editor, since)
            .into_iter()
            .chain(check)
            .chain(signature)
            .min()
    }

    pub fn end_take_over(&mut self, editor: &mut Editor) {
        view::takeover::finish(&mut self.isolate, editor);
    }

    pub fn press(&mut self, editor: &mut Editor, buffer: BufferId, slot: usize) {
        view::views::press(&mut self.isolate, editor, buffer, slot);
    }

    pub fn call(
        &mut self,
        editor: &mut Editor,
        buffer: BufferId,
        slot: usize,
        args: &[view::views::Arg],
    ) {
        view::views::call(&mut self.isolate, editor, buffer, slot, args);
    }

    pub fn tab_event(&mut self, editor: &mut Editor, event: view::tabbar::TabEvent) {
        view::tabbar::event(&mut self.isolate, editor, event);
    }

    pub fn view_sized(&mut self, editor: &mut Editor, buffer: BufferId, width: u32, height: u32) {
        view::views::sized(&mut self.isolate, editor, buffer, (width, height));
    }

    fn run_init(&mut self, editor: &mut Editor, home: Option<&Path>) -> bool {
        let Some(home) = home else {
            return false;
        };
        if editor.config() != &Config::default() {
            editor.reconfigure(&Config::default());
        }
        runtime::plugins::reset_configs(self.isolate.heap_mut(), editor);
        let custom = self.run_file(editor, &home.join("custom.ox"));
        api::settings::mark_custom(editor);
        runtime::plugins::mark_custom(self.isolate.heap_mut(), editor);
        let init = self.run_file(editor, &home.join("init.ox"));
        api::settings::mark_saved(editor);
        runtime::plugins::mark_saved(self.isolate.heap_mut(), editor);
        api::hooks::settings_changed(editor);
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
            ide::log::report(editor, &error, Some((&name, &source)));
            ide::log::show(editor);
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
        api::buffers::watch(&mut self.isolate, editor);
        let highlighting = self.isolate.profiling().then(Instant::now);
        runtime::modes::highlight(&mut self.isolate, editor);
        self.timed("highlight", highlighting);
        let refreshing = self.isolate.profiling().then(Instant::now);
        view::views::refresh(&mut self.isolate, editor);
        self.timed("views", refreshing);
        ide::help::auto_complete(&self.isolate, editor, self.home.as_deref(), &mut self.help);
        self.timed("tick", started);
        if ide::repl::requests_waiting(editor) || api::hooks::waiting(editor) {
            return Pace::Running;
        }
        self.pace()
    }

    fn drain(&mut self, editor: &mut Editor) -> bool {
        let mut failed = false;
        api::hooks::release_saves(&mut self.isolate, editor);
        self.handle_requests(editor);
        for plugin in runtime::modes::wakes(editor)
            .into_iter()
            .chain(runtime::plugins::woken_by_modes(editor))
        {
            runtime::plugins::request_wake(editor, &plugin);
        }
        runtime::plugins::wake_waiting(&mut self.isolate, editor);
        api::hooks::run(&mut self.isolate, editor);
        api::keystrokes::run(&mut self.isolate, editor);
        api::consoles::run(&mut self.isolate, editor);
        api::buffers::watch(&mut self.isolate, editor);
        api::commands::run_waiting(&mut self.isolate, editor);
        api::transients::run_waiting(&mut self.isolate, editor);
        view::takeover::run_endings(&mut self.isolate, editor);
        api::prompts::deliver(&mut self.isolate, &self.answers);
        ide::manual::deliver(&mut self.isolate, editor);
        let deadline = Instant::now() + BUDGET;
        while self.isolate.pending() > 0 {
            let before = self.evaluating();
            let report = self
                .isolate
                .run_with(editor, deadline.saturating_duration_since(Instant::now()));
            let evaluated = self.evaluated(editor, report.finished);
            for error in report.unhandled.into_iter().chain(evaluated) {
                failed = true;
                ide::log::report(editor, &error, None);
            }
            self.advance(editor);
            let after = self.evaluating();
            if after.is_none() || after == before || Instant::now() >= deadline {
                break;
            }
        }
        api::flashes::run(&mut self.isolate, editor);
        failed
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use super::{Ox, Pace, ide};
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
        let problems = super::isolate().undocumented();

        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn the_checking_stubs_take_what_the_editor_natives_take() {
        let real = super::isolate().natives();
        let stubs = oxmacs_api::editor_isolate().natives();

        let missing: Vec<&String> = real
            .iter()
            .map(|(name, _, _)| name)
            .filter(|name| !stubs.iter().any(|(stub, _, _)| stub == *name))
            .collect();
        let different: Vec<String> = stubs
            .iter()
            .filter_map(|(name, least, most)| {
                let (_, real_least, real_most) = real.iter().find(|(known, _, _)| known == name)?;
                (least != real_least || (most.is_some() && most != real_most)).then(|| {
                    format!("{name}: stub {least}..{most:?}, editor {real_least}..{real_most:?}")
                })
            })
            .collect();
        assert!(missing.is_empty(), "no stub for {missing:?}");
        assert!(different.is_empty(), "{}", different.join("\n"));
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
        assert_eq!(editor.buffer_named(ide::log::LOG), None);
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
    fn starting_without_init_files_skips_custom_ox_and_init_ox_but_keeps_plugins() {
        let home = home_with(&[
            ("custom.ox", "(settings {clock: false})"),
            ("init.ox", "(settings {tab-width: 7})"),
        ]);
        let mut editor = Editor::new();

        Ox::new().start_without_init(&mut editor, Some(home.path()));

        assert_eq!(editor.config(), &oxmacs_core::config::Config::default());
        assert!(editor.command_named("dired").is_some());
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
        let log = editor.buffer_named(ide::log::LOG).unwrap();
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
        let log = editor.buffer_named(ide::log::LOG).unwrap();
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

        assert_eq!(editor.buffer_named(ide::log::LOG), None);
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
