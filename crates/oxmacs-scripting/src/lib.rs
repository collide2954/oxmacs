mod api;
mod language;
mod script;

use oxmacs_core::applet::Applet;
use oxmacs_core::command::{CommandSpec, Scope};
use oxmacs_core::editor::Editor;
use oxmacs_gui::applet::GuiApplet;
use rhai::Dynamic;

use script::{Failure, Origin};

#[derive(Debug)]
pub struct Scripting;

fn report(editor: &mut Editor, outcome: Result<Dynamic, Failure>, origin: Origin) {
    match outcome {
        Ok(value) if value.is_unit() => {}
        Ok(value) => editor.show_message(format!("{value:?}")),
        Err(error) => editor.show_message(script::describe(*error, origin)),
    }
}

fn eval_buffer(editor: &mut Editor) {
    let code = editor.buffer().contents();
    let outcome = script::evaluate(editor, &code);
    report(editor, outcome, Origin::default());
}

fn eval_region(editor: &mut Editor) {
    let buffer = editor.buffer();
    let Some(range) = buffer.selection() else {
        editor.show_message("Select the code to evaluate");
        return;
    };
    let line = buffer.line_of(range.start);
    let origin = Origin {
        line,
        column: range.start - buffer.line_start(line),
    };
    let code = buffer.text_in(range);
    let outcome = script::evaluate(editor, &code);
    report(editor, outcome, origin);
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
        let buffer = editor.add_command(CommandSpec {
            name: "script-eval-buffer",
            summary: "Evaluate the buffer as a Rhai script",
            help: Some(
                "Runs the whole buffer as a Rhai script and shows its value in the echo area, or the error with its line and column. Nothing runs until you ask.",
            ),
            scope: Scope::Global.into(),
            run: eval_buffer,
        });
        let region = editor.add_command(CommandSpec {
            name: "script-eval-region",
            summary: "Evaluate the selected code as a Rhai script",
            help: Some(
                "Runs the selected code as a Rhai script and shows its value in the echo area, or the error with its line and column in the buffer.",
            ),
            scope: Scope::Global.into(),
            run: eval_region,
        });
        let rhai = editor.add_scope("Rhai buffers");
        editor.add_language(language::LANGUAGE, Some(rhai));
        editor.set_highlighter(language::LANGUAGE, language::highlight);
        editor.bind(rhai, "C-c C-c", buffer).ok();
        editor.bind(rhai, "C-c C-r", region).ok();
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

    pub(crate) fn opened(name: &str, text: &str) -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        let mut editor = installed();
        editor.open_file(&path);
        (dir, editor)
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::theme::Highlight;

    use crate::testing::{installed, m_x, opened, press, with_text};

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
    fn evaluating_the_region_runs_only_the_selection() {
        let mut editor = with_text("1 + 1\n2 * 21\n");

        press(&mut editor, "M-< C-n C-SPC C-e");
        m_x(&mut editor, "script-eval-region");

        assert_eq!(editor.message(), Some("42"));
    }

    #[test]
    fn region_errors_say_where_they_are_in_the_buffer() {
        let mut editor = with_text("let a = 1;\nlet b = 1 + zz;\n");

        press(
            &mut editor,
            "M-< C-n C-f C-f C-f C-f C-f C-f C-f C-f C-SPC C-e",
        );
        m_x(&mut editor, "script-eval-region");

        assert_eq!(
            editor.message(),
            Some("Variable not found: zz (line 2, column 13)")
        );
    }

    #[test]
    fn evaluating_the_region_without_one_asks_for_it() {
        let mut editor = with_text("1 + 1");

        m_x(&mut editor, "script-eval-region");

        assert_eq!(editor.message(), Some("Select the code to evaluate"));
    }

    #[test]
    fn rhai_files_evaluate_with_c_c_c_c_and_c_c_c_r() {
        let (_dir, mut editor) = opened("answer.rhai", "let a = 40;\na + 2\n");
        assert!(
            editor.mode_line().ends_with("(Rhai)"),
            "{}",
            editor.mode_line()
        );

        press(&mut editor, "C-c C-c");
        assert_eq!(editor.message(), Some("42"));

        press(&mut editor, "M-< M-f M-f M-f M-b C-SPC M-f C-c C-r");
        assert_eq!(editor.message(), Some("40"));
    }

    #[test]
    fn rhai_files_are_highlighted_and_comment_with_two_slashes() {
        let (_dir, mut editor) = opened("note.rhai", "let x = 1;\n");
        assert_eq!(editor.line_style(0).spans, [(0..3, Highlight::Keyword)]);

        press(&mut editor, "C-x C-;");

        assert!(
            editor.buffer().contents().starts_with("// let x = 1;"),
            "{}",
            editor.buffer().contents()
        );
    }

    #[test]
    fn thrown_values_show_without_a_prefix() {
        let mut editor = with_text("throw \"boom\";");

        m_x(&mut editor, "script-eval-buffer");

        assert_eq!(editor.message(), Some("boom (line 1, column 1)"));
    }
}
