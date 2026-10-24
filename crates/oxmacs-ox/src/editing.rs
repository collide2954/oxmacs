use std::ops::Range;

use ox::heap::{Heap, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::{Axis, Editor};
use oxmacs_core::text::BufferId;

use crate::log;
use crate::views::face_named;

fn made(made: Result<Value, ox::heap::HeapFull>) -> Result<Value, Error> {
    made.map_err(|full| Error::new(full.to_string()))
}

fn int(number: usize) -> Value {
    Value::Int(i64::try_from(number).unwrap_or(i64::MAX))
}

fn offset(heap: &Heap, native: &str, value: Value) -> Result<usize, Error> {
    match value {
        Value::Int(number) => Ok(usize::try_from(number).unwrap_or(0)),
        _ => Err(Error::new(format!(
            "{native} expects a position, got {}",
            print::brief(heap, value)
        ))),
    }
}

fn range(heap: &mut Heap, found: Option<Range<usize>>) -> Result<Value, Error> {
    match found {
        Some(found) => made(heap.vector_from(&[int(found.start), int(found.end)])),
        None => Ok(Value::Nil),
    }
}

fn named(heap: &Heap, editor: &Editor, native: &str, value: Value) -> Result<BufferId, Error> {
    let name = print::display(heap, value);
    editor
        .buffer_named(&name)
        .ok_or_else(|| Error::new(format!("{native}: there is no buffer named {name}")))
}

fn display(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let id = named(heap, editor, "display-buffer", arguments[0])?;
    let mut near = None;
    let mut select = false;
    for pair in arguments[1..].chunks(2) {
        match pair {
            [Value::Keyword(option), value] if option.name() == "select" => {
                select = value.truthy();
            }
            [Value::Keyword(option), value]
                if matches!(option.name(), "below" | "right-of") && *value != Value::Nil =>
            {
                let axis = if option.name() == "below" {
                    Axis::Rows
                } else {
                    Axis::Columns
                };
                near = editor
                    .buffer_named(&print::display(heap, *value))
                    .map(|other| (other, axis));
            }
            [Value::Keyword(option), Value::Nil]
                if matches!(option.name(), "below" | "right-of") => {}
            _ => {
                return Err(Error::new(format!(
                    "display-buffer takes :below, :right-of and :select, not {}",
                    print::brief(heap, pair[0])
                )));
            }
        }
    }
    editor.display_buffer(id, near, select);
    Ok(Value::Nil)
}

fn header_line(heap: &Heap, value: Value) -> Result<(String, LineStyle), Error> {
    let wrong = || {
        Error::new(format!(
            "set-header expects lines of text and [text face] pieces, got {}",
            print::brief(heap, value)
        ))
    };
    let pieces = match heap.text(value) {
        Some(_) => vec![value],
        None => heap.elements(value).ok_or_else(wrong)?,
    };
    let mut text = String::new();
    let mut style = LineStyle::default();
    for piece in pieces {
        let (shown, face) = match heap.text(piece) {
            Some(shown) => (shown.to_owned(), None),
            None => match heap.elements(piece).as_deref() {
                Some([shown, Value::Keyword(face)]) => (
                    print::display(heap, *shown),
                    Some(face_named(face.name()).ok_or_else(wrong)?),
                ),
                _ => return Err(wrong()),
            },
        };
        let start = text.chars().count();
        text.push_str(&shown);
        if let Some(face) = face {
            style.spans.push((start..text.chars().count(), face));
        }
    }
    Ok((text, style))
}

fn header(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let id = named(heap, editor, "set-header", arguments[0])?;
    let lines = heap
        .elements(arguments[1])
        .filter(|_| heap.text(arguments[1]).is_none())
        .ok_or_else(|| {
            Error::new(format!(
                "set-header expects a vector of lines, got {}",
                print::brief(heap, arguments[1])
            ))
        })?
        .into_iter()
        .map(|line| header_line(heap, line))
        .collect::<Result<Vec<_>, Error>>()?;
    editor.set_header_lines(id, lines);
    Ok(Value::Nil)
}

fn complete(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let start = offset(heap, "complete-at", arguments[0])?;
    let candidates = heap
        .elements(arguments[1])
        .filter(|_| heap.text(arguments[1]).is_none())
        .ok_or_else(|| {
            Error::new(format!(
                "complete-at expects a vector of candidates, got {}",
                print::brief(heap, arguments[1])
            ))
        })?
        .into_iter()
        .map(|candidate| print::display(heap, candidate))
        .collect();
    let explicit = arguments.get(2).is_some_and(|value| value.truthy());
    editor.complete_at(start, candidates, explicit);
    Ok(Value::Nil)
}

#[derive(Debug, Default)]
struct Typing {
    handlers: Vec<(String, Hold)>,
    seen: Option<(BufferId, u64)>,
}

fn on_typed(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) {
    let key = print::display(heap, arguments[0]);
    let hold = heap.hold(arguments[1]);
    let mut typing = editor.take_plugin_state::<Typing>().unwrap_or_default();
    match typing.handlers.iter_mut().find(|(known, _)| *known == key) {
        Some((_, existing)) => {
            heap.release(std::mem::replace(existing, hold));
        }
        None => typing.handlers.push((key, hold)),
    }
    editor.set_plugin_state(typing);
}

pub fn typed(isolate: &mut Isolate, editor: &mut Editor) {
    if !editor.just_typed() || editor.minibuffer().is_some() {
        return;
    }
    let buffer = editor.buffer();
    let key = (buffer.id(), buffer.revision());
    let Some(typing) = editor.plugin_state_mut::<Typing>() else {
        return;
    };
    if typing.seen == Some(key) {
        return;
    }
    typing.seen = Some(key);
    let handlers: Vec<Hold> = typing.handlers.iter().map(|(_, hold)| *hold).collect();
    for hold in handlers {
        if let Some(handler) = isolate.held(hold)
            && let Err(error) = isolate.spawn(handler, &[])
        {
            log::report(editor, &error, None);
        }
    }
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("point", 0, Some(0), |_, editor, _| {
        Ok(int(editor.buffer().point()))
    });
    isolate.define_host_native::<Editor>("set-point", 1, Some(1), |heap, editor, arguments| {
        let at = offset(heap, "set-point", arguments[0])?;
        let buffer = editor.buffer_mut();
        buffer.set_point(at.min(buffer.len_chars()));
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("insert", 1, Some(1), |heap, editor, arguments| {
        let text = print::display(heap, arguments[0]);
        Ok(Value::Bool(editor.insert_text(&text)))
    });
    isolate.define_host_native::<Editor>("text-range", 2, Some(2), |heap, editor, arguments| {
        let start = offset(heap, "text-range", arguments[0])?;
        let end = offset(heap, "text-range", arguments[1])?;
        let buffer = editor.buffer();
        let length = buffer.len_chars();
        let text = buffer.text_in(start.min(length)..end.clamp(start.min(length), length));
        made(heap.string(&text))
    });
    isolate.define_host_native::<Editor>("region", 0, Some(0), |heap, editor, _| {
        let selection = editor.buffer().selection();
        range(heap, selection)
    });
    isolate.define_host_native::<Editor>("statement-at", 1, Some(1), |heap, editor, arguments| {
        let at = offset(heap, "statement-at", arguments[0])?;
        let found = editor.statement_at(at);
        range(heap, found)
    });
    isolate.define_host_native::<Editor>(
        "statement-after",
        1,
        Some(1),
        |heap, editor, arguments| {
            let at = offset(heap, "statement-after", arguments[0])?;
            Ok(editor.statement_after(at).map_or(Value::Nil, int))
        },
    );
    isolate.define_host_native::<Editor>("defun-around", 1, Some(1), |heap, editor, arguments| {
        let at = offset(heap, "defun-around", arguments[0])?;
        let found = editor.defun_around(at);
        range(heap, found)
    });
    isolate.define_host_native::<Editor>(
        "buffer-language",
        0,
        Some(1),
        |heap, editor, arguments| {
            let id = match arguments.first() {
                Some(name) => editor.buffer_named(&print::display(heap, *name)),
                None => Some(editor.buffer().id()),
            };
            match id.and_then(|id| editor.buffer_language(id)) {
                Some(language) => made(heap.string(language.name())),
                None => Ok(Value::Nil),
            }
        },
    );
    isolate.define_host_native::<Editor>("buffer-list", 0, Some(0), |heap, editor, _| {
        let names: Vec<String> = editor
            .buffer_names()
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mut values = Vec::with_capacity(names.len());
        for name in names {
            values.push(made(heap.string(&name))?);
        }
        made(heap.vector_from(&values))
    });
    install_places(isolate);
}

fn install_places(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>(
        "buffer-contents",
        1,
        Some(1),
        |heap, editor, arguments| {
            let name = print::display(heap, arguments[0]);
            match editor
                .buffer_named(&name)
                .and_then(|id| editor.buffer_by_id(id))
            {
                Some(buffer) => made(heap.string(&buffer.contents())),
                None => Ok(Value::Nil),
            }
        },
    );
    isolate.define_host_native::<Editor>("display-buffer", 1, None, display);
    isolate.define_host_native::<Editor>("set-header", 2, Some(2), header);
    isolate.define_host_native::<Editor>("complete-at", 2, Some(3), complete);
    isolate.define_host_native::<Editor>("completion-shown?", 0, Some(0), |_, editor, _| {
        Ok(Value::Bool(editor.pane().popup().is_some()))
    });
    isolate.define_host_native::<Editor>(
        "remember-location",
        0,
        Some(2),
        |heap, editor, arguments| {
            let location = match arguments {
                [name, at] => {
                    let id = named(heap, editor, "remember-location", *name)?;
                    (id, offset(heap, "remember-location", *at)?)
                }
                _ => editor.current_location(),
            };
            editor.remember_location(location);
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native::<Editor>("on-typed", 2, Some(2), |heap, editor, arguments| {
        on_typed(heap, editor, arguments);
        Ok(Value::Nil)
    });
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::syntax::Language;
    use oxmacs_core::text::Buffer;

    use crate::Ox;
    use crate::testing::{press, settle, typed};

    fn editing(text: &str, point: usize) -> (Editor, Ox) {
        let mut buffer = Buffer::with_text("t", text);
        buffer.set_point(point);
        let editor = Editor::with_buffer(buffer);
        (editor, Ox::new())
    }

    fn eval(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
        match ox.eval(editor, source) {
            Ok(shown) => shown,
            Err(error) => format!("error: {}", error.message),
        }
    }

    #[test]
    fn plugins_read_and_move_point_and_insert_text() {
        let (mut editor, mut ox) = editing("hello world", 5);

        assert_eq!(eval(&mut editor, &mut ox, "(point)"), "5");
        assert_eq!(
            eval(
                &mut editor,
                &mut ox,
                "[(text-range 0 5) (text-range 6 99) (text-range 9 3)]"
            ),
            r#"["hello" "world" ""]"#
        );
        assert_eq!(eval(&mut editor, &mut ox, r#"(insert ",")"#), "true");
        assert_eq!(editor.buffer().contents(), "hello, world");
        eval(&mut editor, &mut ox, "(set-point 99)");
        assert_eq!(editor.buffer().point(), 12);
        assert_eq!(eval(&mut editor, &mut ox, "(region)"), "nil");
        press(&mut editor, &mut ox, "C-SPC C-a");
        assert_eq!(eval(&mut editor, &mut ox, "(region)"), "[0 12]");

        press(&mut editor, &mut ox, "C-x C-q");
        assert_eq!(eval(&mut editor, &mut ox, r#"(insert "!")"#), "false");
        assert_eq!(editor.message(), Some("Buffer is read-only: t"));
    }

    #[test]
    fn syntax_finds_statements_and_definitions() {
        let (mut editor, mut ox) = editing("x <- 1\n\nf <- function() {\n  1\n}\n", 0);
        editor.set_major_mode(Some(Language::R));

        assert_eq!(
            eval(
                &mut editor,
                &mut ox,
                "[(statement-at 2) (statement-after 6) (defun-around 20) (defun-around 1)]"
            ),
            "[[0 6] 8 [8 31] nil]"
        );
        assert_eq!(
            eval(
                &mut editor,
                &mut ox,
                "[(buffer-language) (buffer-language \"*scratch*\")]"
            ),
            "[\"R\" nil]"
        );
    }

    #[test]
    fn plugins_list_buffers_and_read_any_of_them() {
        let (mut editor, mut ox) = editing("one", 0);
        ox.eval(&mut editor, r#"(open-buffer "*other*" ["two"])"#)
            .unwrap();

        assert_eq!(
            eval(
                &mut editor,
                &mut ox,
                r#"[(sort (buffer-list)) (buffer-contents "*other*") (buffer-contents "*none*")]"#
            ),
            r#"[["*other*" "t"] "two" nil]"#
        );
    }

    #[test]
    fn buffers_are_displayed_where_plugins_ask() {
        let (mut editor, mut ox) = editing("one", 0);
        ox.eval(
            &mut editor,
            r#"(open-buffer "*a*" [] :show false) (open-buffer "*b*" [] :show false)"#,
        )
        .unwrap();

        eval(&mut editor, &mut ox, r#"(display-buffer "*a*")"#);
        eval(
            &mut editor,
            &mut ox,
            r#"(display-buffer "*b*" :below "*a*" :select true)"#,
        );

        assert_eq!(editor.buffer().name(), "*b*");
        assert_eq!(editor.tiling().windows().len(), 3);
        assert!(
            eval(&mut editor, &mut ox, r#"(display-buffer "*c*")"#)
                .contains("there is no buffer named *c*")
        );
    }

    #[test]
    fn completions_are_offered_at_a_start() {
        let (mut editor, mut ox) = editing("pr", 2);

        eval(&mut editor, &mut ox, r#"(complete-at 0 ["print" "prod"])"#);
        assert_eq!(eval(&mut editor, &mut ox, "(completion-shown?)"), "true");
        press(&mut editor, &mut ox, "C-g");

        eval(&mut editor, &mut ox, r#"(complete-at 0 ["print"] true)"#);
        assert_eq!(editor.buffer().contents(), "print");
        eval(&mut editor, &mut ox, "(complete-at 0 [] true)");
        assert_eq!(editor.message(), Some("No completions"));
    }

    #[test]
    fn buffers_keep_header_lines_above_their_text() {
        let (mut editor, mut ox) = editing("rows", 0);
        ox.eval(&mut editor, r#"(open-buffer "*table*" ["row"])"#)
            .unwrap();

        eval(
            &mut editor,
            &mut ox,
            r#"(set-header "*table*" [[["name" :heading] " x"] "types"])"#,
        );

        let header = editor.pane().header_lines();
        let texts: Vec<&str> = header.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(texts, ["name x", "types"]);
        assert_eq!(
            header[0].1.spans,
            [(0..4, oxmacs_core::theme::Highlight::Heading)]
        );
        eval(&mut editor, &mut ox, r#"(set-header "*table*" [])"#);
        assert_eq!(editor.pane().header_lines(), []);
    }

    #[test]
    fn locations_are_remembered_for_m_comma() {
        let (mut editor, mut ox) = editing("abc def", 2);

        eval(&mut editor, &mut ox, "(remember-location) (set-point 6)");
        press(&mut editor, &mut ox, "M-,");
        assert_eq!(editor.buffer().point(), 2);

        eval(&mut editor, &mut ox, r#"(remember-location "t" 5)"#);
        press(&mut editor, &mut ox, "M-,");
        assert_eq!(editor.buffer().point(), 5);
    }

    #[test]
    fn typing_calls_its_handlers_once_per_change() {
        let (mut editor, mut ox) = editing("", 0);
        ox.eval(
            &mut editor,
            r"(def seen (atom [])) (on-typed :watch (fn [] (swap! seen conj (point))))",
        )
        .unwrap();

        typed(&mut editor, &mut ox, "ab");
        press(&mut editor, &mut ox, "C-b");
        settle(&mut editor, &mut ox);

        assert_eq!(eval(&mut editor, &mut ox, "@seen"), "[1 2]");
    }
}
