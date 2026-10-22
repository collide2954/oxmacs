use std::collections::HashMap;
use std::sync::Arc;

use ox::heap::{Heap, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::command::{DynamicSpec, Scope};
use oxmacs_core::editor::Editor;

#[derive(Debug, Default)]
pub struct Commands {
    defined: HashMap<String, Hold>,
    waiting: Vec<String>,
}

fn name_of(heap: &Heap, value: Value) -> Result<String, Error> {
    match value {
        Value::Symbol(name) => Ok(name.name().to_owned()),
        _ => heap.text(value).map(str::to_owned).ok_or_else(|| {
            Error::new(format!(
                "A command name must be a symbol, got {}",
                print::write(heap, value)
            ))
        }),
    }
}

fn define(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = name_of(heap, arguments[0])?;
    let summary = print::display(heap, arguments[1]);
    let hold = heap.hold(arguments[2]);
    if editor.applet_state::<Commands>().is_none() {
        editor.set_applet_state(Commands::default());
    }
    let replaced = editor
        .applet_state_mut::<Commands>()
        .and_then(|commands| commands.defined.insert(name.clone(), hold));
    if let Some(old) = replaced {
        heap.release(old);
    }
    let queued = name.clone();
    editor.add_dynamic_command(DynamicSpec {
        name: name.clone(),
        summary,
        scope: Scope::Global.into(),
        run: Arc::new(move |editor: &mut Editor| {
            if let Some(commands) = editor.applet_state_mut::<Commands>() {
                commands.waiting.push(queued.clone());
            }
        }),
    });
    Ok(Value::symbol(&name))
}

fn bind(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let keys = print::display(heap, arguments[0]);
    let name = name_of(heap, arguments[1])?;
    let command = editor
        .command_named(&name)
        .ok_or_else(|| Error::new(format!("There is no command named {name}")))?;
    editor
        .bind(Scope::Global, &keys, command)
        .map_err(|error| Error::new(format!("{keys}: {error}")))?;
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("define-command", 3, Some(3), define);
    isolate.define_host_native::<Editor>("bind-key", 2, Some(2), bind);
}

pub fn run_waiting(isolate: &mut Isolate, editor: &mut Editor) {
    let waiting = editor
        .applet_state_mut::<Commands>()
        .map(|commands| std::mem::take(&mut commands.waiting))
        .unwrap_or_default();
    for name in waiting {
        let function = editor
            .applet_state::<Commands>()
            .and_then(|commands| commands.defined.get(&name))
            .and_then(|hold| isolate.held(*hold));
        let Some(function) = function else {
            continue;
        };
        if let Err(error) = isolate.call_with(editor, function, &[]) {
            editor.show_message(format!("Ox: {}", error.message));
        }
    }
}
