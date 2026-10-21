mod indent;
mod mode;
mod repl;
mod ruff;
mod run;
mod tools;
mod uv;

use oxmacs_core::applet::Applet;
use oxmacs_core::command::{Command, CommandId, CommandSpec, Scope, ScopeId};
use oxmacs_core::editor::Editor;
use oxmacs_gui::applet::GuiApplet;

pub use mode::LANGUAGE;
pub use tools::{RUFF, Settings, UV};

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
    Spec {
        name: "python-run-file",
        summary: "Run this file with the project's Python in *compilation*",
        help: Some(
            "Python is the project's .venv, looked for upward from the file, then [python] interpreter, then python3 or python on PATH. The file runs in the folder holding pyproject.toml, or its own folder, and the lines of a traceback visit the code they name.",
        ),
        keys: &["C-c C-r"],
        run: run::run_file,
    },
    Spec {
        name: "python-pytest",
        summary: "Run pytest for the project in *compilation*",
        help: Some(
            "pytest runs with the project's Python in the folder holding pyproject.toml, and prints failures as Python tracebacks whose lines visit the code they name.",
        ),
        keys: &["C-c C-t"],
        run: run::pytest,
    },
    Spec {
        name: "python-shell-send-statement",
        summary: "Send the statement at point, or the region, to the Python console",
        help: Some(
            "Point moves on to the next statement. The console starts if it is not running. Code is sent the way the console reads it: dedented, without the blank lines and comments between statements, and with a blank line closing each block.",
        ),
        keys: &["C-RET"],
        run: repl::send_statement,
    },
    Spec {
        name: "python-shell-send-defun",
        summary: "Send the class or def around point, or the paragraph, to the Python console",
        help: None,
        keys: &["C-c C-c"],
        run: repl::send_defun,
    },
    Spec {
        name: "python-shell-switch-to-shell",
        summary: "Show the Python console, starting it if needed",
        help: Some(
            "The console runs the project's Python, the same one C-c C-r uses, over pipes in the file's folder.",
        ),
        keys: &["C-c C-z"],
        run: repl::switch,
    },
    Spec {
        name: "python-uv-dashboard",
        summary: "Show the project's uv dashboard",
        help: Some(
            "The dashboard in *uv* shows the project from pyproject.toml, the Python in its .venv, whether uv.lock is up to date, and the required, locked and newer packages. uv checks run in the background; g reads everything again.",
        ),
        keys: &["C-c C-u"],
        run: uv::open,
    },
];

const DASHBOARD: &[Spec] = &[
    Spec {
        name: "uv-add",
        summary: "Add a dependency to the project with uv add",
        help: Some(
            "The answer goes to uv add as it is typed, so it can be a requirement such as httpx>=0.27 or several, with options such as --dev.",
        ),
        keys: &["a"],
        run: uv::add,
    },
    Spec {
        name: "uv-remove",
        summary: "Remove the dependency at point with uv remove",
        help: None,
        keys: &["d"],
        run: uv::remove,
    },
    Spec {
        name: "uv-upgrade",
        summary: "Upgrade the package at point in uv.lock",
        help: None,
        keys: &["u"],
        run: uv::upgrade,
    },
    Spec {
        name: "uv-sync",
        summary: "Bring the .venv in line with uv.lock with uv sync",
        help: None,
        keys: &["s"],
        run: uv::sync,
    },
    Spec {
        name: "uv-lock",
        summary: "Update uv.lock with uv lock",
        help: None,
        keys: &["l"],
        run: uv::lock,
    },
    Spec {
        name: "uv-refresh",
        summary: "Read the project again and check it with uv",
        help: None,
        keys: &["g"],
        run: uv::refresh,
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
        editor.bind(python, "C-c C-f", Command::FormatBuffer).ok();
        editor.add_language(LANGUAGE, Some(python));
        editor.add_config_table::<Settings>(tools::TABLE);
        editor.add_settings(tools::TABLE, tools::SETTINGS);
        editor.add_managed_tool(RUFF);
        editor.add_managed_tool(UV);
        editor.add_formatter(LANGUAGE, ruff::FORMAT);
        editor.add_linter(LANGUAGE, ruff::LINT);
        editor.add_error_pattern(run::traceback);
        let dashboard = editor.add_scope("uv dashboard");
        editor.set_applet_state(uv::UvState { scope: dashboard });
        register(editor, dashboard, DASHBOARD);
        editor.bind(dashboard, "q", Command::QuitWindow).ok();
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
