mod indent;
mod mode;

use oxmacs_core::applet::Applet;
use oxmacs_core::command::{CommandId, CommandSpec, Scope, ScopeId};
use oxmacs_core::editor::Editor;
use oxmacs_gui::applet::GuiApplet;

pub use mode::LANGUAGE;

struct Spec {
    name: &'static str,
    summary: &'static str,
    help: Option<&'static str>,
    keys: &'static [&'static str],
    run: fn(&mut Editor),
}

const GLOBAL: &[Spec] = &[Spec {
    name: "python-mode",
    summary: "Edit the buffer as Python",
    help: None,
    keys: &[],
    run: |editor| editor.set_major_mode(Some(LANGUAGE)),
}];

const PYTHON: &[Spec] = &[
    Spec {
        name: "python-indent-line",
        summary: "Indent the line as a best guess; again right away to cycle the levels",
        help: Some(
            "The guess goes one level deeper after a line ending in a colon, one level shallower after return, pass, break, continue or raise, and otherwise matches the previous line of code. Pressing TAB again right away cycles through the other levels, from no indentation to one level deeper than the previous line of code.",
        ),
        keys: &["TAB"],
        run: indent::indent_line,
    },
    Spec {
        name: "python-newline-and-indent",
        summary: "Start a new line indented as TAB would guess",
        help: None,
        keys: &["RET", "C-j"],
        run: indent::newline,
    },
];

fn register(editor: &mut Editor, scope: ScopeId, specs: &[Spec]) -> Vec<CommandId> {
    let mut added = Vec::new();
    for spec in specs {
        let command = editor.add_command(CommandSpec {
            name: spec.name,
            summary: spec.summary,
            help: spec.help,
            scope,
            run: spec.run,
        });
        for keys in spec.keys {
            editor.bind(scope, keys, command).ok();
        }
        added.push(command);
    }
    added
}

#[derive(Debug)]
pub struct Python;

impl Applet for Python {
    fn name(&self) -> &'static str {
        "python"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn opt_in(&self) -> bool {
        true
    }

    fn install(&self, editor: &mut Editor) {
        let python = editor.add_scope("Python buffers");
        register(editor, Scope::Global.into(), GLOBAL);
        register(editor, python, PYTHON);
        editor.add_language(LANGUAGE, Some(python));
    }
}

impl GuiApplet for Python {}

#[cfg(test)]
pub(crate) mod testing {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;
    use oxmacs_core::text::Buffer;

    use crate::{LANGUAGE, Python};

    pub(crate) fn installed() -> Editor {
        let mut editor = Editor::new();
        editor.install(&Python);
        editor
    }

    pub(crate) fn editor_with(text: &str, point: usize) -> Editor {
        let mut buffer = Buffer::with_text("t.py", text);
        buffer.set_point(point);
        let mut editor = Editor::with_buffer(buffer);
        editor.install(&Python);
        editor.set_major_mode(Some(LANGUAGE));
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
                other => other.to_string(),
            };
            press(editor, &notation);
        }
    }

    pub(crate) fn run(editor: &mut Editor, command: &str) {
        press(editor, "M-x");
        type_text(editor, command);
        press(editor, "RET");
    }

    pub(crate) fn lines(editor: &Editor) -> Vec<String> {
        editor
            .buffer()
            .contents()
            .split('\n')
            .map(str::to_owned)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::LANGUAGE;
    use crate::testing::{installed, lines, press, run, type_text};

    #[test]
    fn python_files_and_stubs_open_in_python_mode() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("app.py"), "x = 1\n").unwrap();
        fs::write(dir.path().join("app.pyi"), "x: int\n").unwrap();
        let mut editor = installed();

        for name in ["app.py", "app.pyi"] {
            editor.open_file(&dir.path().join(name));

            assert_eq!(editor.language(), Some(LANGUAGE), "{name}");
            assert!(
                editor.mode_line().ends_with("(Python)"),
                "{}",
                editor.mode_line()
            );
        }
    }

    #[test]
    fn python_files_stay_plain_text_without_the_applet() {
        let editor = oxmacs_core::editor::Editor::new();

        assert_eq!(editor.language_for(std::path::Path::new("app.py")), None);
    }

    #[test]
    fn python_mode_is_a_command() {
        let mut editor = installed();

        run(&mut editor, "python-mode");

        assert_eq!(editor.language(), Some(LANGUAGE));
    }

    #[test]
    fn comments_use_hashes() {
        let mut editor = installed();
        run(&mut editor, "python-mode");
        type_text(&mut editor, "x = 1");

        press(&mut editor, "C-a C-x C-;");

        assert_eq!(lines(&editor), ["# x = 1"]);
    }

    #[test]
    fn the_doctor_shows_what_python_supports() {
        let mut editor = installed();

        run(&mut editor, "oxmacs-doctor");

        let row = lines(&editor)
            .into_iter()
            .find(|line| line.trim_start().starts_with("Python (.py .pyi)"));
        assert!(row.is_some_and(|row| row.contains('✓')));
    }
}
