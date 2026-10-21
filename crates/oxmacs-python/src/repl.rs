use std::iter;
use std::ops::Range;

use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::{ConsoleSpec, Editor, JobSpec};
use oxmacs_core::syntax::Syntax;
use oxmacs_core::text::{Buffer, BufferId};
use tree_sitter::{Node, Point};

use crate::indent::{self, parsed};
use crate::tools;

const LABEL: &str = "Python";
const NOTHING: &str = "No Python code here";
const STARTUP: &str = r"import sys; sys.ps1 = '\x01>\x02>>> \x03'; sys.ps2 = '\x01+\x02... \x03'";
const COMPOUND: &[&str] = &[
    "function_definition",
    "class_definition",
    "decorated_definition",
    "if_statement",
    "for_statement",
    "while_statement",
    "try_statement",
    "with_statement",
    "match_statement",
];

#[derive(Debug)]
pub(crate) struct ConsoleScope(pub(crate) ScopeId);

fn ancestors(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    iter::successors(Some(node), Node::parent)
}

fn dedent(code: &str) -> String {
    let margin = code
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    code.lines()
        .map(|line| line.get(margin..).unwrap_or_else(|| line.trim_start()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn last_row(node: Node<'_>) -> usize {
    let end = node.end_position();
    if end.column == 0 && end.row > node.start_position().row {
        end.row - 1
    } else {
        end.row
    }
}

fn in_string(root: Node<'_>, row: usize) -> bool {
    let point = Point { row, column: 0 };
    root.descendant_for_point_range(point, point)
        .is_some_and(|node| {
            ancestors(node)
                .any(|ancestor| ancestor.kind() == "string" && ancestor.start_position().row < row)
        })
}

pub(crate) fn interactive(code: &str) -> (String, bool) {
    let dedented = dedent(code);
    let (buffer, syntax) = parsed(&dedented);
    let Some(tree) = syntax.tree() else {
        return (dedented, false);
    };
    let root = tree.root_node();
    let mut cursor = root.walk();
    let statements: Vec<Node<'_>> = root
        .named_children(&mut cursor)
        .filter(|node| node.kind() != "comment")
        .collect();
    let compound = |node: &Node<'_>| COMPOUND.contains(&node.kind());
    let ends: Vec<usize> = statements
        .iter()
        .filter(|node| compound(node))
        .map(|node| last_row(*node))
        .collect();
    let last = statements.last().map(|node| last_row(*node));
    let mut lines = Vec::new();
    for row in 0..buffer.len_lines() {
        if !in_string(root, row) && indent::code(&buffer, &syntax, row).trim().is_empty() {
            continue;
        }
        lines.push(buffer.line_text(row).into_owned());
        if ends.contains(&row) && Some(row) != last {
            lines.push(String::new());
        }
    }
    (lines.join("\n"), statements.last().is_some_and(compound))
}

fn session_name(editor: &Editor) -> String {
    let directory = editor.default_directory();
    let folder = directory.file_name().map_or_else(
        || LABEL.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    format!("*Python:{folder}*")
}

fn running(editor: &mut Editor) -> Option<BufferId> {
    let existing = editor.consoles(LABEL).first().copied();
    if let Some(id) = existing.filter(|id| editor.console_running(*id)) {
        return Some(id);
    }
    let python = match tools::interpreter(editor) {
        Ok(python) => python,
        Err(error) => {
            editor.show_message(error);
            return None;
        }
    };
    let name = existing
        .and_then(|id| editor.buffer_by_id(id))
        .map_or_else(|| session_name(editor), |buffer| buffer.name().to_owned());
    let spec = ConsoleSpec {
        name: name.clone(),
        label: LABEL,
        job: JobSpec {
            program: python.display().to_string(),
            args: ["-i", "-q", "-u", "-c", STARTUP]
                .map(str::to_owned)
                .to_vec(),
            directory: editor.default_directory(),
            env: vec![("PYTHONIOENCODING".to_owned(), "utf-8".to_owned())],
            label: "Python REPL",
            name,
            buffer: None,
            interactive: true,
        },
        on_marker: |_, _, _, _| {},
        on_idle: |_, _| {},
        scope: editor.applet_state::<ConsoleScope>().map(|known| known.0),
    };
    Some(editor.start_console(spec, existing))
}

fn show(editor: &mut Editor, id: BufferId, select: bool) {
    let origin = editor.selected_window();
    if !editor.buffer_shown(id) {
        editor.show_buffer_beside(id);
    }
    let showing = editor.tiling().windows().into_iter().find(|window| {
        editor
            .pane_of(*window)
            .is_some_and(|pane| pane.buffer().id() == id)
    });
    let target = if select { showing } else { None };
    editor.select_window(target.unwrap_or(origin));
}

pub(crate) fn switch(editor: &mut Editor) {
    if let Some(id) = running(editor) {
        show(editor, id, true);
    }
}

fn send(editor: &mut Editor, code: &str) {
    let Some(id) = running(editor) else {
        return;
    };
    show(editor, id, false);
    let (lines, finish) = interactive(code);
    editor.console_evaluate(id, &lines);
    if finish {
        editor.console_request(id, "");
    }
}

fn send_selection(editor: &mut Editor) -> bool {
    let buffer = editor.buffer();
    let Some(selection) = buffer.selection() else {
        return false;
    };
    let line_start = buffer.line_start(buffer.line_of(selection.start));
    let start = if buffer
        .text_in(line_start..selection.start)
        .trim()
        .is_empty()
    {
        line_start
    } else {
        selection.start
    };
    let code = buffer.text_in(start..selection.end);
    editor.buffer_mut().deactivate_mark();
    send(editor, &code);
    true
}

fn paragraph(buffer: &Buffer, point: usize) -> Option<Range<usize>> {
    let blank = |line: usize| buffer.line_text(line).trim().is_empty();
    let lines = buffer.len_lines();
    let mut first = (buffer.line_of(point)..lines).find(|line| !blank(*line))?;
    let mut last = first;
    while first > 0 && !blank(first - 1) {
        first -= 1;
    }
    while last + 1 < lines && !blank(last + 1) {
        last += 1;
    }
    Some(buffer.line_start(first)..buffer.line_end(last))
}

fn send_and_step(
    editor: &mut Editor,
    buffer: &Buffer,
    syntax: &Syntax,
    range: Option<Range<usize>>,
) {
    let Some(range) = range else {
        editor.show_message(NOTHING);
        return;
    };
    let start = buffer.line_start(buffer.line_of(range.start));
    let code = buffer.text_in(start..range.end);
    let next = syntax
        .statement_after(buffer, range.end)
        .unwrap_or_else(|| buffer.len_chars());
    editor.buffer_mut().set_point(next);
    send(editor, &code);
}

pub(crate) fn send_statement(editor: &mut Editor) {
    if send_selection(editor) {
        return;
    }
    let (buffer, syntax) = parsed(&editor.buffer().contents());
    let range = syntax.statement_at(&buffer, editor.buffer().point());
    send_and_step(editor, &buffer, &syntax, range);
}

pub(crate) fn send_defun(editor: &mut Editor) {
    if send_selection(editor) {
        return;
    }
    let (buffer, syntax) = parsed(&editor.buffer().contents());
    let point = editor.buffer().point();
    let range = syntax
        .defun_around(&buffer, point)
        .or_else(|| paragraph(&buffer, point));
    send_and_step(editor, &buffer, &syntax, range);
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use oxmacs_core::config::Config;
    use oxmacs_core::editor::{Editor, Effect, Job, JobId};

    use super::interactive;
    use crate::testing::{editor_with, press};

    const PS1: &str = "\u{1}>\u{2}>>> \u{3}";
    const PS2: &str = "\u{1}+\u{2}... \u{3}";

    fn sent_as(code: &str) -> (String, bool) {
        interactive(code)
    }

    #[test]
    fn simple_statements_go_as_they_are() {
        assert_eq!(sent_as("x = 1\n"), ("x = 1".to_owned(), false));
        assert_eq!(
            sent_as("x = foo(\n    1,\n)\n"),
            ("x = foo(\n    1,\n)".to_owned(), false)
        );
    }

    #[test]
    fn blocks_lose_their_blank_lines_and_end_with_one() {
        assert_eq!(
            sent_as("def f():\n    x = 1\n\n    return x\n"),
            ("def f():\n    x = 1\n    return x".to_owned(), true)
        );
        assert_eq!(
            sent_as("def f():\n    pass\n\n\nx = f()\n"),
            ("def f():\n    pass\n\nx = f()".to_owned(), false)
        );
    }

    #[test]
    fn indented_code_is_dedented() {
        assert_eq!(
            sent_as("    if a:\n        b()\n"),
            ("if a:\n    b()".to_owned(), true)
        );
    }

    #[test]
    fn strings_keep_their_blank_lines_and_comments_are_left_out() {
        assert_eq!(
            sent_as("s = \"\"\"a\n\nb\"\"\"\n"),
            ("s = \"\"\"a\n\nb\"\"\"".to_owned(), false)
        );
        assert_eq!(
            sent_as("# note\nx = 1  # keep\n"),
            ("x = 1  # keep".to_owned(), false)
        );
    }

    fn python(folder: &Path) -> PathBuf {
        let path = folder.join(format!("python{}", std::env::consts::EXE_SUFFIX));
        fs::write(&path, "").unwrap();
        path
    }

    fn script(text: &str, point: usize) -> (tempfile::TempDir, PathBuf, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let interpreter = python(dir.path());
        let mut editor = editor_with(text, point);
        editor.set_working_directory(dir.path().join("analysis"));
        editor.apply_config(
            &Config::parse(&format!(
                "[python]\ninterpreter = '{}'\n",
                interpreter.display()
            ))
            .unwrap(),
        );
        (dir, interpreter, editor)
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

    fn sent(effects: &[Effect]) -> Vec<String> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Send(_, text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn started(editor: &mut Editor) -> JobId {
        let script = editor.selected_window();
        press(editor, "C-c C-z");
        let job = sessions(editor).pop().unwrap().id;
        editor.session_output(job, PS1);
        editor.select_window(script);
        editor.take_effects();
        job
    }

    #[test]
    fn c_c_c_z_starts_python_with_marked_prompts_and_shows_it() {
        let (_dir, interpreter, mut editor) = script("x = 1\n", 0);

        press(&mut editor, "C-c C-z");

        let job = sessions(&mut editor).pop().unwrap();
        assert_eq!(job.program, interpreter.display().to_string());
        assert_eq!(job.args[..4], ["-i", "-q", "-u", "-c"]);
        assert!(
            job.args[4].contains(r"sys.ps1 = '\x01>\x02>>> \x03'"),
            "{:?}",
            job.args
        );
        assert!(
            job.args[4].contains(r"sys.ps2 = '\x01+\x02... \x03'"),
            "{:?}",
            job.args
        );
        assert!(
            job.env
                .contains(&("PYTHONIOENCODING".to_owned(), "utf-8".to_owned()))
        );
        assert_eq!(editor.buffer().name(), "*Python:analysis*");
        assert_eq!(editor.tiling().windows().len(), 2);
        editor.session_output(job.id, &format!("hello\n{PS1}"));
        assert_eq!(editor.buffer().contents(), "hello\n>>> ");
    }

    #[test]
    fn the_console_has_keys_of_its_own_and_none_of_rs() {
        let (_dir, _interpreter, mut editor) = script("x = 1\n", 0);
        press(&mut editor, "C-c C-z");
        let job = sessions(&mut editor).pop().unwrap().id;
        editor.session_output(job, PS1);

        press(&mut editor, "1 RET C-c C-v");

        assert_eq!(sent(&editor.take_effects()), ["1\n"]);
        assert_eq!(editor.message(), Some("C-c C-v is undefined"));
    }

    #[test]
    fn a_running_console_is_reused() {
        let (_dir, _interpreter, mut editor) = script("x = 1\n", 0);
        started(&mut editor);

        press(&mut editor, "C-c C-z");

        assert_eq!(sessions(&mut editor), []);
        assert_eq!(editor.buffer().name(), "*Python:analysis*");
    }

    #[test]
    fn control_return_sends_the_statement_and_steps_to_the_next() {
        let (_dir, _interpreter, mut editor) = script("x = 1\n\ny = [\n    2,\n]\n", 2);

        press(&mut editor, "C-RET");

        let effects = editor.take_effects();
        assert!(matches!(effects.first(), Some(Effect::StartSession(_))));
        assert_eq!(sent(&effects), ["x = 1\n"]);
        assert_eq!(editor.buffer().name(), "t.py");
        assert_eq!(editor.buffer().point(), 7);
        assert_eq!(editor.tiling().windows().len(), 2);

        press(&mut editor, "C-RET");

        assert_eq!(sent(&editor.take_effects()), ["y = [\n    2,\n]\n"]);
        assert_eq!(editor.buffer().point(), editor.buffer().len_chars());
    }

    #[test]
    fn statements_inside_a_body_go_on_their_own() {
        let text = "def f():\n    if a:\n        b()\n    return 1\n";
        let (_dir, _interpreter, mut editor) = script(text, 13);
        started(&mut editor);

        press(&mut editor, "C-RET");

        assert_eq!(sent(&editor.take_effects()), ["if a:\n    b()\n", "\n"]);
        assert_eq!(editor.buffer().point(), 35);
    }

    #[test]
    fn blocks_are_echoed_line_by_line_and_end_with_a_hidden_blank_line() {
        let text = "def f():\n    return 1\n";
        let (_dir, _interpreter, mut editor) = script(text, 0);
        let job = started(&mut editor);

        press(&mut editor, "C-RET");
        editor.session_output(job, &format!("{PS2}{PS2}{PS1}"));

        let console = editor
            .buffer_named("*Python:analysis*")
            .and_then(|id| editor.buffer_by_id(id))
            .map(oxmacs_core::text::Buffer::contents);
        assert_eq!(
            console.as_deref(),
            Some(">>> def f():\n...     return 1\n>>> ")
        );
    }

    #[test]
    fn control_c_control_c_sends_the_whole_class_or_def() {
        let text = "x = 1\n\n@dataclass\nclass A:\n    def f(self):\n        return 1\n\ny = 2\n";
        let (_dir, _interpreter, mut editor) = script(text, 50);
        started(&mut editor);

        press(&mut editor, "C-c C-c");

        assert_eq!(
            sent(&editor.take_effects()),
            [
                "@dataclass\nclass A:\n    def f(self):\n        return 1\n",
                "\n"
            ]
        );
        assert_eq!(editor.buffer().point(), 62);
    }

    #[test]
    fn control_c_control_c_outside_a_def_sends_the_paragraph() {
        let (_dir, _interpreter, mut editor) = script("a = 1\nb = 2\n\nc = 3\n", 0);
        started(&mut editor);

        press(&mut editor, "C-c C-c");

        assert_eq!(sent(&editor.take_effects()), ["a = 1\nb = 2\n"]);
    }

    #[test]
    fn a_selection_is_sent_dedented_and_point_stays() {
        let text = "def f():\n    a = 1\n    b = 2\n";
        let (_dir, _interpreter, mut editor) = script(text, 13);
        started(&mut editor);
        press(&mut editor, "C-SPC C-n C-e");

        press(&mut editor, "C-RET");

        assert_eq!(sent(&editor.take_effects()), ["a = 1\nb = 2\n"]);
        assert_eq!(editor.buffer().point(), 28);
    }

    #[test]
    fn blank_buffers_have_nothing_to_send() {
        let (_dir, _interpreter, mut editor) = script("\n# only\n", 0);
        started(&mut editor);

        press(&mut editor, "C-RET");

        assert_eq!(editor.message(), Some("No Python code here"));
        assert_eq!(sent(&editor.take_effects()), Vec::<String>::new());
    }

    #[test]
    fn a_missing_interpreter_starts_nothing() {
        let mut editor = editor_with("x = 1\n", 0);
        editor.set_working_directory(std::env::temp_dir());
        editor.apply_config(
            &Config::parse("[python]\ninterpreter = \"oxmacs-no-such-python\"\n").unwrap(),
        );

        press(&mut editor, "C-RET");

        assert_eq!(
            editor.message(),
            Some("[python] interpreter not found: oxmacs-no-such-python")
        );
        assert_eq!(sessions(&mut editor), []);
    }
}
