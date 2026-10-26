use std::time::{Duration, Instant};

use ox_lang::heap::{Heap, Hold};
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::Editor;

use crate::ide::log;

#[derive(Debug)]
struct Handler {
    key: String,
    after: Duration,
    hold: Hold,
    fired: Option<Instant>,
}

#[derive(Debug, Default)]
struct Resting(Vec<Handler>);

pub fn next(editor: &Editor, since: Instant) -> Option<Duration> {
    let handlers = &editor.plugin_state::<Resting>()?.0;
    let rested = handlers
        .iter()
        .filter(|handler| handler.fired == Some(since))
        .map(|handler| handler.after)
        .max()
        .unwrap_or_default();
    handlers
        .iter()
        .filter(|handler| handler.fired != Some(since))
        .map(|handler| handler.after.saturating_sub(rested))
        .min()
}

fn seconds(heap: &Heap, value: Value) -> Result<Duration, Error> {
    let after = match value {
        Value::Int(seconds) => u64::try_from(seconds).ok().map(Duration::from_secs),
        Value::Float(seconds) => Duration::try_from_secs_f64(seconds).ok(),
        _ => None,
    };
    after.filter(|after| !after.is_zero()).ok_or_else(|| {
        Error::new(format!(
            "on-idle expects a positive number of seconds, got {}",
            print::brief(heap, value)
        ))
    })
}

fn on_idle(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let after = seconds(heap, arguments[1])?;
    let key = print::display(heap, arguments[0]);
    let hold = heap.hold(arguments[2]);
    let mut resting = editor.take_plugin_state::<Resting>().unwrap_or_default();
    match resting.0.iter_mut().find(|handler| handler.key == key) {
        Some(handler) => {
            heap.release(std::mem::replace(&mut handler.hold, hold));
            handler.after = after;
            handler.fired = None;
        }
        None => resting.0.push(Handler {
            key,
            after,
            hold,
            fired: None,
        }),
    }
    editor.set_plugin_state(resting);
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("on-idle", 3, Some(3), on_idle);
}

pub fn run(isolate: &mut Isolate, editor: &mut Editor, since: Instant, now: Instant) {
    let rested = now.saturating_duration_since(since);
    let Some(resting) = editor.plugin_state_mut::<Resting>() else {
        return;
    };
    let due: Vec<Hold> = resting
        .0
        .iter_mut()
        .filter(|handler| handler.fired != Some(since) && handler.after <= rested)
        .map(|handler| {
            handler.fired = Some(since);
            handler.hold
        })
        .collect();
    for hold in due {
        if let Some(function) = isolate.held(hold)
            && let Err(error) = isolate.spawn(function, &[])
        {
            log::report(editor, &error, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use oxmacs_core::editor::Editor;

    use super::next;
    use crate::Ox;
    use crate::testing::settle;

    const NAPS: &str = r"(def naps (atom []))
(on-idle :nap 60 (fn [] (swap! naps conj :nap)))
(on-idle :sleep 300 (fn [] (swap! naps conj :sleep)))";

    fn rest(ox: &mut Ox, editor: &mut Editor, since: Instant, seconds: u64) -> String {
        ox.idle(editor, since, since + Duration::from_secs(seconds));
        settle(editor, ox);
        ox.eval(editor, "@naps").unwrap()
    }

    #[test]
    fn idle_handlers_run_once_per_stretch_without_input() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, NAPS).unwrap();
        let since = Instant::now();

        assert_eq!(next(&editor, since), Some(Duration::from_secs(60)));
        assert_eq!(rest(&mut ox, &mut editor, since, 30), "[]");
        assert_eq!(rest(&mut ox, &mut editor, since, 61), "[:nap]");
        assert_eq!(next(&editor, since), Some(Duration::from_secs(240)));
        assert_eq!(rest(&mut ox, &mut editor, since, 100), "[:nap]");
        assert_eq!(rest(&mut ox, &mut editor, since, 300), "[:nap :sleep]");
        assert_eq!(next(&editor, since), None);

        let later = since + Duration::from_secs(400);
        assert_eq!(next(&editor, later), Some(Duration::from_secs(60)));
        assert_eq!(rest(&mut ox, &mut editor, later, 60), "[:nap :sleep :nap]");
    }

    #[test]
    fn registering_a_key_again_replaces_its_handler() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, NAPS).unwrap();
        ox.eval(
            &mut editor,
            "(on-idle :nap 1.5 (fn [] (swap! naps conj :doze)))",
        )
        .unwrap();
        let since = Instant::now();

        assert_eq!(next(&editor, since), Some(Duration::from_millis(1500)));
        assert_eq!(rest(&mut ox, &mut editor, since, 2), "[:doze]");
        assert_eq!(
            ox.eval(&mut editor, "(on-idle :nap -1 (fn []))")
                .unwrap_err()
                .to_string(),
            "on-idle expects a positive number of seconds, got -1"
        );
    }
}
