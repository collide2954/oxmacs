use std::path::{Path, PathBuf};

use ox::heap::{Heap, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::editor::Editor;
use oxmacs_core::text::BufferId;

use crate::{log, plugins};

#[derive(Debug)]
enum Waiting {
    OpenDirectory(PathBuf),
    DiskChanged,
    Saved(PathBuf),
}

#[derive(Debug, Default)]
pub struct Hooks {
    directory: Option<Hold>,
    opener: Option<String>,
    disk: Vec<(String, Hold)>,
    saved: Vec<(String, Hold)>,
    waiting: Vec<Waiting>,
}

fn hooks(editor: &mut Editor) -> Option<&mut Hooks> {
    if editor.plugin_state::<Hooks>().is_none() {
        editor.set_plugin_state(Hooks::default());
    }
    editor.plugin_state_mut::<Hooks>()
}

fn open_directory(editor: &mut Editor, path: &Path) -> bool {
    let Some(opener) = editor
        .plugin_state::<Hooks>()
        .and_then(|hooks| hooks.opener.clone())
    else {
        return false;
    };
    plugins::request_wake(editor, &opener);
    if let Some(hooks) = hooks(editor) {
        hooks
            .waiting
            .push(Waiting::OpenDirectory(path.to_path_buf()));
    }
    true
}

fn disk_changed(editor: &mut Editor) {
    if let Some(hooks) = hooks(editor) {
        hooks.waiting.push(Waiting::DiskChanged);
    }
}

fn saved(editor: &mut Editor, id: BufferId) {
    let Some(path) = editor
        .buffer_by_id(id)
        .and_then(|buffer| buffer.file())
        .map(|file| file.path.clone())
    else {
        return;
    };
    if let Some(hooks) = hooks(editor) {
        hooks.waiting.push(Waiting::Saved(path));
    }
}

pub fn watch(editor: &mut Editor) {
    hooks(editor);
    editor.on_disk_change(disk_changed);
    editor.on_saved(saved);
}

pub fn open_directories(editor: &mut Editor, plugin: &str) {
    if let Some(hooks) = hooks(editor) {
        hooks.opener = Some(plugin.to_owned());
    }
    editor.on_open_directory(open_directory);
}

pub fn run(isolate: &mut Isolate, editor: &mut Editor) {
    let waiting = hooks(editor)
        .map(|hooks| std::mem::take(&mut hooks.waiting))
        .unwrap_or_default();
    for event in waiting {
        let (handlers, arguments) = match event {
            Waiting::OpenDirectory(path) => {
                let handler = editor
                    .plugin_state::<Hooks>()
                    .and_then(|hooks| hooks.directory);
                let Ok(path) = isolate.heap_mut().string(&path.display().to_string()) else {
                    continue;
                };
                (handler.into_iter().collect::<Vec<_>>(), vec![path])
            }
            Waiting::DiskChanged => (
                editor
                    .plugin_state::<Hooks>()
                    .map(|hooks| hooks.disk.iter().map(|(_, hold)| *hold).collect())
                    .unwrap_or_default(),
                Vec::new(),
            ),
            Waiting::Saved(path) => {
                let Ok(path) = isolate.heap_mut().string(&path.display().to_string()) else {
                    continue;
                };
                (
                    editor
                        .plugin_state::<Hooks>()
                        .map(|hooks| hooks.saved.iter().map(|(_, hold)| *hold).collect())
                        .unwrap_or_default(),
                    vec![path],
                )
            }
        };
        for hold in handlers {
            let Some(handler) = isolate.held(hold) else {
                continue;
            };
            if let Err(error) = isolate.spawn(handler, &arguments) {
                log::report(editor, &error, None);
            }
        }
    }
}

fn path_of(heap: &Heap, name: &str, value: Value) -> Result<PathBuf, Error> {
    heap.text(value).map(PathBuf::from).ok_or_else(|| {
        Error::new(format!(
            "{name} expects a path, got {}",
            print::write(heap, value)
        ))
    })
}

fn shown(heap: &mut Heap, path: Option<&Path>) -> Result<Value, Error> {
    match path {
        Some(path) => heap
            .string(&path.display().to_string())
            .map_err(|full| Error::new(full.to_string())),
        None => Ok(Value::Nil),
    }
}

fn on_open_directory(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) {
    let hold = heap.hold(arguments[0]);
    if let Some(old) = hooks(editor).and_then(|hooks| hooks.directory.replace(hold)) {
        heap.release(old);
    }
}

fn keyed(heap: &mut Heap, handlers: &mut Vec<(String, Hold)>, arguments: &[Value]) {
    let key = print::display(heap, arguments[0]);
    let hold = heap.hold(arguments[1]);
    match handlers.iter_mut().find(|(known, _)| *known == key) {
        Some((_, existing)) => {
            let old = std::mem::replace(existing, hold);
            heap.release(old);
        }
        None => handlers.push((key, hold)),
    }
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>(
        "on-open-directory",
        1,
        Some(1),
        |heap, editor, arguments| {
            on_open_directory(heap, editor, arguments);
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native::<Editor>(
        "on-disk-change",
        2,
        Some(2),
        |heap, editor, arguments| {
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.disk, arguments);
            }
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native::<Editor>("on-saved", 2, Some(2), |heap, editor, arguments| {
        if let Some(hooks) = hooks(editor) {
            keyed(heap, &mut hooks.saved, arguments);
        }
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("open-file", 1, Some(1), |heap, editor, arguments| {
        let path = path_of(heap, "open-file", arguments[0])?;
        Ok(Value::Bool(editor.open_file(&path)))
    });
    isolate.define_host_native::<Editor>(
        "open-externally",
        1,
        Some(1),
        |heap, editor, arguments| {
            let path = path_of(heap, "open-externally", arguments[0])?;
            editor.open_externally(path);
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native::<Editor>("buffer-file", 0, Some(0), |heap, editor, _| {
        let path = editor.buffer().file().map(|file| file.path.clone());
        shown(heap, path.as_deref())
    });
    isolate.define_host_native::<Editor>("default-directory", 0, Some(0), |heap, editor, _| {
        let directory = editor.default_directory();
        shown(heap, Some(&directory))
    });
    isolate.define_host_native::<Editor>("close-buffer", 1, Some(1), |heap, editor, arguments| {
        let name = print::display(heap, arguments[0]);
        if let Some(id) = editor.buffer_named(&name) {
            editor.close_buffer(id);
        }
        Ok(Value::Nil)
    });
}
#[cfg(test)]
mod tests {
    use std::path::Path;

    use oxmacs_core::editor::{Editor, Effect};
    use oxmacs_core::input::KeySequence;

    use crate::{Bundle, Ox};

    const LISTER: Bundle = Bundle {
        name: "lister",
        files: &[
            (
                "plugin.ox",
                r#"(plugin lister
  :commands {list-dir: "List a directory"}
  :keys {"C-x l" list-dir}
  :start-screen {list-dir: "List a directory"}
  :opens-directories true)"#,
            ),
            (
                "lister.ox",
                r#"(module (import (ox path)))
(defcommand list-dir "List a directory" (message "listing"))
(on-open-directory (fn [dir] (message "opened {(path/name dir)}")))"#,
            ),
        ],
    };

    fn started(home: &Path) -> (Editor, Ox) {
        let mut editor = Editor::new();
        let mut ox = Ox::with_bundled(&[LISTER]);
        ox.start(&mut editor, Some(home));
        (editor, ox)
    }

    fn press(editor: &mut Editor, ox: &mut Ox, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
        ox.tick(editor);
    }

    #[test]
    fn manifests_bind_global_keys_and_add_start_screen_entries() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = started(home.path());

        editor.about();
        let splash = editor.buffer().contents();
        assert!(splash.contains("List a directory"), "{splash}");
        assert!(splash.contains("C-x l"), "{splash}");

        press(&mut editor, &mut ox, "C-x l");
        assert_eq!(editor.message(), Some("listing"));
    }

    #[test]
    fn opening_a_directory_wakes_the_plugin_that_opens_them() {
        let home = tempfile::tempdir().unwrap();
        let folder = home.path().join("projects");
        std::fs::create_dir(&folder).unwrap();
        let (mut editor, mut ox) = started(home.path());

        assert!(editor.open_file(&folder));
        ox.tick(&mut editor);

        assert_eq!(editor.message(), Some("opened projects"));
    }

    #[test]
    fn disk_changes_reach_their_handlers() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = started(home.path());
        ox.eval(&mut editor, r"(def seen (atom 0)) (on-disk-change :count (fn [] (swap! seen inc))) (on-disk-change :count (fn [] (swap! seen + 10)))")
            .unwrap();

        editor.check_disk_changes();
        ox.tick(&mut editor);

        assert_eq!(ox.eval(&mut editor, "@seen").unwrap(), "10");
    }

    #[test]
    fn saves_reach_their_handlers_with_the_path() {
        let home = tempfile::tempdir().unwrap();
        let file = home.path().join("notes.txt");
        std::fs::write(&file, "hello").unwrap();
        let (mut editor, mut ox) = started(home.path());
        ox.eval(
            &mut editor,
            "(module (import (ox path))) (def seen (atom nil)) (on-saved :seen (fn [path] (reset! seen (path/name path))))",
        )
        .unwrap();
        editor.open_file(&file);

        press(&mut editor, &mut ox, "x C-x C-s");

        assert_eq!(ox.eval(&mut editor, "@seen").unwrap(), r#""notes.txt""#);
    }

    #[test]
    fn plugins_open_files_and_ask_about_buffers() {
        let home = tempfile::tempdir().unwrap();
        let file = home.path().join("notes.txt");
        std::fs::write(&file, "hello").unwrap();
        let (mut editor, mut ox) = started(home.path());
        let path = format!("{:?}", file.display().to_string());

        ox.eval(&mut editor, &format!("(open-file {path})"))
            .unwrap();
        assert_eq!(editor.buffer().contents(), "hello");
        let folder = format!("{:?}", home.path().display().to_string());
        assert_eq!(
            ox.eval(&mut editor, &format!("(= (default-directory) {folder})"))
                .unwrap(),
            "true"
        );
        assert_eq!(
            ox.eval(&mut editor, &format!("(= (buffer-file) {path})"))
                .unwrap(),
            "true"
        );
        ox.eval(&mut editor, &format!("(open-externally {path})"))
            .unwrap();
        assert!(
            editor
                .take_effects()
                .contains(&Effect::OpenExternally(file.clone()))
        );
        ox.eval(
            &mut editor,
            r#"(open-buffer "*gone*" ["x"]) (close-buffer "*gone*")"#,
        )
        .unwrap();
        assert_eq!(editor.buffer_named("*gone*"), None);
    }
}
