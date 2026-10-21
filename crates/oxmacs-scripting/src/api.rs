use std::sync::Arc;

use oxmacs_core::command::{DynamicSpec, Scope};
use oxmacs_core::editor::Editor;
use rhai::{Dynamic, INT};

use crate::script::{self, Callback, Reply};

#[derive(Debug, Default)]
struct Defined(Vec<String>);

fn number(value: usize) -> Dynamic {
    Dynamic::from(INT::try_from(value).unwrap_or(INT::MAX))
}

pub(crate) fn insert(editor: &mut Editor, text: &str) -> Reply {
    let buffer = editor.buffer_mut();
    if buffer.is_read_only() {
        return Err(format!("Buffer is read-only: {}", buffer.name()));
    }
    buffer.insert(text);
    Ok(Dynamic::UNIT)
}

pub(crate) fn text(editor: &Editor) -> Dynamic {
    editor.buffer().contents().into()
}

pub(crate) fn region(editor: &Editor) -> Dynamic {
    let buffer = editor.buffer();
    buffer
        .selection()
        .map(|range| buffer.text_in(range))
        .unwrap_or_default()
        .into()
}

pub(crate) fn point(editor: &Editor) -> Dynamic {
    number(editor.buffer().point())
}

pub(crate) fn goto(editor: &mut Editor, position: INT) -> Dynamic {
    editor
        .buffer_mut()
        .set_point(usize::try_from(position).unwrap_or(0));
    Dynamic::UNIT
}

pub(crate) fn line(editor: &Editor) -> Dynamic {
    let buffer = editor.buffer();
    number(buffer.line_of(buffer.point()) + 1)
}

pub(crate) fn message(editor: &mut Editor, text: String) -> Dynamic {
    editor.show_message(text);
    Dynamic::UNIT
}

pub(crate) fn run(editor: &mut Editor, name: &str) -> Reply {
    let Some(command) = editor.command_named(name) else {
        return Err(format!("No command named {name}"));
    };
    editor.execute(command);
    Ok(Dynamic::UNIT)
}

pub(crate) fn command(
    editor: &mut Editor,
    name: String,
    summary: String,
    callback: Callback,
) -> Reply {
    let ours = editor
        .applet_state::<Defined>()
        .is_some_and(|defined| defined.0.contains(&name));
    if !ours && editor.command_named(&name).is_some() {
        return Err(format!("{name} is already a command"));
    }
    editor.add_dynamic_command(DynamicSpec {
        name: name.clone(),
        summary,
        scope: Scope::Global.into(),
        run: Arc::new(move |editor: &mut Editor| script::call_back(editor, &callback, ())),
    });
    if !ours {
        let mut defined = editor.take_applet_state::<Defined>().unwrap_or_default();
        defined.0.push(name);
        editor.set_applet_state(defined);
    }
    Ok(Dynamic::UNIT)
}

