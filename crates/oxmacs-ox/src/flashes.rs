use std::ops::Range;
use std::time::{Duration, Instant};

use ox::heap::{Heap, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::editor::Editor;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::views::face_named;
use crate::{handles, log};

const DURATION: Duration = Duration::from_millis(500);

#[derive(Debug)]
struct Ran {
    buffer: BufferId,
    range: Range<usize>,
    failed: bool,
}

#[derive(Debug, Default)]
struct Evaluations {
    handlers: Vec<(String, Hold)>,
    waiting: Vec<Ran>,
}

fn listening(editor: &Editor) -> bool {
    editor
        .plugin_state::<Evaluations>()
        .is_some_and(|state| !state.handlers.is_empty())
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

pub(crate) fn sent(editor: &mut Editor, source: BufferId, code: &str) {
    if !listening(editor) {
        return;
    }
    for range in editor.sent_ranges(source, code) {
        evaluated(editor, source, range, false);
    }
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
    let mut shown = Highlight::Region;
    let mut duration = DURATION;
    for pair in arguments[2..].chunks(2) {
        match pair {
            [Value::Keyword(option), value] if option.name() == "face" => {
                shown = face(heap, *value)?;
            }
            [Value::Keyword(option), value] if option.name() == "duration" => {
                duration = milliseconds(heap, *value)?;
            }
            _ => {
                return Err(Error::new(format!(
                    "flash takes :face and :duration, not {}",
                    print::brief(heap, pair[0])
                )));
            }
        }
    }
    editor.flash(buffer, range, shown, duration, Instant::now());
    Ok(Value::Nil)
}

fn zap(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let (buffer, range) = span(heap, editor, "zap", arguments)?;
    let mut failed = false;
    for pair in arguments[2..].chunks(2) {
        match pair {
            [Value::Keyword(option), value] if option.name() == "error" => {
                failed = value.truthy();
            }
            _ => {
                return Err(Error::new(format!(
                    "zap takes :error, not {}",
                    print::brief(heap, pair[0])
                )));
            }
        }
    }
    let last = editor
        .buffer_by_id(buffer)
        .map_or(0, oxmacs_core::text::Buffer::len_chars);
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
    let Some(state) = editor.plugin_state_mut::<Evaluations>() else {
        return;
    };
    let key = print::display(heap, arguments[0]);
    let hold = heap.hold(arguments[1]);
    match state.handlers.iter_mut().find(|(known, _)| *known == key) {
        Some((_, existing)) => {
            let old = std::mem::replace(existing, hold);
            heap.release(old);
        }
        None => state.handlers.push((key, hold)),
    }
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("flash", 2, Some(6), flash);
    isolate.define_host_native::<Editor>("zap", 2, Some(4), zap);
    isolate.define_host_native::<Editor>("on-evaluate", 2, Some(2), |heap, editor, arguments| {
        on_evaluate(heap, editor, arguments);
        Ok(Value::Nil)
    });
}

pub fn run(isolate: &mut Isolate, editor: &mut Editor) {
    let Some(state) = editor.plugin_state_mut::<Evaluations>() else {
        return;
    };
    let waiting = std::mem::take(&mut state.waiting);
    let handlers: Vec<Hold> = state.handlers.iter().map(|(_, hold)| *hold).collect();
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
        for hold in &handlers {
            let Some(handler) = isolate.held(*hold) else {
                continue;
            };
            if let Err(error) = isolate.spawn(handler, &arguments) {
                log::report(editor, &error, None);
            }
        }
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
            "error: flash takes :face and :duration, not :speed"
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
