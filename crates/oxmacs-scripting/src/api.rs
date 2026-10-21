use oxmacs_core::editor::Editor;
use rhai::{Dynamic, INT};

use crate::script::Reply;

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

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use crate::script::{Origin, describe, evaluate};

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
