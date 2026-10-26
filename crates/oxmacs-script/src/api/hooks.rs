use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ox_lang::heap::{Heap, Hold};
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Arrangement, Editor, WindowId};
use oxmacs_core::text::{BufferId, Splice};

use crate::{PATIENT, api::handles, ide::log, runtime::plugins};

#[derive(Debug, PartialEq)]
enum Waiting {
    OpenDirectory(PathBuf),
    OpenFile(PathBuf, String),
    DiskChanged,
    Saved(PathBuf),
    Configured,
    Processes,
    Opened(BufferId),
    Changed(BufferId, Vec<Splice>),
    Mode(BufferId, String),
    Windows,
    Diagnostics,
}

type Seen = (WindowId, BufferId, Arrangement);

fn windows_seen(editor: &Editor) -> Seen {
    (
        editor.selected_window(),
        editor.buffer().id(),
        editor.window_arrangement(),
    )
}

#[derive(Debug, Default)]
pub struct Hooks {
    file_types: Vec<(String, Hold)>,
    claimed_types: Vec<(String, String)>,
    windows: Vec<(String, Hold)>,
    seen_windows: Option<Seen>,
    diagnostics: Vec<(String, Hold)>,
    seen_diagnostics: u64,
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

fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_lowercase)
}

fn open_typed_file(editor: &mut Editor, path: &Path) -> bool {
    let Some(extension) = extension_of(path) else {
        return false;
    };
    let Some(plugin) = editor.plugin_state::<Hooks>().and_then(|hooks| {
        hooks
            .claimed_types
            .iter()
            .find(|(known, _)| *known == extension)
            .map(|(_, plugin)| plugin.clone())
    }) else {
        return false;
    };
    plugins::request_wake(editor, &plugin);
    if let Some(hooks) = hooks(editor) {
        hooks
            .waiting
            .push(Waiting::OpenFile(path.to_path_buf(), extension));
    }
    true
}

pub fn open_file_types(editor: &mut Editor, plugin: &str, extensions: &[String]) {
    for extension in extensions {
        let extension = extension.to_lowercase();
        if let Some(hooks) = hooks(editor) {
            hooks.claimed_types.retain(|(known, _)| *known != extension);
            hooks
                .claimed_types
                .push((extension.clone(), plugin.to_owned()));
        }
        editor.on_open_file_type(&extension, open_typed_file);
    }
}

