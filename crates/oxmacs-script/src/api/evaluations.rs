use std::ops::Range;

use ox_lang::heap::Heap;
use ox_lang::value::Value;
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::Editor;
use oxmacs_core::text::{Buffer, BufferId};

use crate::api::annotations::span;
use crate::api::handlers::{self, Hook};
use crate::api::handles;

#[derive(Debug)]
struct Ran {
    buffer: BufferId,
    range: Range<usize>,
    failed: bool,
}

#[derive(Debug)]
struct Sent {
    console: BufferId,
    source: BufferId,
    ranges: Vec<Range<usize>>,
}

#[derive(Debug, Default)]
struct Evaluations {
    waiting: Vec<Ran>,
    sent: Vec<Sent>,
}

fn listening(editor: &Editor) -> bool {
    handlers::listening(editor, Hook::Evaluate)
}

pub(crate) fn evaluated(editor: &mut Editor, buffer: BufferId, range: Range<usize>, failed: bool) {
    if range.is_empty() || !listening(editor) {
        return;
    }
    if let Some(state) = editor.plugin_state_mut::<Evaluations>() {
        state.waiting.push(Ran {
            buffer,
            range,
            failed,
        });
    }
}

pub(crate) fn sent(editor: &mut Editor, console: BufferId, source: BufferId, code: &str) {
    if !listening(editor) {
        return;
    }
    let ranges = editor.sent_ranges(source, code);
    for range in &ranges {
        evaluated(editor, source, range.clone(), false);
    }
    if let Some(state) = editor.plugin_state_mut::<Evaluations>() {
        state.sent.retain(|sent| sent.console != console);
        state.sent.push(Sent {
            console,
            source,
            ranges,
        });
    }
}

fn last_sent(editor: &mut Editor, console: BufferId) -> Option<Sent> {
    let state = editor.plugin_state_mut::<Evaluations>()?;
    let at = state.sent.iter().position(|sent| sent.console == console)?;
    Some(state.sent.remove(at))
}

pub(crate) fn send_failed(editor: &mut Editor, console: BufferId) {
    if let Some(sent) = last_sent(editor, console) {
        for range in sent.ranges {
            evaluated(editor, sent.source, range, true);
        }
    }
}

pub(crate) fn send_settled(editor: &mut Editor, console: BufferId) {
    last_sent(editor, console);
}

fn int(number: usize) -> Value {
    Value::Int(i64::try_from(number).unwrap_or(i64::MAX))
}

fn zap(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let (buffer, range) = span(heap, editor, "zap", arguments)?;
    let failed = arguments[2].truthy();
    let last = editor.buffer_by_id(buffer).map_or(0, Buffer::len_chars);
    evaluated(
        editor,
        buffer,
        range.start.min(last)..range.end.min(last),
        failed,
    );
    Ok(Value::Nil)
}

fn on_evaluate(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) {
    if editor.plugin_state::<Evaluations>().is_none() {
        editor.set_plugin_state(Evaluations::default());
    }
    let key = handlers::key(heap, arguments[0]);
    handlers::register(heap, editor, Hook::Evaluate, key, arguments[1]);
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "zap")
        .arity(2, Some(2))
        .keywords(&["error"])
        .host::<Editor>(zap);
    isolate
        .native("ox", "on-evaluate")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            on_evaluate(heap, editor, arguments);
            Ok(Value::Nil)
        });
}

pub fn run(isolate: &mut Isolate, editor: &mut Editor) {
    let Some(state) = editor.plugin_state_mut::<Evaluations>() else {
        return;
    };
    let waiting = std::mem::take(&mut state.waiting);
    let holds = handlers::held(editor, Hook::Evaluate);
    for ran in waiting {
        let Ok(buffer) = handles::handle(isolate.heap_mut(), editor, ran.buffer) else {
            continue;
        };
        let arguments = [
            buffer,
            int(ran.range.start),
            int(ran.range.end),
            Value::Bool(ran.failed),
        ];
        handlers::spawn(isolate, editor, &holds, &arguments);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use oxmacs_core::editor::Editor;

    use crate::Ox;
    use crate::testing::settle;

    fn written(text: &str) -> (Editor, Ox) {
        let mut editor = Editor::new();
        editor.buffer_mut().insert_at(0, text);
        (editor, Ox::new())
    }

    fn eval(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
        match ox.eval(editor, source) {
            Ok(shown) => shown,
            Err(error) => format!("error: {}", error.message),
        }
    }

    #[test]
    fn zap_tells_the_on_evaluate_handlers_what_ran() {
        let (mut editor, mut ox) = written("abc def");
        eval(
            &mut editor,
            &mut ox,
            "(def heard (atom [])) (on-evaluate :test (fn [b start end failed] (swap! heard conj [(with-buffer b (buffer-name)) start end failed])))",
        );

        eval(&mut editor, &mut ox, "(zap 4 7) (zap 3 0 :error true)");
        settle(&mut editor, &mut ox);

        assert_eq!(
            eval(&mut editor, &mut ox, "@heard"),
            r#"[["*scratch*" 4 7 false] ["*scratch*" 0 3 true]]"#
        );
    }

    #[test]
    fn zap_without_handlers_does_nothing() {
        let (mut editor, mut ox) = written("abc");

        assert_eq!(eval(&mut editor, &mut ox, "(zap 0 3)"), "nil");
        settle(&mut editor, &mut ox);

        assert!(!editor.pane().flashing(Instant::now()));
    }
}
