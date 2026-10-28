use std::ops::Range;
use std::sync::Arc;

use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Axis, Editor};
use oxmacs_core::text::{Buffer, BufferId, Marker};

use crate::api::handles::{current_buffer, current_buffer_mut, target};

#[derive(Debug)]
struct Placed {
    buffer: BufferId,
    marker: Marker,
}

fn int(number: usize) -> Value {
    Value::Int(i64::try_from(number).unwrap_or(i64::MAX))
}

fn position(heap: &Heap, native: &str, value: Value) -> Result<usize, Error> {
    match value {
        Value::Int(number) => usize::try_from(number)
            .map_err(|_| Error::new(format!("{native} expects positions from 0, got {number}"))),
        _ => Err(Error::new(format!(
            "{native} expects a position, got {}",
            print::brief(heap, value)
        ))),
    }
}

fn text(heap: &mut Heap, text: &str) -> Result<Value, Error> {
    heap.string(text).map_err(Error::from)
}

fn writable<'a>(heap: &Heap, editor: &'a mut Editor) -> Result<&'a mut Buffer, Error> {
    let buffer = current_buffer_mut(heap, editor)?;
    if buffer.is_read_only() {
        return Err(Error::new(format!(
            "Buffer is read-only: {}",
            buffer.name()
        )));
    }
    Ok(buffer)
}

fn placed(heap: &Heap, native: &str, value: Value) -> Result<(BufferId, Marker), Error> {
    heap.foreign_of::<Placed>(value)
        .map(|found| (found.buffer, found.marker))
        .ok_or_else(|| {
            Error::new(format!(
                "{native} expects a marker, got {}",
                print::brief(heap, value)
            ))
        })
}

fn insert(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let added = print::display(heap, arguments[0]);
    writable(heap, editor)?.insert(&added);
    Ok(Value::Nil)
}

fn delete(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let start = position(heap, "delete", arguments[0])?;
    let end = position(heap, "delete", arguments[1])?;
    let buffer = writable(heap, editor)?;
    let last = buffer.len_chars();
    let removed = buffer.delete_range(start.min(end).min(last)..start.max(end).min(last));
    text(heap, &removed)
}

fn between(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let start = position(heap, "text-between", arguments[0])?;
    let end = position(heap, "text-between", arguments[1])?;
    let buffer = current_buffer(heap, editor)?;
    let last = buffer.len_chars();
    let found = buffer.text_in(start.min(end).min(last)..start.max(end).min(last));
    text(heap, &found)
}

fn region(heap: &mut Heap, editor: &mut Editor, _: &[Value]) -> Result<Value, Error> {
    match current_buffer(heap, editor)?.selection() {
        Some(range) => heap
            .vector_from(&[int(range.start), int(range.end)])
            .map_err(Error::from),
        None => Ok(Value::Nil),
    }
}

fn line_native(isolate: &mut Isolate, name: &'static str, find: fn(&Buffer, usize) -> usize) {
    isolate
        .native("ox", name)
        .arity(1, Some(1))
        .host::<Editor>(move |heap, editor, arguments| {
            let at = position(heap, name, arguments[0])?;
            Ok(int(find(current_buffer(heap, editor)?, at)))
        });
}

