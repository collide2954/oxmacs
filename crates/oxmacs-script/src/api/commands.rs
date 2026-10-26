use std::collections::HashMap;
use std::sync::Arc;

use ox_lang::heap::{Heap, Hold};
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::command::{DynamicSpec, Scope};
use oxmacs_core::editor::Editor;

use crate::ide::log;

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
                print::brief(heap, value)
            ))
        }),
    }
}

fn define(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = name_of(heap, arguments[0])?;
    let summary = print::display(heap, arguments[1]);
    let hold = heap.hold(arguments[2]);
    if editor.plugin_state::<Commands>().is_none() {
        editor.set_plugin_state(Commands::default());
    }
    let replaced = editor
        .plugin_state_mut::<Commands>()
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
            if let Some(commands) = editor.plugin_state_mut::<Commands>() {
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

fn run(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = name_of(heap, arguments[0])?;
    let command = editor
        .command_named(&name)
        .ok_or_else(|| Error::new(format!("There is no command named {name}")))?;
    editor.execute(command);
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("define-command", 3, Some(3), define);
    isolate.define_host_native::<Editor>("bind-key", 2, Some(2), bind);
    isolate.define_host_native::<Editor>("run-command", 1, Some(1), run);
}

pub fn queue(editor: &mut Editor, name: &str) {
    if editor.plugin_state::<Commands>().is_none() {
        editor.set_plugin_state(Commands::default());
    }
    if let Some(commands) = editor.plugin_state_mut::<Commands>() {
        commands.waiting.push(name.to_owned());
    }
}

pub fn defined(editor: &Editor, name: &str) -> bool {
    editor
        .plugin_state::<Commands>()
        .is_some_and(|commands| commands.defined.contains_key(name))
}

pub fn run_waiting(isolate: &mut Isolate, editor: &mut Editor) {
    let waiting = editor
        .plugin_state_mut::<Commands>()
        .map(|commands| std::mem::take(&mut commands.waiting))
        .unwrap_or_default();
    for name in waiting {
        let function = editor
            .plugin_state::<Commands>()
            .and_then(|commands| commands.defined.get(&name))
            .and_then(|hold| isolate.held(*hold));
        let Some(function) = function else {
            continue;
        };
        if let Err(error) = isolate.spawn(function, &[]) {
            log::report(editor, &error, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use crate::Ox;
    use crate::testing::{home_with, press};

    fn started(init: &str) -> (tempfile::TempDir, Editor, Ox) {
        let home = home_with(&[("init.ox", init)]);
        let mut editor = Editor::new();
        let mut ox = Ox::with_bundled(&[]);
        ox.start(&mut editor, Some(home.path()));
        (home, editor, ox)
    }

    #[test]
    fn keyboard_macros_let_ox_commands_finish_before_the_next_key() {
        let (_home, mut editor, mut ox) = started(
            r#"(def seen (atom [])) (defcommand note "Note the text" (swap! seen conj (buffer-text))) (bind-key "C-c m" 'note)"#,
        );
        press(&mut editor, &mut ox, "<f3> x C-c m y <f4>");

        press(&mut editor, &mut ox, "<f4>");

        assert_eq!(editor.buffer().contents(), "xyxy");
        assert_eq!(ox.eval(&mut editor, "@seen").unwrap(), r#"["x" "xyx"]"#);
    }

    #[test]
    fn an_ox_error_stops_the_keyboard_macro() {
        let (_home, mut editor, mut ox) =
            started(r#"(defcommand boom "Fail" (raise (error "no"))) (bind-key "C-c b" 'boom)"#);
        press(&mut editor, &mut ox, "<f3> a C-c b c <f4>");
        assert_eq!(editor.buffer().contents(), "ac");

        press(&mut editor, &mut ox, "<f4>");

        assert_eq!(editor.buffer().contents(), "aca");
        assert_eq!(editor.message(), Some("Ox: no"));
    }
}
