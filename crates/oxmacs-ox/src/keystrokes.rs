use ox::heap::{Heap, HeapFull, Hold};
use ox::value::Value;
use ox::{Error, Isolate};
use oxmacs_core::editor::{Editor, Keystroke};

use crate::{hooks, log};

#[derive(Debug, Default)]
struct Listeners(Vec<(String, Hold)>);

fn event(heap: &mut Heap, stroke: &Keystroke) -> Result<Value, Error> {
    let full = |full: HeapFull| Error::new(full.to_string());
    let mut keys = Vec::with_capacity(stroke.keys.keys().len());
    for key in stroke.keys.keys() {
        keys.push(heap.string(&key.to_string()).map_err(full)?);
    }
    let keys = heap.vector_from(&keys).map_err(full)?;
    let command = heap.string(stroke.command).map_err(full)?;
    heap.map_from(&[
        Value::keyword("keys"),
        keys,
        Value::keyword("command"),
        command,
    ])
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>(
        "on-key-command",
        2,
        Some(2),
        |heap, editor, arguments| {
            editor.watch_keystrokes();
            if editor.plugin_state::<Listeners>().is_none() {
                editor.set_plugin_state(Listeners::default());
            }
            if let Some(listeners) = editor.plugin_state_mut::<Listeners>() {
                hooks::keyed(heap, &mut listeners.0, arguments);
            }
            Ok(Value::Nil)
        },
    );
}

pub fn run(isolate: &mut Isolate, editor: &mut Editor) {
    let Some(listeners) = editor.plugin_state::<Listeners>() else {
        return;
    };
    let handlers: Vec<Hold> = listeners.0.iter().map(|(_, hold)| *hold).collect();
    for stroke in editor.take_keystrokes() {
        if stroke.typed {
            continue;
        }
        let Ok(event) = event(isolate.heap_mut(), &stroke) else {
            continue;
        };
        for hold in &handlers {
            let Some(handler) = isolate.held(*hold) else {
                continue;
            };
            if let Err(error) = isolate.spawn(handler, &[event]) {
                log::report(editor, &error, None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use crate::Ox;
    use crate::testing::{press, settle, typed};

    const LISTEN: &str = r#"(module (import (ox ui :only [text])))
(def heard (atom []))
(defcommand bump "Bump" (message "bumped"))
(on-key-command :test (fn [event] (swap! heard conj [event.keys event.command])))"#;

    #[test]
    fn key_commands_reach_their_handlers_but_typing_does_not() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, LISTEN).unwrap();

        press(&mut editor, &mut ox, "a b C-x h C-n");
        press(&mut editor, &mut ox, "M-x");
        typed(&mut editor, &mut ox, "end-of-buffer");
        press(&mut editor, &mut ox, "RET");
        ox.eval(
            &mut editor,
            r#"(open-view "*keys*" (fn [] (text "k")) :keys {n: 'bump})"#,
        )
        .unwrap();
        settle(&mut editor, &mut ox);
        press(&mut editor, &mut ox, "n");

        assert_eq!(
            ox.eval(&mut editor, "@heard").unwrap(),
            r#"[[["C-x" "h"] "mark-whole-buffer"] [["C-n"] "next-line"] [["M-x"] "end-of-buffer"] [["n"] "bump"]]"#
        );
    }

    #[test]
    fn registering_a_key_command_handler_again_replaces_it() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, LISTEN).unwrap();
        ox.eval(
            &mut editor,
            "(on-key-command :test (fn [event] (swap! heard conj event.command)))",
        )
        .unwrap();

        press(&mut editor, &mut ox, "C-n");

        assert_eq!(ox.eval(&mut editor, "@heard").unwrap(), r#"["next-line"]"#);
    }
}
