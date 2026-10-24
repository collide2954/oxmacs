use std::sync::Arc;

use ox::heap::Heap;
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::editor::Editor;
use oxmacs_core::text::{Buffer, BufferId, Marker};

use crate::handles::{current_buffer, current_buffer_mut};

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
    heap.string(text)
        .map_err(|full| Error::new(full.to_string()))
}

fn writable<'a>(heap: &Heap, editor: &'a mut Editor) -> Result<&'a mut Buffer, Error> {
    let buffer = current_buffer_mut(heap, editor)?;
    if buffer.is_read_only() {
        return Err(Error::new(format!("{} is read-only", buffer.name())));
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
            .map_err(|full| Error::new(full.to_string())),
        None => Ok(Value::Nil),
    }
}

fn line_native(isolate: &mut Isolate, name: &'static str, find: fn(&Buffer, usize) -> usize) {
    isolate.define_host_native::<Editor>(name, 1, Some(1), move |heap, editor, arguments| {
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
    heap.shared("marker", Arc::new(placed))
        .map_err(|full| Error::new(full.to_string()))
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("point", 0, Some(0), |heap, editor, _| {
        Ok(int(current_buffer(heap, editor)?.point()))
    });
    isolate.define_host_native::<Editor>("set-point", 1, Some(1), |heap, editor, arguments| {
        let at = position(heap, "set-point", arguments[0])?;
        current_buffer_mut(heap, editor)?.set_point(at);
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("buffer-size", 0, Some(0), |heap, editor, _| {
        Ok(int(current_buffer(heap, editor)?.len_chars()))
    });
    isolate.define_host_native::<Editor>("insert", 1, Some(1), insert);
    isolate.define_host_native::<Editor>("delete", 2, Some(2), delete);
    isolate.define_host_native::<Editor>("text-between", 2, Some(2), between);
    isolate.define_host_native::<Editor>("region", 0, Some(0), region);
    line_native(isolate, "line-start", Buffer::line_start);
    line_native(isolate, "line-end", Buffer::line_end);
    line_native(isolate, "line-of", Buffer::line_of);
    isolate.define_host_native::<Editor>("marker", 1, Some(1), marker);
    isolate.define_host_native::<Editor>("marker?", 1, Some(1), |heap, _, arguments| {
        Ok(Value::Bool(
            heap.foreign_of::<Placed>(arguments[0]).is_some(),
        ))
    });
    isolate.define_host_native::<Editor>(
        "marker-position",
        1,
        Some(1),
        |heap, editor, arguments| {
            let (id, marker) = placed(heap, "marker-position", arguments[0])?;
            Ok(editor
                .buffer_by_id(id)
                .and_then(|buffer| buffer.marker(marker))
                .map_or(Value::Nil, int))
        },
    );
    isolate.define_host_native::<Editor>("move-marker", 2, Some(2), |heap, editor, arguments| {
        let (id, marker) = placed(heap, "move-marker", arguments[0])?;
        let at = position(heap, "move-marker", arguments[1])?;
        if let Some(buffer) = editor.buffer_by_id_mut(id) {
            buffer.set_marker(marker, at);
        }
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("marker-buffer", 1, Some(1), |heap, editor, arguments| {
        let (id, _) = placed(heap, "marker-buffer", arguments[0])?;
        if editor.buffer_by_id(id).is_none() {
            return Ok(Value::Nil);
        }
        crate::handles::handle(heap, editor, id)
    });
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use crate::Ox;

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
            "*other* is read-only"
        );
        ask(&mut editor, &mut ox, "(close-buffer other)");
        assert_eq!(ask(&mut editor, &mut ox, "(marker-position far)"), "nil");
    }
}