fn disk_changed(editor: &mut Editor) {
    if let Some(hooks) = hooks(editor)
        && !hooks.waiting.contains(&Waiting::DiskChanged)
    {
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
    let windows = (!known.windows.is_empty())
        .then(|| windows_seen(editor))
        .filter(|now| known.seen_windows.as_ref() != Some(now));
    let revision = editor.diagnostics_revision();
    let diagnosed = !known.diagnostics.is_empty() && known.seen_diagnostics != revision;
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
    if let Some(now) = windows {
        hooks.seen_windows = Some(now);
        hooks.waiting.push(Waiting::Windows);
    }
    if diagnosed {
        hooks.seen_diagnostics = revision;
        hooks.waiting.push(Waiting::Diagnostics);
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

pub fn waiting(editor: &mut Editor) -> bool {
    notice(editor);
    editor
        .plugin_state::<Hooks>()
        .is_some_and(|hooks| !hooks.waiting.is_empty())
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
            Waiting::OpenFile(path, extension) => {
                let handler = editor.plugin_state::<Hooks>().and_then(|hooks| {
                    hooks
                        .file_types
                        .iter()
                        .find(|(known, _)| *known == extension)
                        .map(|(_, hold)| *hold)
                });
                let Ok(path) = isolate.heap_mut().string(&path.display().to_string()) else {
                    continue;
                };
                (handler.into_iter().collect(), vec![path])
            }
            Waiting::Windows => (held(editor, |hooks| &hooks.windows), Vec::new()),
            Waiting::Diagnostics => (held(editor, |hooks| &hooks.diagnostics), Vec::new()),
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

fn on_open_file(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) {
    let extension = print::display(heap, arguments[0]).to_lowercase();
    let hold = heap.hold(arguments[1]);
    let Some(hooks) = hooks(editor) else {
        heap.release(hold);
        return;
    };
    match hooks
        .file_types
        .iter_mut()
        .find(|(known, _)| *known == extension)
    {
        Some((_, existing)) => {
            let old = std::mem::replace(existing, hold);
            heap.release(old);
        }
        None => hooks.file_types.push((extension, hold)),
    }
}

fn on_open_directory(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) {
    let hold = heap.hold(arguments[0]);
    if let Some(old) = hooks(editor).and_then(|hooks| hooks.directory.replace(hold)) {
        heap.release(old);
    }
}

pub(crate) fn keyed(heap: &mut Heap, handlers: &mut Vec<(String, Hold)>, arguments: &[Value]) {
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

fn diagnostic_counts(heap: &mut Heap, editor: &mut Editor, _: &[Value]) -> Result<Value, Error> {
    let count = |number: usize| Value::Int(i64::try_from(number).unwrap_or(i64::MAX));
    let mut items = Vec::new();
    for (path, errors, warnings) in editor.diagnostic_counts() {
        items.push(
            heap.string(&path.display().to_string())
                .map_err(|full| Error::new(full.to_string()))?,
        );
        items.push(heap.map_from(&[
            Value::keyword("errors"),
            count(errors),
            Value::keyword("warnings"),
            count(warnings),
        ])?);
    }
    heap.map_from(&items)
}

fn install_buffer_hooks(isolate: &mut Isolate) {
    isolate
        .native("ox", "on-buffer-open")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.opened, arguments);
            }
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "on-buffer-change")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            editor.track_edits();
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.changed, arguments);
            }
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "on-mode-enter")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            let known = languages(editor);
            if let Some(hooks) = hooks(editor) {
                if hooks.modes.is_empty() {
                    hooks.languages = known.into_iter().collect();
                }
                keyed(heap, &mut hooks.modes, arguments);
            }
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "on-window-change")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            let now = windows_seen(editor);
            if let Some(hooks) = hooks(editor) {
                if hooks.windows.is_empty() {
                    hooks.seen_windows = Some(now);
                }
                keyed(heap, &mut hooks.windows, arguments);
            }
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "on-diagnostics-change")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            let revision = editor.diagnostics_revision();
            if let Some(hooks) = hooks(editor) {
                if hooks.diagnostics.is_empty() {
                    hooks.seen_diagnostics = revision;
                }
                keyed(heap, &mut hooks.diagnostics, arguments);
            }
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "diagnostic-counts")
        .arity(0, Some(0))
        .host::<Editor>(diagnostic_counts);
    isolate
        .native("ox", "on-before-save")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            editor.hold_saves();
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.before_save, arguments);
            }
            Ok(Value::Nil)
        });
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "on-open-directory")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            on_open_directory(heap, editor, arguments);
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "on-open-file")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            on_open_file(heap, editor, arguments);
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "on-disk-change")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.disk, arguments);
            }
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "on-saved")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.saved, arguments);
            }
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "on-settings-change")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.configured, arguments);
            }
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "on-processes-change")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            if let Some(hooks) = hooks(editor) {
                keyed(heap, &mut hooks.processes, arguments);
            }
            Ok(Value::Nil)
        });
    install_buffer_hooks(isolate);
    isolate
        .native("ox", "open-file")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            let path = path_of(heap, "open-file", arguments[0])?;
            Ok(Value::Bool(editor.open_file(&path)))
        });
    isolate
        .native("ox", "move-visited-files")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            let from = path_of(heap, "move-visited-files", arguments[0])?;
            let to = path_of(heap, "move-visited-files", arguments[1])?;
            editor.file_moved(&from, &to);
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "open-externally")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            let path = path_of(heap, "open-externally", arguments[0])?;
            editor.open_externally(path);
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "buffer-file")
        .arity(0, Some(0))
        .host::<Editor>(|heap, editor, _| {
            let path = editor.buffer().file().map(|file| file.path.clone());
            shown(heap, path.as_deref())
        });
    isolate
        .native("ox", "default-directory")
        .arity(0, Some(0))
        .host::<Editor>(|heap, editor, _| {
            let directory = editor.default_directory();
            shown(heap, Some(&directory))
        });
    isolate
        .native("ox", "close-buffer")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
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
    fn window_changes_reach_their_handlers_once_each() {
        let (_home, mut editor, mut ox) =
            with_init(r"(def seen (atom 0)) (on-window-change :count (fn [] (swap! seen inc)))");
        let seen = |editor: &mut Editor, ox: &mut Ox| ox.eval(editor, "@seen").unwrap();

        press(&mut editor, &mut ox, "x");
        assert_eq!(seen(&mut editor, &mut ox), "0");
        press(&mut editor, &mut ox, "C-x 2");
        assert_eq!(seen(&mut editor, &mut ox), "1");
        press(&mut editor, &mut ox, "C-x o");
        assert_eq!(seen(&mut editor, &mut ox), "2");
        press(&mut editor, &mut ox, "C-x b o t h e r RET");
        assert_eq!(seen(&mut editor, &mut ox), "3");
    }

    #[test]
    fn window_changes_made_by_ox_commands_are_heard_in_the_same_tick() {
        let (_home, mut editor, mut ox) = with_init(
            r#"(def seen (atom 0)) (on-window-change :count (fn [] (swap! seen inc)))
(defcommand other "Show another buffer" (show-text "*other*" "x"))"#,
        );

        let id = editor.command_named("other").unwrap();
        editor.execute(id);
        assert_eq!(ox.tick(&mut editor), crate::Pace::Running);
        ox.tick(&mut editor);

        assert_eq!(ox.eval(&mut editor, "@seen").unwrap(), "1");
    }

    #[test]
    fn diagnostics_are_counted_per_file_and_changes_reach_their_handlers() {
        use oxmacs_core::external::diagnostics::{Diagnostic, Severity};

        let (home, mut editor, mut ox) = with_init(
            r"(def seen (atom 0)) (on-diagnostics-change :count (fn [] (swap! seen inc)))",
        );
        let path = home.path().join("main.rs");
        let found = |severity| Diagnostic {
            path: path.clone(),
            start: (1, 1),
            end: (1, 2),
            severity,
            message: "x".to_owned(),
        };

        editor.replace_diagnostics(
            &path,
            vec![found(Severity::Error), found(Severity::Warning)],
        );
        settle(&mut editor, &mut ox);

        assert_eq!(ox.eval(&mut editor, "@seen").unwrap(), "1");
        let counts = ox
            .eval(
                &mut editor,
                &format!("(get (diagnostic-counts) {:?})", path.display().to_string()),
            )
            .unwrap();
        assert_eq!(counts, "{errors: 1 warnings: 1}");
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

    const READER: Bundle = Bundle {
        name: "reader",
        files: &[
            (
                "plugin.ox",
                r#"(plugin reader :opens-files ["book" "story"])"#,
            ),
            (
                "reader.ox",
                r#"(module (import (ox path)))
(on-open-file "book" (fn [place] (message "reading {(path/name place)}")))"#,
            ),
        ],
    };

    #[test]
    fn opening_a_claimed_file_type_wakes_its_plugin_and_reaches_its_handler() {
        let home = tempfile::tempdir().unwrap();
        let book = home.path().join("tale.BOOK");
        std::fs::write(&book, "binary").unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::with_bundled(&[READER]);
        ox.start(&mut editor, Some(home.path()));

        assert!(editor.open_file(&book));
        settle(&mut editor, &mut ox);

        assert_eq!(editor.message(), Some("reading tale.BOOK"));
        assert_eq!(editor.buffer().name(), "*scratch*");
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
