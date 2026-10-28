use std::collections::HashMap;
use std::sync::Arc;

use ox_lang::heap::{Heap, Hold};
use ox_lang::native::expected;
use ox_lang::task::TaskId;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Editor, EventKind};
use oxmacs_core::text::{Buffer, BufferId};

#[derive(Debug)]
struct Handle(BufferId);

#[derive(Debug, Default)]
struct Handles {
    made: HashMap<BufferId, Arc<Handle>>,
    entered: HashMap<Option<TaskId>, Vec<BufferId>>,
    values: HashMap<BufferId, HashMap<String, Hold>>,
}

fn killed() -> Error {
    Error::new("That buffer was killed")
}

fn handles(editor: &mut Editor) -> &mut Handles {
    if editor.plugin_state::<Handles>().is_none() {
        editor.set_plugin_state(Handles::default());
    }
    editor
        .plugin_state_mut::<Handles>()
        .unwrap_or_else(|| unreachable!("the handles were just added"))
}

pub(crate) fn handle(heap: &mut Heap, editor: &mut Editor, id: BufferId) -> Result<Value, Error> {
    let name = editor
        .buffer_by_id(id)
        .ok_or_else(killed)?
        .name()
        .to_owned();
    editor.subscribe(EventKind::Killed);
    let state = handles(editor);
    let shared = Arc::clone(state.made.entry(id).or_insert_with(|| Arc::new(Handle(id))));
    heap.shared(&format!("buffer {name}"), shared)
        .map_err(Error::from)
}

pub(crate) fn closed(heap: &mut Heap, editor: &mut Editor, id: BufferId) -> Result<Value, Error> {
    let shared = handles(editor)
        .made
        .remove(&id)
        .unwrap_or_else(|| Arc::new(Handle(id)));
    heap.shared("closed buffer", shared).map_err(Error::from)
}

pub(crate) fn forget(editor: &mut Editor, id: BufferId) {
    handles(editor).made.remove(&id);
}

pub(crate) fn release_values(heap: &mut Heap, editor: &mut Editor, id: BufferId) {
    let released = editor
        .plugin_state_mut::<Handles>()
        .and_then(|state| state.values.remove(&id));
    for hold in released.into_iter().flat_map(HashMap::into_values) {
        heap.release(hold);
    }
}

fn key(heap: &Heap, native: &str, value: Value) -> Result<String, Error> {
    match value {
        Value::Keyword(name) => Ok(name.name().to_owned()),
        _ => Err(expected(heap, native, "a keyword", value)),
    }
}

fn value(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let key = key(heap, "buffer-value", arguments[0])?;
    let id = current(heap, editor)?;
    Ok(editor
        .plugin_state::<Handles>()
        .and_then(|state| state.values.get(&id)?.get(&key).copied())
        .and_then(|hold| heap.held(hold))
        .unwrap_or(Value::Nil))
}

fn set_value(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let key = key(heap, "set-buffer-value!", arguments[0])?;
    let id = current(heap, editor)?;
    editor.subscribe(EventKind::Killed);
    let held = (arguments[1] != Value::Nil).then(|| heap.hold(arguments[1]));
    let values = &mut handles(editor).values;
    let kept = values.entry(id).or_default();
    let old = match held {
        Some(held) => kept.insert(key, held),
        None => kept.remove(&key),
    };
    if kept.is_empty() {
        values.remove(&id);
    }
    if let Some(old) = old {
        heap.release(old);
    }
    crate::view::views::redraw(editor);
    Ok(arguments[1])
}

fn of(heap: &Heap, value: Value) -> Option<BufferId> {
    heap.foreign_of::<Handle>(value).map(|handle| handle.0)
}

pub(crate) fn current(heap: &Heap, editor: &Editor) -> Result<BufferId, Error> {
    let entered = editor
        .plugin_state::<Handles>()
        .and_then(|state| state.entered.get(&heap.running_task()))
        .and_then(|stack| stack.last().copied());
    match entered {
        Some(id) if editor.buffer_by_id(id).is_some() => Ok(id),
        Some(_) => Err(killed()),
        None => Ok(editor.buffer().id()),
    }
}

pub(crate) fn current_buffer<'a>(heap: &Heap, editor: &'a Editor) -> Result<&'a Buffer, Error> {
    let id = current(heap, editor)?;
    editor.buffer_by_id(id).ok_or_else(killed)
}

