use std::ops::Range;
use std::time::{Duration, Instant};

use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::Editor;
use oxmacs_core::text::{Buffer, BufferId};
use oxmacs_core::theme::Highlight;

use crate::api::handlers::{self, Hook};
use crate::api::handles;
use crate::view::views::face_named;

const DURATION: Duration = Duration::from_millis(500);

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

fn span(
    heap: &Heap,
    editor: &Editor,
    native: &str,
    arguments: &[Value],
) -> Result<(BufferId, Range<usize>), Error> {
    let start = position(heap, native, arguments[0])?;
    let end = position(heap, native, arguments[1])?;
    let buffer = handles::current(heap, editor)?;
    Ok((buffer, start.min(end)..start.max(end)))
}

fn face(heap: &Heap, value: Value) -> Result<Highlight, Error> {
    match value {
        Value::Keyword(name) => face_named(name.name()),
        _ => None,
    }
    .ok_or_else(|| {
        Error::new(format!(
            "flash expects a face such as :region or :removed, got {}",
            print::brief(heap, value)
        ))
    })
}

fn milliseconds(heap: &Heap, value: Value) -> Result<Duration, Error> {
    match value {
        Value::Int(number) => u64::try_from(number).ok().map(Duration::from_millis),
        _ => None,
    }
    .ok_or_else(|| {
        Error::new(format!(
            "flash expects :duration in milliseconds from 0, got {}",
            print::brief(heap, value)
        ))
    })
}

fn flash(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let (buffer, range) = span(heap, editor, "flash", arguments)?;
    let shown = match arguments[2] {
        Value::Nil => Highlight::Region,
        given => face(heap, given)?,
    };
    let duration = match arguments[3] {
        Value::Nil => DURATION,
        given => milliseconds(heap, given)?,
    };
    editor.flash(buffer, range, shown, duration, Instant::now());
    Ok(Value::Nil)
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
        .native("ox", "flash")
        .arity(2, Some(2))
        .keywords(&["face", "duration"])
        .host::<Editor>(flash);
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
    use std::time::{Duration, Instant};

    use oxmacs_core::editor::Editor;
    use oxmacs_core::theme::Highlight;

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
    fn flash_lights_up_characters_of_the_current_buffer() {
        let (mut editor, mut ox) = written("abc def\nghi");
        let before = Instant::now();

        eval(&mut editor, &mut ox, "(flash 4 9)");

        let pane = editor.pane();
        let [(first, Highlight::Region, _)] = &pane.line_flashes(0, before)[..] else {
            panic!("{:?}", pane.line_flashes(0, before));
        };
        assert_eq!(*first, 4..7);
        assert_eq!(pane.line_flashes(1, before)[0].0, 0..1);
        assert!(pane.flashing(before + Duration::from_millis(400)));
        assert!(!pane.flashing(Instant::now() + Duration::from_millis(500)));
    }

    #[test]
    fn flash_takes_a_face_and_a_duration() {
        let (mut editor, mut ox) = written("abc def");
        let before = Instant::now();

        eval(
            &mut editor,
            &mut ox,
            "(flash 7 4 :face :removed :duration 2000)",
        );

        let pane = editor.pane();
        assert_eq!(pane.line_flashes(0, before)[0].0, 4..7);
        assert_eq!(pane.line_flashes(0, before)[0].1, Highlight::Removed);
        assert!(pane.flashing(before + Duration::from_millis(1900)));
        assert_eq!(
            eval(&mut editor, &mut ox, "(flash 0 1 :face :plaid)"),
            "error: flash expects a face such as :region or :removed, got :plaid"
        );
        assert_eq!(
            eval(&mut editor, &mut ox, "(flash 0 1 :speed 3)"),
            "error: flash has no keyword parameter :speed"
        );
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
