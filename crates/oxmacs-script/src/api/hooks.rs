use std::path::{Path, PathBuf};

use ox_lang::heap::{Heap, Hold};
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Editor, Event, EventKind};
use oxmacs_core::text::{Buffer, Splice};

use crate::api::handlers::{self, Hook};
use crate::api::{buffers, games, handles, items, lines};
use crate::ide::{log, repl};
use crate::{PATIENT, runtime::plugins};

#[derive(Debug, Default)]
struct Waiting(Vec<Event>);

const EVENTS: [(&str, Hook, EventKind); 10] = [
    ("on-disk-change", Hook::DiskChange, EventKind::DiskChanged),
    ("on-saved", Hook::Saved, EventKind::Saved),
    (
        "on-settings-change",
        Hook::SettingsChange,
        EventKind::Configured,
    ),
    (
        "on-processes-change",
        Hook::ProcessesChange,
        EventKind::Processes,
    ),
    ("on-buffer-open", Hook::BufferOpen, EventKind::Opened),
    ("on-buffer-close", Hook::BufferClose, EventKind::Killed),
    ("on-buffer-change", Hook::BufferChange, EventKind::Edited),
    ("on-mode-enter", Hook::ModeEnter, EventKind::ModeChanged),
    ("on-window-change", Hook::WindowChange, EventKind::Windows),
    (
        "on-diagnostics-change",
        Hook::DiagnosticsChange,
        EventKind::Diagnostics,
    ),
];

pub fn collect(editor: &mut Editor) {
    let events = editor.take_events();
    if events.is_empty() {
        return;
    }
    for event in &events {
        match event {
            Event::Saved(id) => {
                games::saved(editor, *id);
                repl::saved(editor, *id);
            }
            Event::OpenFile { owner, .. } | Event::OpenDirectory { owner, .. } => {
                plugins::request_wake(editor, owner);
            }
            _ => {}
        }
    }
    if editor.plugin_state::<Waiting>().is_none() {
        editor.set_plugin_state(Waiting::default());
    }
    if let Some(waiting) = editor.plugin_state_mut::<Waiting>() {
        waiting.0.extend(events);
    }
}

pub fn waiting(editor: &mut Editor) -> bool {
    editor.events_waiting()
        || editor
            .plugin_state::<Waiting>()
            .is_some_and(|waiting| !waiting.0.is_empty())
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default()
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
    heap.vector_from(&items).map_err(Error::from)
}

fn holds(editor: &Editor, event: &Event) -> Vec<Hold> {
    let hook = match event {
        Event::OpenFile { path, .. } => {
            return handlers::held_for(editor, Hook::OpenFile, &extension_of(path))
                .into_iter()
                .collect();
        }
        Event::OpenDirectory { .. } => Hook::OpenDirectory,
        Event::DiskChanged => Hook::DiskChange,
        Event::Saved(_) => Hook::Saved,
        Event::Configured => Hook::SettingsChange,
        Event::Processes => Hook::ProcessesChange,
        Event::Opened(_) => Hook::BufferOpen,
        Event::Killed(_) => Hook::BufferClose,
        Event::Edited(..) => Hook::BufferChange,
        Event::ModeChanged(..) => Hook::ModeEnter,
        Event::Windows => Hook::WindowChange,
        Event::Diagnostics => Hook::DiagnosticsChange,
        Event::Clicked { .. }
        | Event::Dropped { .. }
        | Event::Typed { .. }
        | Event::Link { .. }
        | Event::Items(_)
        | Event::ItemChosen { .. } => return Vec::new(),
    };
    handlers::held(editor, hook)
}

