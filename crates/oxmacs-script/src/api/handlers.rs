use ox_lang::Isolate;
use ox_lang::heap::{Heap, Hold};
use ox_lang::value::{Value, print};
use oxmacs_core::editor::Editor;

use crate::ide::log;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hook {
    OpenDirectory,
    OpenFile,
    DiskChange,
    Saved,
    SettingsChange,
    ProcessesChange,
    BufferOpen,
    BufferClose,
    BufferChange,
    ModeEnter,
    WindowChange,
    DiagnosticsChange,
    BeforeSave,
    KeyCommand,
    Evaluate,
    Idle,
}

#[derive(Debug, Default)]
struct Handlers(Vec<(Hook, String, Hold)>);

pub(crate) fn key(heap: &Heap, value: Value) -> String {
    print::display(heap, value)
}

pub(crate) fn register(
    heap: &mut Heap,
    editor: &mut Editor,
    hook: Hook,
    key: String,
    function: Value,
) {
    let hold = heap.hold(function);
    if editor.plugin_state::<Handlers>().is_none() {
        editor.set_plugin_state(Handlers::default());
    }
    let Some(handlers) = editor.plugin_state_mut::<Handlers>() else {
        heap.release(hold);
        return;
    };
    match handlers
        .0
        .iter_mut()
        .find(|(known, name, _)| *known == hook && *name == key)
    {
        Some((_, _, existing)) => {
            heap.release(std::mem::replace(existing, hold));
        }
        None => handlers.0.push((hook, key, hold)),
    }
}

pub(crate) fn held(editor: &Editor, hook: Hook) -> Vec<Hold> {
    editor
        .plugin_state::<Handlers>()
        .map(|handlers| {
            handlers
                .0
                .iter()
                .filter(|(known, _, _)| *known == hook)
                .map(|(_, _, hold)| *hold)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn held_for(editor: &Editor, hook: Hook, key: &str) -> Option<Hold> {
    editor.plugin_state::<Handlers>().and_then(|handlers| {
        handlers
            .0
            .iter()
            .find(|(known, name, _)| *known == hook && name == key)
            .map(|(_, _, hold)| *hold)
    })
}

pub(crate) fn listening(editor: &Editor, hook: Hook) -> bool {
    editor
        .plugin_state::<Handlers>()
        .is_some_and(|handlers| handlers.0.iter().any(|(known, _, _)| *known == hook))
}

pub(crate) fn spawn(
    isolate: &mut Isolate,
    editor: &mut Editor,
    holds: &[Hold],
    arguments: &[Value],
) {
    for hold in holds {
        let Some(handler) = isolate.held(*hold) else {
            continue;
        };
        if let Err(error) = isolate.spawn(handler, arguments) {
            log::report(editor, &error, None);
        }
    }
}