fn marker(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let at = position(heap, "marker", arguments[0])?;
    let buffer = current_buffer_mut(heap, editor)?;
    let placed = Placed {
        buffer: buffer.id(),
        marker: buffer.add_marker(at),
    };
    heap.shared("marker", Arc::new(placed)).map_err(Error::from)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "point")
        .arity(0, Some(0))
        .host::<Editor>(|heap, editor, _| Ok(int(current_buffer(heap, editor)?.point())));
    isolate
        .native("ox", "set-point")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            let at = position(heap, "set-point", arguments[0])?;
            current_buffer_mut(heap, editor)?.set_point(at);
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "buffer-size")
        .arity(0, Some(0))
        .host::<Editor>(|heap, editor, _| Ok(int(current_buffer(heap, editor)?.len_chars())));
    isolate
        .native("ox", "insert")
        .arity(1, Some(1))
        .host::<Editor>(insert);
    isolate
        .native("ox", "delete")
        .arity(2, Some(2))
        .host::<Editor>(delete);
    isolate
        .native("ox", "text-between")
        .arity(2, Some(2))
        .host::<Editor>(between);
    isolate
        .native("ox", "region")
        .arity(0, Some(0))
        .host::<Editor>(region);
    line_native(isolate, "line-start", Buffer::line_start);
    line_native(isolate, "line-end", Buffer::line_end);
    line_native(isolate, "line-of", Buffer::line_of);
    isolate
        .native("ox", "marker")
        .arity(1, Some(1))
        .host::<Editor>(marker);
    isolate
        .native("ox", "marker?")
        .arity(1, Some(1))
        .host::<Editor>(|heap, _, arguments| {
            Ok(Value::Bool(
                heap.foreign_of::<Placed>(arguments[0]).is_some(),
            ))
        });
    isolate
        .native("ox", "marker-position")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            let (id, marker) = placed(heap, "marker-position", arguments[0])?;
            Ok(editor
                .buffer_by_id(id)
                .and_then(|buffer| buffer.marker(marker))
                .map_or(Value::Nil, int))
        });
    isolate
        .native("ox", "move-marker")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            let (id, marker) = placed(heap, "move-marker", arguments[0])?;
            let at = position(heap, "move-marker", arguments[1])?;
            if let Some(buffer) = editor.buffer_by_id_mut(id) {
                buffer.set_marker(marker, at);
            }
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "marker-buffer")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            let (id, _) = placed(heap, "marker-buffer", arguments[0])?;
            if editor.buffer_by_id(id).is_none() {
                return Ok(Value::Nil);
            }
            crate::api::handles::handle(heap, editor, id)
        });
    install_syntax(isolate);
    install_places(isolate);
}

fn range(heap: &mut Heap, found: Option<Range<usize>>) -> Result<Value, Error> {
    match found {
        Some(found) => heap
            .vector_from(&[int(found.start), int(found.end)])
            .map_err(Error::from),
        None => Ok(Value::Nil),
    }
}

fn display(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let id = target(heap, editor, "display-buffer", arguments[0])?;
    let [below, right_of, select] = [arguments[1], arguments[2], arguments[3]];
    let mut near = None;
    for (beside, axis) in [(below, Axis::Rows), (right_of, Axis::Columns)] {
        if beside != Value::Nil {
            near = target(heap, editor, "display-buffer", beside)
                .ok()
                .map(|other| (other, axis));
        }
    }
    let select = select.truthy();
    editor.display_buffer(id, near, select);
    Ok(Value::Nil)
}

fn header(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let id = target(heap, editor, "set-header", arguments[0])?;
    let lines = crate::api::lines::lines(heap, "set-header", arguments[1])?
        .iter()
        .map(|line| (line.text(), line.style()))
        .collect();
    editor.set_header_lines(id, lines);
    Ok(Value::Nil)
}

fn show_text(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = crate::api::handles::name(heap, editor, "show-text", arguments[0])?;
    let shown = print::display(heap, arguments[1]);
    let language = match arguments[2] {
        Value::Nil => None,
        given => {
            let mode = print::display(heap, given);
            Some(
                editor
                    .language_named(&mode)
                    .ok_or_else(|| Error::new(format!("There is no mode called {mode}")))?,
            )
        }
    };
    let read_only = arguments[3].truthy();
    editor
        .show_text(&name, &shown, language, read_only)
        .map_err(Error::new)?;
    Ok(Value::Nil)
}