fn dispatch(isolate: &mut Isolate, editor: &mut Editor, event: Event) -> Result<(), Error> {
    let holds = holds(editor, &event);
    let heap = isolate.heap_mut();
    if let Event::Killed(id) = event {
        handles::release_values(heap, editor, id);
        lines::release(heap, editor, id);
        items::release(heap, editor, id);
    }
    let arguments = match event {
        Event::Killed(id) if holds.is_empty() => {
            handles::forget(editor, id);
            return Ok(());
        }
        _ if holds.is_empty() => return Ok(()),
        Event::Opened(id) | Event::Edited(id, _) | Event::ModeChanged(id, _)
            if editor.buffer_by_id(id).is_none() =>
        {
            return Ok(());
        }
        Event::OpenFile { path, .. } | Event::OpenDirectory { path, .. } => {
            vec![shown(heap, Some(&path))?]
        }
        Event::Saved(id) => {
            let path = editor
                .buffer_by_id(id)
                .and_then(Buffer::file)
                .map(|file| file.path.clone());
            vec![shown(heap, path.as_deref())?]
        }
        Event::Opened(id) => vec![handles::handle(heap, editor, id)?],
        Event::Killed(id) => vec![handles::closed(heap, editor, id)?],
        Event::Edited(id, splices) => {
            vec![handles::handle(heap, editor, id)?, edits(heap, &splices)?]
        }
        Event::ModeChanged(id, mode) => {
            vec![handles::handle(heap, editor, id)?, heap.string(&mode)?]
        }
        _ => Vec::new(),
    };
    handlers::spawn(isolate, editor, &holds, &arguments);
    Ok(())
}

pub fn run(isolate: &mut Isolate, editor: &mut Editor) {
    let waiting = editor
        .plugin_state_mut::<Waiting>()
        .map(|waiting| std::mem::take(&mut waiting.0))
        .unwrap_or_default();
    for event in waiting {
        if matches!(
            event,
            Event::Clicked { .. }
                | Event::Dropped { .. }
                | Event::Typed { .. }
                | Event::Link { .. }
        ) {
            buffers::hear(isolate, editor, event);
        } else if matches!(event, Event::Items(_) | Event::ItemChosen { .. }) {
            items::hear(isolate, editor, &event);
        } else if let Err(error) = dispatch(isolate, editor, event) {
            log::report(editor, &error, None);
        }
    }
}

pub fn release_saves(isolate: &mut Isolate, editor: &mut Editor) {
    let saves = editor.held_saves();
    if saves.is_empty() {
        return;
    }
    let handlers = handlers::held(editor, Hook::BeforeSave);
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

fn path_of(heap: &Heap, name: &str, value: Value) -> Result<PathBuf, Error> {
    heap.text(value).map(PathBuf::from).ok_or_else(|| {
        Error::new(format!(
            "{name} expects a path, got {}",
            print::brief(heap, value)
        ))
    })
}

fn folder_given(
    heap: &Heap,
    editor: &Editor,
    native: &str,
    value: Option<Value>,
) -> Result<PathBuf, Error> {
    match value {
        None | Some(Value::Nil) => Ok(editor.default_directory()),
        Some(value) => heap.text(value).map(PathBuf::from).ok_or_else(|| {
            Error::new(format!(
                "{native} expects a folder, got {}",
                print::brief(heap, value)
            ))
        }),
    }
}

fn project_root(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let folder = folder_given(heap, editor, "project-root", arguments.first().copied())?;
    shown(heap, editor.project_root(&folder).as_deref())
}

fn language_root(
    heap: &mut Heap,
    editor: &mut Editor,
    arguments: &[Value],
) -> Result<Value, Error> {
    let folder = folder_given(heap, editor, "language-root", arguments.first().copied())?;
    let language = match arguments.get(1) {
        None | Some(Value::Nil) => editor.language(),
        Some(mode) => {
            let name = print::display(heap, *mode);
            let known = editor.language_named(&name);
            Some(known.ok_or_else(|| Error::new(format!("language-root knows no mode {name}")))?)
        }
    };
    shown(heap, editor.language_root(&folder, language).as_deref())
}

fn shown(heap: &mut Heap, path: Option<&Path>) -> Result<Value, Error> {
    match path {
        Some(path) => heap
            .string(&path.display().to_string())
            .map_err(Error::from),
        None => Ok(Value::Nil),
    }
}

fn diagnostic_counts(heap: &mut Heap, editor: &mut Editor, _: &[Value]) -> Result<Value, Error> {
    let count = |number: usize| Value::Int(i64::try_from(number).unwrap_or(i64::MAX));
    let mut items = Vec::new();
    for (path, errors, warnings) in editor.diagnostic_counts() {
        items.push(heap.string(&path.display().to_string())?);
        items.push(heap.map_from(&[
            Value::keyword("errors"),
            count(errors),
            Value::keyword("warnings"),
            count(warnings),
        ])?);
    }
    heap.map_from(&items)
}

fn install_handlers(isolate: &mut Isolate) {
    for (name, hook, kind) in EVENTS {
        isolate.native("ox", name).arity(2, Some(2)).host::<Editor>(
            move |heap, editor, arguments| {
                editor.subscribe(kind);
                let key = handlers::key(heap, arguments[0]);
                handlers::register(heap, editor, hook, key, arguments[1]);
                Ok(Value::Nil)
            },
        );
    }
    isolate
        .native("ox", "on-open-directory")
        .arity(1, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            handlers::register(
                heap,
                editor,
                Hook::OpenDirectory,
                String::new(),
                arguments[0],
            );
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "on-open-file")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            let extension = handlers::key(heap, arguments[0]).to_lowercase();
            handlers::register(heap, editor, Hook::OpenFile, extension, arguments[1]);
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "on-before-save")
        .arity(2, Some(2))
        .host::<Editor>(|heap, editor, arguments| {
            editor.hold_saves();
            let key = handlers::key(heap, arguments[0]);
            handlers::register(heap, editor, Hook::BeforeSave, key, arguments[1]);
            Ok(Value::Nil)
        });
}

