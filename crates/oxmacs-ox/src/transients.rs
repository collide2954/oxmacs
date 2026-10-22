use std::collections::HashMap;

use ox::heap::{Heap, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::editor::{Editor, Group, Transient};

use crate::log;

#[derive(Debug, Default)]
struct Transients {
    waiting: Vec<(Hold, Vec<String>)>,
    held: HashMap<String, Vec<Hold>>,
}

fn state(editor: &mut Editor) -> Option<&mut Transients> {
    if editor.plugin_state::<Transients>().is_none() {
        editor.set_plugin_state(Transients::default());
    }
    editor.plugin_state_mut::<Transients>()
}

fn queue(editor: &mut Editor, hold: Hold, flags: &[String]) {
    if let Some(transients) = state(editor) {
        transients.waiting.push((hold, flags.to_vec()));
    }
}

fn key(heap: &Heap, value: Value) -> Result<char, Error> {
    let text = heap.text(value).unwrap_or_default();
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(character), None) => Ok(character),
        _ => Err(Error::new(format!(
            "transient keys are single characters, got {}",
            print::write(heap, value)
        ))),
    }
}

fn parts(heap: &Heap, value: Value, kind: &str, shape: &str) -> Result<Vec<Vec<Value>>, Error> {
    let wrong = |value: Value| {
        Error::new(format!(
            "transient {kind} are {shape}, got {}",
            print::write(heap, value)
        ))
    };
    heap.elements(value)
        .ok_or_else(|| wrong(value))?
        .into_iter()
        .map(|item| {
            heap.elements(item)
                .filter(|found| found.len() == 3)
                .ok_or_else(|| wrong(item))
        })
        .collect()
}

fn group(heap: &mut Heap, value: Value, holds: &mut Vec<Hold>) -> Result<Group, Error> {
    let entries = heap.entries(value).ok_or_else(|| {
        Error::new(format!(
            "transient groups are maps with :title, :switches and :actions, got {}",
            print::write(heap, value)
        ))
    })?;
    let field = |name: &str| {
        entries
            .iter()
            .find(|(key, _)| matches!(key, Value::Keyword(key) if key.name() == name))
            .map(|(_, value)| *value)
    };
    let mut made =
        Group::new(&field("title").map_or_else(String::new, |title| print::display(heap, title)));
    if let Some(switches) = field("switches") {
        for switch in parts(heap, switches, "switches", "[key flag summary]")? {
            made = made.switch(
                key(heap, switch[0])?,
                &print::display(heap, switch[1]),
                &print::display(heap, switch[2]),
            );
        }
    }
    if let Some(actions) = field("actions") {
        for action in parts(heap, actions, "actions", "[key summary function]")? {
            let character = key(heap, action[0])?;
            let summary = print::display(heap, action[1]);
            let hold = heap.hold(action[2]);
            holds.push(hold);
            made = made.action(character, &summary, move |editor, flags| {
                queue(editor, hold, flags);
            });
        }
    }
    Ok(made)
}

fn open(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let title = print::display(heap, arguments[0]);
    let given = heap.elements(arguments[1]).ok_or_else(|| {
        Error::new(format!(
            "transient expects a vector of groups, got {}",
            print::write(heap, arguments[1])
        ))
    })?;
    let mut holds = Vec::new();
    let built: Result<Vec<Group>, Error> = given
        .into_iter()
        .map(|value| group(heap, value, &mut holds))
        .collect();
    let groups = match built {
        Ok(groups) => groups,
        Err(error) => {
            for hold in holds {
                heap.release(hold);
            }
            return Err(error);
        }
    };
    if let Some(transients) = state(editor)
        && let Some(old) = transients.held.insert(title.clone(), holds)
    {
        for hold in old {
            heap.release(hold);
        }
    }
    editor.open_transient(Transient::new(&title, groups));
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("transient", 2, Some(2), open);
}

pub fn run_waiting(isolate: &mut Isolate, editor: &mut Editor) {
    let waiting = state(editor)
        .map(|transients| std::mem::take(&mut transients.waiting))
        .unwrap_or_default();
    for (hold, flags) in waiting {
        let Some(function) = isolate.held(hold) else {
            continue;
        };
        let heap = isolate.heap_mut();
        let made: Result<Vec<Value>, _> = flags.iter().map(|flag| heap.string(flag)).collect();
        let Ok(flags) = made.and_then(|flags| heap.vector_from(&flags)) else {
            continue;
        };
        if let Err(error) = isolate.spawn(function, &[flags]) {
            log::report(editor, &error, None);
        }
    }
}
#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use crate::Ox;
    use crate::testing::press;

    const COMMIT: &str = r#"(transient "Commit"
      [{title: "Arguments" switches: [["a" "--all" "Stage all"] ["n" "--no-verify" "Skip hooks"]]}
       {title: "Create" actions: [["c" "Commit" (fn [switches] (reset! ran switches))]]}])"#;

    #[test]
    fn ox_transients_run_their_function_with_the_switches_that_are_on() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, &format!("(def ran (atom nil)) {COMMIT}"))
            .unwrap();

        let view = editor.transient().unwrap();
        assert_eq!(view.title, "Commit");
        let keys: Vec<String> = view
            .groups
            .iter()
            .flat_map(|group| group.entries.iter().map(|entry| entry.key.clone()))
            .collect();
        assert_eq!(keys, ["-a", "-n", "c"]);

        press(&mut editor, &mut ox, "- a c");
        assert_eq!(ox.eval(&mut editor, "@ran").unwrap(), r#"["--all"]"#);
        assert!(editor.transient().is_none());

        ox.eval(&mut editor, COMMIT).unwrap();
        assert!(editor.transient().unwrap().groups[0].entries[0].on);
    }

    #[test]
    fn mistakes_in_transients_are_errors() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        let long = ox
            .eval(
                &mut editor,
                r#"(transient "X" [{title: "G" actions: [["cc" "Two keys" (fn [s] nil)]]}])"#,
            )
            .unwrap_err();
        let shape = ox
            .eval(
                &mut editor,
                r#"(transient "X" [{title: "G" switches: [["a"]]}])"#,
            )
            .unwrap_err();

        assert_eq!(
            long.message,
            r#"transient keys are single characters, got "cc""#
        );
        assert_eq!(
            shape.message,
            r#"transient switches are [key flag summary], got ["a"]"#
        );
        assert!(editor.transient().is_none());
    }
}