pub(crate) fn current_buffer_mut<'a>(
    heap: &Heap,
    editor: &'a mut Editor,
) -> Result<&'a mut Buffer, Error> {
    let id = current(heap, editor)?;
    if id == editor.buffer().id() {
        return Ok(editor.buffer_mut());
    }
    editor.buffer_by_id_mut(id).ok_or_else(killed)
}

fn not_a_buffer(heap: &Heap, native: &str, value: Value) -> Error {
    Error::new(format!(
        "{native} expects a buffer or a buffer name, got {}",
        print::brief(heap, value)
    ))
}

pub(crate) fn target(
    heap: &Heap,
    editor: &Editor,
    native: &str,
    value: Value,
) -> Result<BufferId, Error> {
    if let Some(id) = of(heap, value) {
        return editor.buffer_by_id(id).map(|_| id).ok_or_else(killed);
    }
    let name = heap
        .text(value)
        .ok_or_else(|| not_a_buffer(heap, native, value))?;
    editor
        .buffer_named(name)
        .ok_or_else(|| Error::new(format!("There is no buffer named {name}")))
}

pub(crate) fn name(
    heap: &Heap,
    editor: &Editor,
    native: &str,
    value: Value,
) -> Result<String, Error> {
    if let Some(id) = of(heap, value) {
        return editor
            .buffer_by_id(id)
            .map(|buffer| buffer.name().to_owned())
            .ok_or_else(killed);
    }
    heap.text(value)
        .map(str::to_owned)
        .ok_or_else(|| not_a_buffer(heap, native, value))
}

pub(crate) fn live(heap: &Heap, editor: &Editor, value: Value) -> bool {
    match of(heap, value) {
        Some(id) => editor.buffer_by_id(id).is_some(),
        None => heap
            .text(value)
            .is_some_and(|name| editor.buffer_named(name).is_some()),
    }
}