pub(crate) fn bind(editor: &mut Editor, keys: &str, name: &str) -> Reply {
    let Some(command) = editor.command_named(name) else {
        return Err(format!("No command named {name}"));
    };
    editor
        .bind(Scope::Global, keys, command)
        .map_err(|error| format!("{keys}: {error}"))?;
    Ok(Dynamic::UNIT)
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use crate::script::{Origin, describe, evaluate};
    use crate::testing::{m_x, press};

    fn evaluated(editor: &mut Editor, code: &str) -> String {
        match evaluate(editor, code) {
            Ok(value) => format!("{value:?}"),
            Err(error) => describe(*error, Origin::default()),
        }
    }

    fn holding(text: &str) -> Editor {
        let mut editor = Editor::new();
        editor.buffer_mut().insert(text);
        editor
    }

    #[test]
    fn scripts_insert_at_point_and_read_the_whole_text() {
        let mut editor = holding("one ");

        assert_eq!(
            evaluated(&mut editor, "insert(\"two\"); text()"),
            "\"one two\""
        );
        assert_eq!(editor.buffer().contents(), "one two");
    }

    #[test]
    fn scripts_read_point_and_line_and_move_point() {
        let mut editor = holding("ab\ncd");

        assert_eq!(evaluated(&mut editor, "[point(), line()]"), "[5, 2]");
        assert_eq!(
            evaluated(&mut editor, "goto(1); [point(), line()]"),
            "[1, 1]"
        );
        assert_eq!(evaluated(&mut editor, "goto(-4); point()"), "0");
        assert_eq!(evaluated(&mut editor, "goto(99); point()"), "5");
    }

    #[test]
    fn region_is_the_selected_text_or_nothing() {
        let mut editor = holding("hello world");
        editor.buffer_mut().set_mark(0);
        editor.buffer_mut().set_point(5);

        assert_eq!(evaluated(&mut editor, "region()"), "\"hello\"");
        editor.buffer_mut().deactivate_mark();
        assert_eq!(evaluated(&mut editor, "region()"), "\"\"");
    }

    #[test]
    fn message_shows_text_in_the_echo_area() {
        let mut editor = holding("");

        assert_eq!(evaluated(&mut editor, "message(\"hello\")"), "()");
        assert_eq!(editor.message(), Some("hello"));
    }

    #[test]
    fn run_runs_any_command_by_name() {
        let mut editor = holding("ab\ncd");

        assert_eq!(
            evaluated(&mut editor, "run(\"beginning-of-buffer\"); point()"),
            "0"
        );
    }

    #[test]
    fn running_an_unknown_command_is_a_script_error() {
        let mut editor = holding("");

        assert_eq!(
            evaluated(&mut editor, "run(\"no-such-command\")"),
            "No command named no-such-command (line 1, column 1)"
        );
    }

    #[test]
    fn scripts_add_commands_that_m_x_runs() {
        let mut editor = holding("");

        assert_eq!(
            evaluated(
                &mut editor,
                "command(\"say-hi\", \"Say hi\", || message(\"hi\"))"
            ),
            "()"
        );
        m_x(&mut editor, "say-hi");

        assert_eq!(editor.message(), Some("hi"));
    }

    #[test]
    fn scripts_bind_keys_to_their_own_commands_or_any_other() {
        let mut editor = holding("ab");

        evaluated(
            &mut editor,
            "command(\"shout\", \"Shout\", || insert(\"!\"));
             bind(\"C-c x\", \"shout\");
             bind(\"C-c a\", \"beginning-of-buffer\");",
        );
        press(&mut editor, "C-c x");
        assert_eq!(editor.buffer().contents(), "ab!");
        press(&mut editor, "C-c a");

        assert_eq!(editor.buffer().point(), 0);
    }

    #[test]
    fn script_commands_replace_their_own_but_not_other_commands() {
        let mut editor = holding("");

        assert_eq!(
            evaluated(&mut editor, "command(\"next-line\", \"Oops\", || 1)"),
            "next-line is already a command (line 1, column 1)"
        );
        evaluated(
            &mut editor,
            "command(\"greet\", \"Greet\", || message(\"a\"))",
        );
        evaluated(
            &mut editor,
            "command(\"greet\", \"Greet\", || message(\"b\"))",
        );
        m_x(&mut editor, "greet");

        assert_eq!(editor.message(), Some("b"));
    }

    #[test]
    fn bind_reports_unknown_commands_and_bad_keys() {
        let mut editor = holding("");

        assert_eq!(
            evaluated(&mut editor, "bind(\"C-c q\", \"nope\")"),
            "No command named nope (line 1, column 1)"
        );
        assert_eq!(
            evaluated(&mut editor, "bind(\"Q-x\", \"next-line\")"),
            "Q-x: unknown modifier `Q-` (line 1, column 1)"
        );
    }

    #[test]
    fn errors_in_script_commands_show_in_the_echo_area() {
        let mut editor = holding("");
        evaluated(&mut editor, "command(\"broken\", \"Break\", || nope())");

        m_x(&mut editor, "broken");

        let message = editor.message().unwrap_or_default();
        assert!(message.starts_with("Function not found: nope"), "{message}");
    }

    #[test]
    fn read_only_buffers_refuse_inserts() {
        let mut editor = holding("fixed");
        editor.buffer_mut().set_read_only(true);

        assert_eq!(
            evaluated(&mut editor, "insert(\"x\")"),
            "Buffer is read-only: *scratch* (line 1, column 1)"
        );
        assert_eq!(editor.buffer().contents(), "fixed");
    }
}
