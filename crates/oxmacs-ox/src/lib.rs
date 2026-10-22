mod commands;
mod natives;

use std::time::Duration;

use ox::{Error, Isolate};
use oxmacs_core::editor::Editor;

const BUDGET: Duration = Duration::from_millis(4);
const PRELUDE: &str = include_str!("prelude.ox");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    Idle,
    Waiting,
    Running,
}

#[derive(Debug)]
pub struct Ox {
    isolate: Isolate,
}

impl Default for Ox {
    fn default() -> Self {
        Self::new()
    }
}

impl Ox {
    pub fn new() -> Self {
        let mut isolate = Isolate::new();
        natives::install(&mut isolate);
        commands::install(&mut isolate);
        if let Err(error) = isolate.extend_prelude("oxmacs/prelude.ox", PRELUDE) {
            panic!("The oxmacs prelude failed to load: {error}");
        }
        Self { isolate }
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
        commands::run_waiting(&mut self.isolate, editor);
        if self.isolate.pending() > 0 {
            let report = self.isolate.run_with(editor, BUDGET);
            for error in report.unhandled {
                editor.show_message(format!("Ox: {}", error.message));
            }
        }
        self.pace()
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;

    use super::{Ox, Pace};

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
