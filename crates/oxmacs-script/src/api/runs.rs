use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use ox_lang::heap::{Heap, Hold};
use ox_lang::native::{expected, given, text};
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Editor, RunSpec};

use crate::ide::log;

const NAME: &str = "run-job";
const OPTIONS: [&str; 6] = ["cwd", "env", "title", "input", "on-line", "on-exit"];

#[derive(Debug, Default)]
struct Held {
    line: Option<Hold>,
    exit: Option<Hold>,
}

#[derive(Debug)]
enum Call {
    Line(String, String),
    Exit(String, Option<i32>),
}

#[derive(Debug, Default)]
struct Runs {
    held: HashMap<String, Held>,
    waiting: Vec<Call>,
}

fn queue(editor: &mut Editor, call: Call) {
    if let Some(runs) = editor.plugin_state_mut::<Runs>() {
        runs.waiting.push(call);
    }
}

fn words(heap: &Heap, value: Value) -> Result<Vec<String>, Error> {
    heap.text(value)
        .is_none()
        .then(|| heap.elements(value))
        .flatten()
        .map(|items| {
            items
                .into_iter()
                .map(|item| print::display(heap, item))
                .collect()
        })
        .ok_or_else(|| expected(heap, NAME, "a vector of arguments", value))
}

fn environment(heap: &Heap, value: Value) -> Result<Vec<(String, String)>, Error> {
    let entries = heap
        .entries(value)
        .ok_or_else(|| expected(heap, NAME, "a map for :env", value))?;
    Ok(entries
        .into_iter()
        .map(|(key, given)| {
            let key = match key {
                Value::Keyword(name) => name.name().to_owned(),
                other => print::display(heap, other),
            };
            (key, print::display(heap, given))
        })
        .collect())
}

fn run(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = text(heap, NAME, arguments[0])?.to_owned();
    let program = text(heap, NAME, arguments[1])?.to_owned();
    let mut spec = RunSpec {
        directory: editor.default_directory(),
        ..RunSpec::new(name.clone(), program, words(heap, arguments[2])?)
    };
    let mut held = Held::default();
    for (option, value) in given(&OPTIONS, &arguments[3..]) {
        if value == Value::Nil {
            continue;
        }
        match option {
            "cwd" => spec.directory = PathBuf::from(text(heap, NAME, value)?),
            "env" => spec.env = environment(heap, value)?,
            "title" => spec.title = Some(text(heap, NAME, value)?.to_owned()),
            "input" => spec.input = Some(text(heap, NAME, value)?.to_owned()),
            "on-line" => {
                held.line = Some(heap.hold(value));
                let named = name.clone();
                spec.on_line = Some(Arc::new(move |editor, line| {
                    queue(editor, Call::Line(named.clone(), line.to_owned()));
                }));
            }
            "on-exit" => {
                held.exit = Some(heap.hold(value));
                let named = name.clone();
                spec.on_exit = Some(Arc::new(move |editor, code| {
                    queue(editor, Call::Exit(named.clone(), code));
                }));
            }
            _ => {}
        }
    }
    if editor.plugin_state::<Runs>().is_none() {
        editor.set_plugin_state(Runs::default());
    }
    let old = editor
        .plugin_state_mut::<Runs>()
        .and_then(|runs| runs.held.insert(name, held));
    for hold in old.iter().flat_map(|old| old.line.iter().chain(&old.exit)) {
        heap.release(*hold);
    }
    editor.run_in_buffer(spec);
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", NAME)
        .arity(3, Some(3))
        .keywords(&OPTIONS)
        .host::<Editor>(run);
}

pub fn run_waiting(isolate: &mut Isolate, editor: &mut Editor) {
    let waiting = editor
        .plugin_state_mut::<Runs>()
        .map(|runs| std::mem::take(&mut runs.waiting))
        .unwrap_or_default();
    for call in waiting {
        let found = editor.plugin_state::<Runs>().and_then(|runs| match &call {
            Call::Line(name, _) => runs.held.get(name)?.line,
            Call::Exit(name, _) => runs.held.get(name)?.exit,
        });
        let Some(function) = found.and_then(|hold| isolate.held(hold)) else {
            continue;
        };
        let argument = match &call {
            Call::Line(_, line) => isolate.heap_mut().string(line).map_err(Error::from),
            Call::Exit(_, code) => Ok(code.map_or(Value::Nil, |code| Value::Int(i64::from(code)))),
        };
        let spawned = argument.and_then(|argument| isolate.spawn(function, &[argument]));
        if let Err(error) = spawned {
            log::report(editor, &error, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use oxmacs_core::editor::Editor;

    use crate::Ox;
    use crate::testing::{lines, settle};

    #[test]
    fn ox_runs_a_program_into_its_own_buffer_and_hears_each_line_and_the_end() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        let (program, args) = if cfg!(windows) {
            ("cmd", r#"["/C" "echo one"]"#)
        } else {
            ("sh", r#"["-c" "echo one"]"#)
        };
        ox.eval(
            &mut editor,
            &format!(
                "(def heard (atom [])) (run-job \"*job*\" {program:?} {args} :title \"Saying one\" :on-line (fn [line] (swap! heard conj line)) :on-exit (fn [code] (swap! heard conj code)))"
            ),
        )
        .unwrap();
        let mut runner = crate::testing::Runner::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        while ox.eval(&mut editor, "(count @heard)").unwrap() != "2" {
            assert!(
                Instant::now() < deadline,
                "{:?}",
                ox.eval(&mut editor, "@heard")
            );
            runner.step(&mut editor);
            settle(&mut editor, &mut ox);
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(ox.eval(&mut editor, "@heard").unwrap(), r#"["one" 0]"#);
        let id = editor.buffer_named("*job*").unwrap();
        editor.show_buffer(id);
        let shown = lines(&editor);
        assert_eq!(shown[1], "Saying one");
        assert!(shown.contains(&"one".to_owned()), "{shown:?}");
        assert_eq!(shown.last().map(String::as_str), Some("Finished"));
    }
}
