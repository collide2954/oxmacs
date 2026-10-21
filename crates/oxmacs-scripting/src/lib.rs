mod script;

use oxmacs_core::applet::Applet;
use oxmacs_core::command::{CommandSpec, Scope};
use oxmacs_core::editor::Editor;
use oxmacs_gui::applet::GuiApplet;
use rhai::Dynamic;

use script::Failure;

#[derive(Debug)]
pub struct Scripting;

fn report(editor: &mut Editor, outcome: Result<Dynamic, Failure>) {
    match outcome {
        Ok(value) if value.is_unit() => {}
        Ok(value) => editor.show_message(format!("{value:?}")),
        Err(error) => editor.show_message(script::describe(*error)),
    }
}

fn eval_buffer(editor: &mut Editor) {
    let code = editor.buffer().contents();
    report(editor, script::evaluate(&code));
}

impl Applet for Scripting {
    fn name(&self) -> &'static str {
        "scripting"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn opt_in(&self) -> bool {
        true
    }

    fn install(&self, editor: &mut Editor) {
        editor.add_command(CommandSpec {
            name: "script-eval-buffer",
            summary: "Evaluate the buffer as a Rhai script",
            help: Some(
                "Runs the whole buffer as a Rhai script and shows its value in the echo area, or the error with its line and column. Nothing runs until you ask.",
            ),
            scope: Scope::Global.into(),
            run: eval_buffer,
        });
    }
}

impl GuiApplet for Scripting {}

#[cfg(test)]
pub(crate) mod testing {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;

    use crate::Scripting;

    pub(crate) fn installed() -> Editor {
        let mut editor = Editor::new();
        editor.install(&Scripting);
        editor
    }

    pub(crate) fn press(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    pub(crate) fn type_text(editor: &mut Editor, text: &str) {
        for character in text.chars() {
            let notation = match character {
                ' ' => "SPC".to_owned(),
                '\n' => "RET".to_owned(),
                other => other.to_string(),
            };
            press(editor, &notation);
        }
    }

    pub(crate) fn m_x(editor: &mut Editor, command: &str) {
        press(editor, "M-x");
        type_text(editor, command);
        press(editor, "RET");
    }

    pub(crate) fn with_text(text: &str) -> Editor {
        let mut editor = installed();
        editor.buffer_mut().insert(text);
        editor
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{installed, m_x, with_text};

    #[test]
    fn the_applet_is_opt_in_with_its_own_version() {
        let editor = installed();

        let applet = editor.installed()[0];
        assert_eq!(applet.name, "scripting");
        assert_eq!(applet.version, "0.1.0");
        assert!(applet.opt_in);
    }

    #[test]
    fn evaluating_the_buffer_shows_the_result() {
        let mut editor = with_text("let x = 40;\nx + 2\n");

        m_x(&mut editor, "script-eval-buffer");

        assert_eq!(editor.message(), Some("42"));
    }

    #[test]
    fn strings_show_quoted_and_unit_shows_nothing() {
        let mut editor = with_text("\"hi\" + \"!\"");
        m_x(&mut editor, "script-eval-buffer");
        assert_eq!(editor.message(), Some("\"hi!\""));

        let mut editor = with_text("let x = 1;");
        m_x(&mut editor, "script-eval-buffer");
        assert_eq!(editor.message(), None);
    }

    #[test]
    fn errors_say_where_they_happened() {
        let mut editor = with_text("let x = 1;\nx + y\n");
        m_x(&mut editor, "script-eval-buffer");
        assert_eq!(
            editor.message(),
            Some("Variable not found: y (line 2, column 5)")
        );

        let mut editor = with_text("let = 3;");
        m_x(&mut editor, "script-eval-buffer");
        let message = editor.message().unwrap_or_default();
        assert!(message.starts_with("Syntax error: "), "{message}");
        assert!(message.ends_with("(line 1, column 5)"), "{message}");
    }

    #[test]
    fn thrown_values_show_without_a_prefix() {
        let mut editor = with_text("throw \"boom\";");

        m_x(&mut editor, "script-eval-buffer");

        assert_eq!(editor.message(), Some("boom (line 1, column 1)"));
    }
}
