use std::iter;

use oxmacs_core::display::display_column;
use oxmacs_core::editor::Editor;
use oxmacs_core::syntax::Syntax;
use oxmacs_core::text::Buffer;

use crate::mode::LANGUAGE;

const UNIT: usize = 4;
const DEDENTERS: &[&str] = &["return", "pass", "break", "continue", "raise"];

pub(crate) fn parsed(text: &str) -> (Buffer, Syntax) {
    let mut buffer = Buffer::with_text("python", text);
    let mut syntax = Syntax::new(LANGUAGE);
    syntax.sync(&mut buffer);
    (buffer, syntax)
}

pub(crate) fn leading(text: &str) -> usize {
    text.chars().take_while(|c| matches!(c, ' ' | '\t')).count()
}

fn indentation(text: &str, tab_width: usize) -> usize {
    display_column(text, leading(text), tab_width)
}

pub(crate) fn code(buffer: &Buffer, syntax: &Syntax, line: usize) -> String {
    let text = buffer.line_text(line);
    let end = syntax
        .comment_start(buffer, line)
        .unwrap_or_else(|| text.chars().count());
    text.chars()
        .take(end)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn starts_with_word(text: &str, word: &str) -> bool {
    text.strip_prefix(word)
        .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
}

fn levels(buffer: &Buffer, syntax: &Syntax, line: usize, tab_width: usize) -> Vec<usize> {
    let previous = (0..line).rev().find_map(|earlier| {
        let code = code(buffer, syntax, earlier);
        (!code.trim().is_empty()).then(|| (indentation(&code, tab_width), code))
    });
    let Some((indent, code)) = previous else {
        return vec![0];
    };
    let first = code.trim_start();
    let guess = if code.ends_with(':') {
        indent + UNIT
    } else if DEDENTERS.iter().any(|word| starts_with_word(first, word)) {
        indent.saturating_sub(UNIT)
    } else {
        indent
    };
    let deepest = indent + UNIT;
    let others = (0..deepest)
        .step_by(UNIT)
        .chain(iter::once(deepest))
        .filter(|level| *level != guess);
    iter::once(guess).chain(others).collect()
}

pub(crate) fn guess(
    syntax: &Syntax,
    buffer: &Buffer,
    line: usize,
    tab_width: usize,
) -> Option<usize> {
    levels(buffer, syntax, line, tab_width).first().copied()
}

fn refused(editor: &mut Editor) -> bool {
    if !editor.buffer().is_read_only() {
        return false;
    }
    let message = format!("Buffer is read-only: {}", editor.buffer().name());
    editor.show_message(message);
    true
}

fn reindent(buffer: &mut Buffer, line: usize, target: usize) {
    let start = buffer.line_start(line);
    let text = buffer.line_text(line).into_owned();
    let leading = leading(&text);
    let wanted = " ".repeat(target);
    let point = buffer.point();
    if text.chars().take(leading).eq(wanted.chars()) {
        if point >= start && point < start + leading {
            buffer.set_point(start + leading);
        }
        return;
    }
    let moved = if point < start {
        point
    } else if point <= start + leading {
        start + target
    } else {
        point - leading + target
    };
    buffer.delete_range(start..start + leading);
    buffer.insert_at(start, &wanted);
    buffer.set_point(moved);
}

fn guesses(editor: &Editor, line: usize) -> Vec<usize> {
    let (buffer, syntax) = parsed(&editor.buffer().contents());
    levels(&buffer, &syntax, line, editor.tab_width())
}

pub(crate) fn indent_line(editor: &mut Editor) {
    if refused(editor) {
        return;
    }
    let buffer = editor.buffer();
    let line = buffer.line_of(buffer.point());
    let current = indentation(&buffer.line_text(line), editor.tab_width());
    let levels = guesses(editor, line);
    let guess = levels.first().copied().unwrap_or(0);
    let target = if editor.repeated() {
        levels
            .iter()
            .position(|level| *level == current)
            .and_then(|at| levels.get((at + 1) % levels.len()))
            .copied()
            .unwrap_or(guess)
    } else {
        guess
    };
    let buffer = editor.buffer_mut();
    buffer.close_undo_step();
    reindent(buffer, line, target);
}

#[cfg(test)]
mod tests {
    use crate::testing::{editor_with, lines, press, type_text};

    fn after(text: &str, point: usize, keys: &str) -> (String, usize) {
        let mut editor = editor_with(text, point);
        press(&mut editor, keys);
        (editor.buffer().contents(), editor.buffer().point())
    }

    #[test]
    fn tab_indents_one_level_deeper_after_a_colon() {
        assert_eq!(
            after("def f():\nx", 9, "TAB"),
            ("def f():\n    x".to_owned(), 13)
        );
        assert_eq!(after("if a:  # why\nb", 13, "TAB").0, "if a:  # why\n    b");
    }

    #[test]
    fn tab_dedents_after_return_pass_break_continue_and_raise() {
        for word in ["return x", "pass", "break", "continue", "raise E"] {
            let text = format!("def f():\n    if a:\n        {word}\ny");
            let point = text.len() - 1;

            let (contents, _) = after(&text, point, "TAB");

            assert!(contents.ends_with("\n    y"), "{word}: {contents:?}");
        }
    }

    #[test]
    fn tab_keeps_the_level_of_the_previous_code_line() {
        let text = "def f():\n    a = 1\n\n# note\n  b";

        let (contents, point) = after(text, text.len(), "TAB");

        assert_eq!(contents, "def f():\n    a = 1\n\n# note\n    b");
        assert_eq!(point, contents.len());
    }

    #[test]
    fn words_that_only_start_like_return_do_not_dedent() {
        let text = "def f():\n    returned = 1\nx";

        assert_eq!(
            after(text, text.len(), "TAB").0,
            "def f():\n    returned = 1\n    x"
        );
    }

    #[test]
    fn the_first_line_has_no_indentation() {
        assert_eq!(after("  x", 3, "TAB"), ("x".to_owned(), 1));
    }

    #[test]
    fn tab_in_the_indentation_moves_to_the_text() {
        let mut editor = editor_with("def f():\n    x", 10);

        press(&mut editor, "TAB");

        assert_eq!(editor.buffer().point(), 13);
        assert!(!editor.buffer().is_modified());
    }

    #[test]
    fn tab_again_cycles_through_the_other_levels() {
        let text = "def f():\n    if a:\n        x = 1\ny";
        let mut editor = editor_with(text, text.len());
        let mut seen = Vec::new();

        for _ in 0..5 {
            press(&mut editor, "TAB");
            seen.push(lines(&editor)[3].clone());
        }

        assert_eq!(
            seen,
            ["        y", "y", "    y", "            y", "        y"]
        );
    }

    #[test]
    fn tab_after_another_command_guesses_again() {
        let text = "def f():\n    if a:\n        x = 1\ny";
        let mut editor = editor_with(text, text.len());

        press(&mut editor, "TAB TAB C-e TAB");

        assert_eq!(lines(&editor)[3], "        y");
    }

    #[test]
    fn return_indents_the_new_line_and_leaves_the_old_one_alone() {
        assert_eq!(
            after("def f():", 8, "RET"),
            ("def f():\n    ".to_owned(), 13)
        );
        assert_eq!(
            after("if a:\n    b", 11, "RET"),
            ("if a:\n    b\n    ".to_owned(), 16)
        );
        assert_eq!(
            after("def f():\n    return 1", 21, "C-j").0,
            "def f():\n    return 1\n"
        );
    }

    #[test]
    fn return_drops_spaces_before_point_and_splits_the_line() {
        assert_eq!(after("x = 1   ", 8, "RET").0, "x = 1\n");
        assert_eq!(after("if a: b", 5, "RET"), ("if a:\n    b".to_owned(), 10));
    }

    #[test]
    fn indenting_the_region_follows_the_guess() {
        let mut editor = editor_with("def f():\nx = 1\nreturn x", 0);

        press(&mut editor, "C-x h C-M-\\");

        assert_eq!(
            editor.buffer().contents(),
            "def f():\n    x = 1\n    return x"
        );
    }

    #[test]
    fn each_tab_undoes_on_its_own() {
        let mut editor = editor_with("def f():\n", 9);
        type_text(&mut editor, "x");

        press(&mut editor, "TAB C-/");

        assert_eq!(editor.buffer().contents(), "def f():\nx");
    }

    #[test]
    fn read_only_buffers_refuse_tab_and_return() {
        for keys in ["TAB", "RET"] {
            let mut editor = editor_with("def f():\nx", 10);
            press(&mut editor, "C-x C-q");

            press(&mut editor, keys);

            assert_eq!(editor.buffer().contents(), "def f():\nx", "{keys}");
            assert_eq!(
                editor.message(),
                Some("Buffer is read-only: t.py"),
                "{keys}"
            );
        }
    }
}