fn complete(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let start = position(heap, "complete-at", arguments[0])?;
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

fn install_syntax(isolate: &mut Isolate) {
    isolate
        .native("ox", "statement-at")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            let at = position(heap, "statement-at", arguments[0])?;
            let found = editor.statement_at(at);
            range(heap, found)
        });
    isolate
        .native("ox", "statement-after")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            let at = position(heap, "statement-after", arguments[0])?;
            Ok(editor.statement_after(at).map_or(Value::Nil, int))
        });
    isolate
        .native("ox", "defun-around")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            let at = position(heap, "defun-around", arguments[0])?;
            let found = editor.defun_around(at);
            range(heap, found)
        });
    isolate
        .native("ox", "buffer-language")
        .arity(0, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            let id = match arguments.first() {
                Some(buffer) => target(heap, editor, "buffer-language", *buffer).ok(),
                None => Some(crate::api::handles::current(heap, editor)?),
            };
            match id.and_then(|id| editor.buffer_language(id)) {
                Some(language) => text(heap, language.name()),
                None => Ok(Value::Nil),
            }
        });
    isolate
        .native("ox", "buffer-list")
        .arity(0, Some(0))
        .host::<Editor>(|heap, editor, _| {
            let names: Vec<String> = editor
                .buffer_names()
                .into_iter()
                .map(str::to_owned)
                .collect();
            let mut values = Vec::with_capacity(names.len());
            for name in names {
                values.push(text(heap, &name)?);
            }
            heap.vector_from(&values).map_err(Error::from)
        });
}