fn enter(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let id = target(heap, editor, "with-buffer", arguments[0])?;
    handles(editor)
        .entered
        .entry(heap.running_task())
        .or_default()
        .push(id);
    Ok(Value::Nil)
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "natives return results even when they cannot fail"
)]
fn leave(heap: &mut Heap, editor: &mut Editor, _: &[Value]) -> Result<Value, Error> {
    let task = heap.running_task();
    let state = handles(editor);
    if let Some(stack) = state.entered.get_mut(&task) {
        stack.pop();
        if stack.is_empty() {
            state.entered.remove(&task);
        }
    }
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "current-buffer")
        .arity(0, Some(0))
        .host::<Editor>(|heap, editor, _| {
            let id = current(heap, editor)?;
            handle(heap, editor, id)
        });
    isolate
        .native("ox", "buffer?")
        .arity(1, Some(1))
        .host::<Editor>(|heap, _, arguments| Ok(Value::Bool(of(heap, arguments[0]).is_some())));
    isolate
        .native("ox", "buffer-live?")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            Ok(Value::Bool(live(heap, editor, arguments[0])))
        });
    isolate
        .native("ox", "enter-buffer")
        .arity(1, Some(1))
        .host::<Editor>(enter);
    isolate
        .native("ox", "leave-buffer")
        .arity(0, Some(0))
        .host::<Editor>(leave);
    isolate
        .native("ox", "buffer-value")
        .arity(1, Some(1))
        .host::<Editor>(value);
    isolate
        .native("ox", "set-buffer-value!")
        .arity(2, Some(2))
        .host::<Editor>(set_value);
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use crate::Ox;
    use crate::testing::settle;

    fn two_buffers() -> (Editor, Ox, String) {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        let home = editor.buffer().name().to_owned();
        ox.eval(
            &mut editor,
            r#"(def home (current-buffer)) (open-buffer "*other*" ["one" "two"]) (def other (current-buffer)) (switch-to-buffer home)"#,
        )
        .unwrap();
        (editor, ox, home)
    }

    #[test]
    fn current_buffer_is_a_handle_that_shows_its_name() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        let name = editor.buffer().name().to_owned();

        assert_eq!(
            ox.eval(&mut editor, "(current-buffer)").unwrap(),
            format!("#<buffer {name}>")
        );
        assert_eq!(
            ox.eval(
                &mut editor,
                r#"[(= (current-buffer) (current-buffer)) (buffer? (current-buffer)) (buffer? "x")]"#
            )
            .unwrap(),
            "[true true false]"
        );
    }

    #[test]
    fn with_buffer_works_in_another_buffer_and_leaves_the_windows_alone() {
        let (mut editor, mut ox, home) = two_buffers();

        assert_eq!(
            ox.eval(
                &mut editor,
                "(with-buffer other [(buffer-name) (buffer-text) (line-text 1)])"
            )
            .unwrap(),
            r#"["*other*" "one\ntwo" "two"]"#
        );
        assert_eq!(editor.buffer().name(), home);
        ox.eval(
            &mut editor,
            r#"(switch-to-buffer other) (with-buffer home (set-buffer-text "hello"))"#,
        )
        .unwrap();
        assert_eq!(editor.buffer().name(), "*other*");
        let id = editor.buffer_named(&home).unwrap();
        assert_eq!(editor.buffer_by_id(id).unwrap().contents(), "hello");
        assert_eq!(
            ox.eval(&mut editor, "(buffer-name)").unwrap(),
            r#""*other*""#
        );
    }

    #[test]
    fn handles_go_wherever_buffer_names_do() {
        let (mut editor, mut ox, _) = two_buffers();

        ox.eval(&mut editor, r#"(set-lines other ["x" "y"])"#)
            .unwrap();

        assert_eq!(
            ox.eval(
                &mut editor,
                "[(buffer-exists? other) (buffer-shown? other) (buffer-shown? home)]"
            )
            .unwrap(),
            "[true false true]"
        );
        let id = editor.buffer_named("*other*").unwrap();
        assert_eq!(editor.buffer_by_id(id).unwrap().contents(), "x\ny");
    }

    #[test]
    fn a_handle_to_a_killed_buffer_says_so() {
        let (mut editor, mut ox, _) = two_buffers();

        ox.eval(&mut editor, "(close-buffer other)").unwrap();

        assert_eq!(
            ox.eval(&mut editor, "[(buffer-live? other) (buffer-live? home)]")
                .unwrap(),
            "[false true]"
        );
        assert_eq!(
            ox.eval(&mut editor, "(with-buffer other (buffer-text))")
                .unwrap_err()
                .message,
            "That buffer was killed"
        );
    }

    #[test]
    fn buffers_keep_values_until_they_close() {
        let (mut editor, mut ox, _) = two_buffers();

        ox.eval(
            &mut editor,
            "(set-buffer-value! :count 1) (with-buffer other (set-buffer-value! :count (atom 2)) (set-buffer-value! :name \"b\"))",
        )
        .unwrap();
        assert_eq!(
            ox.eval(
                &mut editor,
                "[(buffer-value :count) @(with-buffer other (buffer-value :count)) (buffer-value :name)]"
            )
            .unwrap(),
            "[1 2 nil]"
        );
        ox.eval(
            &mut editor,
            "(set-buffer-value! :count nil) (close-buffer other)",
        )
        .unwrap();
        settle(&mut editor, &mut ox);

        assert_eq!(
            ox.eval(&mut editor, "(buffer-value :count)").unwrap(),
            "nil"
        );
        assert!(
            editor
                .plugin_state::<super::Handles>()
                .is_some_and(|state| state.values.is_empty())
        );
        assert_eq!(
            ox.eval(&mut editor, r#"(buffer-value "count")"#)
                .unwrap_err()
                .message,
            r#"buffer-value expects a keyword, got "count""#
        );
    }

    #[test]
    fn each_task_has_its_own_current_buffer() {
        let (mut editor, mut ox, home) = two_buffers();

        ox.eval(
            &mut editor,
            "(def seen (atom [])) (spawn (with-buffer other (sleep 20) (swap! seen conj (buffer-name))))",
        )
        .unwrap();
        assert_eq!(
            ox.eval(&mut editor, "(buffer-name)").unwrap(),
            format!("{home:?}")
        );
        settle(&mut editor, &mut ox);

        assert_eq!(ox.eval(&mut editor, "@seen").unwrap(), r#"["*other*"]"#);
        assert_eq!(
            ox.eval(&mut editor, "(buffer-name)").unwrap(),
            format!("{home:?}")
        );
    }
}