pub fn install(isolate: &mut Isolate) {
    install_handlers(isolate);
    isolate
        .native("ox", "diagnostic-counts")
        .arity(0, Some(0))
        .host::<Editor>(diagnostic_counts);
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
        .native("ox", "project-root")
        .arity(0, Some(1))
        .host::<Editor>(project_root);
    isolate
        .native("ox", "language-root")
        .arity(0, Some(2))
        .host::<Editor>(language_root);
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
  :commands [list-dir]
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
        oxmacs_core::testing::add_kits(&mut editor);

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
    fn closing_a_buffer_reaches_its_handlers_with_the_handle_it_opened_with() {
        let (_home, mut editor, mut ox) = with_init(
            r"(def opened (atom nil)) (def closed (atom nil))
(on-buffer-open :test (fn [b] (reset! opened b)))
(on-buffer-close :test (fn [b] (reset! closed (= b @opened))))",
        );

        ox.eval(&mut editor, r#"(show-text "*note*" "x")"#).unwrap();
        settle(&mut editor, &mut ox);
        ox.eval(&mut editor, r#"(close-buffer "*note*")"#).unwrap();
        settle(&mut editor, &mut ox);

        assert_eq!(ox.eval(&mut editor, "@closed").unwrap(), "true");
    }

    #[test]
    fn buffers_closed_before_their_opening_is_heard_are_skipped_quietly() {
        let (_home, mut editor, mut ox) =
            with_init(r"(def heard (atom 0)) (on-buffer-open :test (fn [b] (swap! heard inc)))");

        ox.eval(
            &mut editor,
            r#"(show-text "*brief*" "x") (close-buffer "*brief*")"#,
        )
        .unwrap();
        settle(&mut editor, &mut ox);

        assert_eq!(ox.eval(&mut editor, "@heard").unwrap(), "0");
        assert_eq!(editor.message(), None);
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
    #[test]
    fn projects_and_languages_have_their_own_roots() {
        let dir = tempfile::tempdir().unwrap();
        let code = dir.path().join("repo").join("pkg").join("R");
        std::fs::create_dir_all(dir.path().join("repo").join(".git")).unwrap();
        std::fs::create_dir_all(&code).unwrap();
        std::fs::write(dir.path().join("repo").join("pkg").join("DESCRIPTION"), "").unwrap();
        let mut editor = oxmacs_core::testing::kits();
        editor.set_home_directory(Some(dir.path().to_owned()));
        let mut ox = Ox::new();
        let quoted = |path: &Path| format!("{:?}", path.display().to_string());
        let (folder, repo, package) = (
            quoted(&code),
            quoted(&dir.path().join("repo")),
            quoted(&dir.path().join("repo").join("pkg")),
        );

        let found = ox
            .eval(
                &mut editor,
                &format!(
                    "[(= (project-root {folder}) {repo}) (= (language-root {folder} \"R\") {package}) (= (language-root {folder} \"TOML\") {repo}) (project-root {:?})]",
                    dir.path().display().to_string()
                ),
            )
            .unwrap();

        assert_eq!(found, "[true true true nil]");
    }
}
