use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ox::heap::{Heap, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::editor::Editor;
use oxmacs_core::text::{BufferId, Splice};

use crate::{PATIENT, handles, log, plugins};

#[derive(Debug, PartialEq)]
enum Waiting {
    OpenDirectory(PathBuf),
    DiskChanged,
    Saved(PathBuf),
    Configured,
    Processes,
    Opened(BufferId),
    Changed(BufferId, Vec<Splice>),
    Mode(BufferId, String),
}

#[derive(Debug, Default)]
pub struct Hooks {
    directory: Option<Hold>,
    opener: Option<String>,
    disk: Vec<(String, Hold)>,
    saved: Vec<(String, Hold)>,
    configured: Vec<(String, Hold)>,
    processes: Vec<(String, Hold)>,
    opened: Vec<(String, Hold)>,
    changed: Vec<(String, Hold)>,
    modes: Vec<(String, Hold)>,
    before_save: Vec<(String, Hold)>,
    languages: HashMap<BufferId, String>,
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

pub fn settings_changed(editor: &mut Editor) {
    if let Some(hooks) = hooks(editor)
        && !hooks.waiting.contains(&Waiting::Configured)
    {
        hooks.waiting.push(Waiting::Configured);
    }
}

fn processes_changed(editor: &mut Editor) {
    if let Some(hooks) = hooks(editor)
        && !hooks.processes.is_empty()
        && !hooks.waiting.contains(&Waiting::Processes)
    {
        hooks.waiting.push(Waiting::Processes);
    }
}

pub fn watch(editor: &mut Editor) {
    hooks(editor);
    editor.on_disk_change(disk_changed);
    editor.on_saved(saved);
    editor.on_configured(settings_changed);
    editor.on_processes_change(processes_changed);
    editor.on_buffer_opened(opened);
}

fn opened(editor: &mut Editor, id: BufferId) {
    if let Some(hooks) = hooks(editor)
        && !hooks.opened.is_empty()
    {
        hooks.waiting.push(Waiting::Opened(id));
    }
}

fn languages(editor: &Editor) -> Vec<(BufferId, String)> {
    editor
        .buffer_ids()
        .into_iter()
        .filter_map(|id| {
            editor
                .buffer_language(id)
                .map(|language| (id, language.name().to_owned()))
        })
        .collect()
}

fn notice(editor: &mut Editor) {
    let Some(known) = editor.plugin_state::<Hooks>() else {
        return;
    };
    let watching_modes = !known.modes.is_empty();
    let edits = if known.changed.is_empty() {
        Vec::new()
    } else {
        editor.take_edits()
    };
    let modes = if watching_modes {
        languages(editor)
    } else {
        Vec::new()
    };
    let Some(hooks) = hooks(editor) else {
        return;
    };
    for (id, splices) in edits {
        hooks.waiting.push(Waiting::Changed(id, splices));
    }
    for (id, mode) in modes {
        if hooks.languages.get(&id) != Some(&mode) {
            hooks.languages.insert(id, mode.clone());
            hooks.waiting.push(Waiting::Mode(id, mode));
        }
    }
}

fn edits(heap: &mut Heap, splices: &[Splice]) -> Result<Value, Error> {
    let count = |number: usize| Value::Int(i64::try_from(number).unwrap_or(i64::MAX));
    let mut items = Vec::with_capacity(splices.len());
    for splice in splices {
        items.push(heap.map_from(&[
            Value::keyword("start"),
            count(splice.start),
            Value::keyword("removed"),
            count(splice.removed),
            Value::keyword("inserted"),
            count(splice.inserted),
        ])?);
    }
    heap.vector_from(&items)
        .map_err(|full| Error::new(full.to_string()))
}

fn held(editor: &Editor, pick: fn(&Hooks) -> &Vec<(String, Hold)>) -> Vec<Hold> {
    editor
        .plugin_state::<Hooks>()
        .map(|hooks| pick(hooks).iter().map(|(_, hold)| *hold).collect())
        .unwrap_or_default()
}

pub fn release_saves(isolate: &mut Isolate, editor: &mut Editor) {
    let saves = editor.held_saves();
    if saves.is_empty() {
        return;
    }
    let handlers = held(editor, |hooks| &hooks.before_save);
    for id in saves {
        let mut failed = None;
        for hold in &handlers {
            let Some(function) = isolate.held(*hold) else {
                continue;
            };
            let Ok(buffer) = handles::handle(isolate.heap_mut(), editor, id) else {
                break;
            };
            if let Err(error) = isolate.within(PATIENT, |isolate| {
                isolate.call_with(editor, function, &[buffer])
            }) {
                failed = Some(error);
                break;
            }
        }
        match failed {
            Some(error) => {
                editor.drop_held_save(id);
                log::report(editor, &error, None);
            }
            None => {
                editor.finish_held_save(id);
            }
        }
    }
}

pub fn open_directories(editor: &mut Editor, plugin: &str) {
    if let Some(hooks) = hooks(editor) {
        hooks.opener = Some(plugin.to_owned());
    }
    editor.on_open_directory(open_directory);
}

pub fn run(isolate: &mut Isolate, editor: &mut Editor) {
    notice(editor);
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
            Waiting::Configured => (
                editor
                    .plugin_state::<Hooks>()
                    .map(|hooks| hooks.configured.iter().map(|(_, hold)| *hold).collect())
                    .unwrap_or_default(),
                Vec::new(),
            ),
            Waiting::Processes => (
                editor
                    .plugin_state::<Hooks>()
                    .map(|hooks| hooks.processes.iter().map(|(_, hold)| *hold).collect())
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
            Waiting::Opened(id) => {
                let Ok(buffer) = handles::handle(isolate.heap_mut(), editor, id) else {
                    continue;
                };
                (held(editor, |hooks| &hooks.opened), vec![buffer])
            }
            Waiting::Changed(id, splices) => {
                let Ok(buffer) = handles::handle(isolate.heap_mut(), editor, id) else {
                    continue;
                };
                let Ok(changes) = edits(isolate.heap_mut(), &splices) else {
                    continue;
                };
                (held(editor, |hooks| &hooks.changed), vec![buffer, changes])
            }
            Waiting::Mode(id, mode) => {
                let Ok(buffer) = handles::handle(isolate.heap_mut(), editor, id) else {
                    continue;
                };
                let Ok(mode) = isolate.heap_mut().string(&mode) else {
                    continue;
                };
                (held(editor, |hooks| &hooks.modes), vec![buffer, mode])
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
            print::brief(heap, value)
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

fn install_buffer_hooks(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>(
        "on-buffer-open",
        2,
        Some(2),
        |heap, editor, arguments| {
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.opened, arguments);
            }
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native::<Editor>(
        "on-buffer-change",
        2,
        Some(2),
        |heap, editor, arguments| {
            editor.track_edits();
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.changed, arguments);
            }
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native::<Editor>("on-mode-enter", 2, Some(2), |heap, editor, arguments| {
        let known = languages(editor);
        if let Some(hooks) = hooks(editor) {
            if hooks.modes.is_empty() {
                hooks.languages = known.into_iter().collect();
            }
            keyed(heap, &mut hooks.modes, arguments);
        }
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>(
        "on-before-save",
        2,
        Some(2),
        |heap, editor, arguments| {
            editor.hold_saves();
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.before_save, arguments);
            }
            Ok(Value::Nil)
        },
    );
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
    isolate.define_host_native::<Editor>(
        "on-settings-change",
        2,
        Some(2),
        |heap, editor, arguments| {
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.configured, arguments);
            }
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native::<Editor>(
        "on-processes-change",
        2,
        Some(2),
        |heap, editor, arguments| {
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.processes, arguments);
            }
            Ok(Value::Nil)
        },
    );
    install_buffer_hooks(isolate);
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
        if let Ok(id) = handles::target(heap, editor, "close-buffer", arguments[0]) {
            editor.close_buffer(id);
        }
        Ok(Value::Nil)
    });
}
#[cfg(test)]
mod tests {
    use std::path::Path;

    use oxmacs_core::editor::{Editor, Effect};

    use crate::testing::{press, settle};
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

    fn with_init(init: &str) -> (tempfile::TempDir, Editor, Ox) {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("init.ox"), init).unwrap();
        let (editor, ox) = started(home.path());
        (home, editor, ox)
    }

    #[test]
    fn buffer_hooks_hear_text_buffers_open_change_and_enter_modes() {
        let (home, mut editor, mut ox) = with_init(
            r"(def heard (atom []))
(on-buffer-open :test (fn [b] (swap! heard conj [:open (with-buffer b (buffer-name))])))
(on-mode-enter :test (fn [b mode] (swap! heard conj [:mode mode])))
(on-buffer-change :test (fn [b edits] (let [e (first edits)] (swap! heard conj [:change (with-buffer b (buffer-name)) e.start e.removed e.inserted (count edits)]))))",
        );
        let path = home.path().join("notes.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();

        editor.open_file(&path);
        settle(&mut editor, &mut ox);
        press(&mut editor, &mut ox, "x");
        ox.eval(&mut editor, r#"(open-buffer "*list*" ["a"])"#)
            .unwrap();
        settle(&mut editor, &mut ox);

        let mode = editor
            .buffer_language(editor.buffer_named("notes.rs").unwrap())
            .unwrap()
            .name()
            .to_owned();
        assert_eq!(
            ox.eval(&mut editor, "@heard").unwrap(),
            format!(r#"[[:open "notes.rs"] [:mode "{mode}"] [:change "notes.rs" 0 0 1 1]]"#)
        );
    }

    #[test]
    fn saving_waits_for_before_save_handlers() {
        let (home, mut editor, mut ox) = with_init(
            r"(module (import (ox string)))
(on-before-save :shout (fn [b] (with-buffer b (set-buffer-text (string/upper (buffer-text))))))",
        );
        let path = home.path().join("notes.txt");
        std::fs::write(&path, "abc").unwrap();
        editor.open_file(&path);

        press(&mut editor, &mut ox, "x C-x C-s");

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "XABC");
        assert!(!editor.buffer().is_modified());
    }

    #[test]
    fn a_failing_before_save_handler_cancels_the_save() {
        let (home, mut editor, mut ox) =
            with_init(r#"(on-before-save :refuse (fn [b] (raise (error "not today"))))"#);
        let path = home.path().join("notes.txt");
        std::fs::write(&path, "abc").unwrap();
        editor.open_file(&path);

        press(&mut editor, &mut ox, "x C-x C-s");

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "abc");
        assert_eq!(editor.message(), Some("Ox: not today"));
        assert!(editor.buffer().is_modified());
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
        settle(&mut editor, &mut ox);

        assert_eq!(editor.message(), Some("opened projects"));
    }

    #[test]
    fn disk_changes_reach_their_handlers() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = started(home.path());
        ox.eval(&mut editor, r"(def seen (atom 0)) (on-disk-change :count (fn [] (swap! seen inc))) (on-disk-change :count (fn [] (swap! seen + 10)))")
            .unwrap();

        editor.check_disk_changes();
        settle(&mut editor, &mut ox);

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
    fn settings_changes_reach_their_handlers_once_per_tick() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = started(home.path());
        ox.eval(
            &mut editor,
            "(def seen (atom 0)) (on-settings-change :count (fn [] (swap! seen inc)))",
        )
        .unwrap();

        ox.eval(
            &mut editor,
            "(settings {tab-width: 4}) (settings {clock: false})",
        )
        .unwrap();
        ox.tick(&mut editor);

        assert_eq!(ox.eval(&mut editor, "@seen").unwrap(), "1");
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