fn install_places(isolate: &mut Isolate) {
    isolate
        .native("ox", "buffer-contents")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            let found = target(heap, editor, "buffer-contents", arguments[0])
                .ok()
                .and_then(|id| editor.buffer_by_id(id))
                .map(Buffer::contents);
            match found {
                Some(contents) => text(heap, &contents),
                None => Ok(Value::Nil),
            }
        });
    isolate
        .native("ox", "display-buffer")
        .arity(1, Some(1))
        .keywords(&["below", "right-of", "select"])
        .host::<Editor>(display);
    isolate
        .native("ox", "set-header")
        .arity(2, Some(2))
        .host::<Editor>(header);
    isolate
        .native("ox", "show-text")
        .arity(2, Some(2))
        .keywords(&["mode", "read-only"])
        .host::<Editor>(show_text);
    isolate
        .native("ox", "prompting?")
        .arity(0, Some(0))
        .host::<Editor>(|_, editor, _| Ok(Value::Bool(editor.prompting())));
    isolate
        .native("ox", "complete-at")
        .arity(2, Some(3))
        .host::<Editor>(complete);
    isolate
        .native("ox", "completion-shown?")
        .arity(0, Some(0))
        .host::<Editor>(|_, editor, _| Ok(Value::Bool(editor.pane().popup().is_some())));
    isolate
        .native("ox", "remember-location")
        .arity(0, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            let location = match arguments {
                [buffer, at] => (
                    target(heap, editor, "remember-location", *buffer)?,
                    position(heap, "remember-location", *at)?,
                ),
                _ => editor.current_location(),
            };
            editor.remember_location(location);
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "just-typed?")
        .arity(0, Some(0))
        .host::<Editor>(|_, editor, _| Ok(Value::Bool(editor.just_typed())));
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

    fn ask(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
        ox.eval(editor, source).unwrap()
    }

    #[test]
    fn plugins_insert_delete_and_move_point_by_character_offsets() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        ask(&mut editor, &mut ox, r#"(insert "hello world")"#);
        assert_eq!(ask(&mut editor, &mut ox, "(point)"), "11");
        ask(&mut editor, &mut ox, r#"(set-point 5) (insert ",")"#);
        assert_eq!(editor.buffer().contents(), "hello, world");
        assert_eq!(ask(&mut editor, &mut ox, "(delete 0 5)"), r#""hello""#);
        assert_eq!(
            ask(
                &mut editor,
                &mut ox,
                "[(text-between 2 7) (buffer-size) (point)]"
            ),
            r#"["world" 7 1]"#
        );
        assert_eq!(ask(&mut editor, &mut ox, "(set-point 99) (point)"), "7");
        assert_eq!(
            ox.eval(&mut editor, "(delete -1 2)").unwrap_err().message,
            "delete expects positions from 0, got -1"
        );
    }

    #[test]
    fn the_region_and_lines_are_offsets_too() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ask(&mut editor, &mut ox, r#"(insert "ab\ncd\n")"#);

        assert_eq!(ask(&mut editor, &mut ox, "(region)"), "nil");
        editor.buffer_mut().set_mark(1);
        editor.buffer_mut().set_point(4);

        assert_eq!(ask(&mut editor, &mut ox, "(region)"), "[1 4]");
        assert_eq!(
            ask(
                &mut editor,
                &mut ox,
                "[(line-start 1) (line-end 1) (line-of 4) (line-of 0)]"
            ),
            "[3 5 1 0]"
        );
    }

    #[test]
    fn markers_follow_the_text_around_them() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ask(
            &mut editor,
            &mut ox,
            r#"(insert "abcdef") (def m (marker 3)) (set-point 0) (insert "xx")"#,
        );

        assert_eq!(ask(&mut editor, &mut ox, "(marker-position m)"), "5");
        ask(&mut editor, &mut ox, "(delete 0 4)");
        assert_eq!(ask(&mut editor, &mut ox, "(marker-position m)"), "1");
        ask(&mut editor, &mut ox, "(move-marker m 0)");
        assert_eq!(ask(&mut editor, &mut ox, "(marker-position m)"), "0");
        assert_eq!(
            ask(&mut editor, &mut ox, "[(marker? m) (marker? 1)]"),
            "[true false]"
        );
    }

    #[test]
    fn markers_keep_their_buffer_and_read_only_buffers_refuse_edits() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ask(
            &mut editor,
            &mut ox,
            r#"(def home (current-buffer)) (open-buffer "*other*" ["one" "two"]) (def other (current-buffer)) (switch-to-buffer home) (def far (with-buffer other (marker 4)))"#,
        );

        assert_eq!(
            ask(
                &mut editor,
                &mut ox,
                "[(marker-position far) (= (marker-buffer far) other)]"
            ),
            "[4 true]"
        );
        assert_eq!(
            ox.eval(&mut editor, r#"(with-buffer other (insert "x"))"#)
                .unwrap_err()
                .message,
            "Buffer is read-only: *other*"
        );
        ask(&mut editor, &mut ox, "(close-buffer other)");
        assert_eq!(ask(&mut editor, &mut ox, "(marker-position far)"), "nil");
    }

    #[test]
    fn syntax_finds_statements_and_definitions() {
        let (mut editor, mut ox) = editing("x <- 1\n\nf <- function() {\n  1\n}\n", 0);
        editor.set_major_mode(Some(Language::r()));

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
                .contains("There is no buffer named *c*")
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
    fn text_is_shown_in_a_buffer_of_its_own_mode() {
        let (mut editor, mut ox) = editing("x", 0);

        eval(
            &mut editor,
            &mut ox,
            r#"(show-text "*source*" "f <- function() 1" :mode "R" :read-only true)"#,
        );
        assert_eq!(editor.buffer().name(), "*source*");
        assert_eq!(editor.buffer().contents(), "f <- function() 1");
        assert!(editor.buffer().is_read_only());
        assert!(!editor.buffer().is_modified());
        assert_eq!(eval(&mut editor, &mut ox, "(buffer-language)"), "\"R\"");

        eval(&mut editor, &mut ox, r#"(show-text "*source*" "again")"#);
        assert_eq!(editor.buffer().contents(), "again");
        assert_eq!(editor.buffer().point(), 0);
        assert!(
            eval(
                &mut editor,
                &mut ox,
                r#"(show-text "*x*" "" :mode "Cobol")"#
            )
            .contains("There is no mode called Cobol")
        );
    }

    #[test]
    fn plugins_know_when_a_prompt_is_open() {
        let (mut editor, mut ox) = editing("x", 0);
        assert_eq!(eval(&mut editor, &mut ox, "(prompting?)"), "false");

        press(&mut editor, &mut ox, "M-x");

        assert_eq!(eval(&mut editor, &mut ox, "(prompting?)"), "true");
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
    fn plugins_know_when_a_character_was_just_typed() {
        let (mut editor, mut ox) = editing("", 0);

        typed(&mut editor, &mut ox, "ab");
        assert_eq!(eval(&mut editor, &mut ox, "(just-typed?)"), "true");
        press(&mut editor, &mut ox, "C-b");
        settle(&mut editor, &mut ox);

        assert_eq!(eval(&mut editor, &mut ox, "(just-typed?)"), "false");
    }
}
