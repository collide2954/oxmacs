use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ox::heap::{Heap, Hold};
use ox::symbol::Sym;
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::command::{DynamicSpec, Scope};
use oxmacs_core::editor::Editor;

use crate::{commands, hooks, log, modes, views};

#[derive(Debug, Clone, Copy)]
pub struct Bundle {
    pub name: &'static str,
    pub files: &'static [(&'static str, &'static str)],
}

#[derive(Debug, Clone)]
enum Origin {
    Bundled(&'static [(&'static str, &'static str)]),
    Folder(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Text,
    Int,
    Number,
    Flag,
    Choice(Vec<Sym>),
}

#[derive(Debug, Clone)]
struct Setting {
    key: String,
    kind: Kind,
    default: Option<Hold>,
}

#[derive(Debug, Clone)]
struct Manifest {
    name: String,
    commands: Vec<(String, String)>,
    settings: Vec<Setting>,
    startup_when: Option<String>,
    entry: String,
    keys: Vec<(String, String)>,
    start_screen: Vec<(String, String)>,
    opens_directories: bool,
    modes: Vec<modes::Declared>,
}

#[derive(Debug, Clone)]
struct Plugin {
    manifest: Manifest,
    origin: Origin,
    active: bool,
    loaded: bool,
    config: Vec<(String, Hold)>,
}

#[derive(Debug, Default)]
pub struct Plugins {
    known: Vec<Plugin>,
    waking: Vec<String>,
}

fn keyword(value: Value) -> Option<&'static str> {
    match value {
        Value::Keyword(name) => Some(name.name()),
        _ => None,
    }
}

fn commands_of(heap: &Heap, value: Value) -> Result<Vec<(String, String)>, String> {
    let wrong = || ":commands must be a map of names to summaries".to_owned();
    heap.entries(value)
        .ok_or_else(wrong)?
        .into_iter()
        .map(|(name, summary)| {
            let name = keyword(name).ok_or_else(wrong)?;
            let summary = heap.text(summary).ok_or_else(wrong)?;
            Ok((name.to_owned(), summary.to_owned()))
        })
        .collect()
}

fn pairs_of(heap: &Heap, value: Value, wrong: &str) -> Result<Vec<(String, String)>, String> {
    let text = |value: Value| match value {
        Value::Keyword(name) | Value::Symbol(name) => Some(name.name().to_owned()),
        _ => heap.text(value).map(str::to_owned),
    };
    heap.entries(value)
        .ok_or_else(|| wrong.to_owned())?
        .into_iter()
        .map(|(key, item)| {
            Ok((
                text(key).ok_or_else(|| wrong.to_owned())?,
                text(item).ok_or_else(|| wrong.to_owned())?,
            ))
        })
        .collect()
}

fn kind_of(heap: &Heap, value: Value) -> Result<Kind, String> {
    match keyword(value) {
        Some("string") => Ok(Kind::Text),
        Some("int") => Ok(Kind::Int),
        Some("number") => Ok(Kind::Number),
        Some("bool") => Ok(Kind::Flag),
        _ => heap
            .elements(value)
            .and_then(|items| {
                items
                    .into_iter()
                    .map(|item| match item {
                        Value::Keyword(name) => Some(name),
                        _ => None,
                    })
                    .collect::<Option<Vec<Sym>>>()
            })
            .map(Kind::Choice)
            .ok_or_else(|| {
                ":type must be :string, :int, :number, :bool or a vector of keywords".to_owned()
            }),
    }
}

fn fits(heap: &Heap, kind: &Kind, value: Value) -> bool {
    match kind {
        Kind::Text => heap.text(value).is_some(),
        Kind::Int => matches!(value, Value::Int(_)),
        Kind::Number => matches!(value, Value::Int(_) | Value::Float(_)),
        Kind::Flag => matches!(value, Value::Bool(_)),
        Kind::Choice(options) => matches!(value, Value::Keyword(name) if options.contains(&name)),
    }
}

fn describe(kind: &Kind) -> String {
    match kind {
        Kind::Text => "a string".to_owned(),
        Kind::Int => "a whole number".to_owned(),
        Kind::Number => "a number".to_owned(),
        Kind::Flag => "true or false".to_owned(),
        Kind::Choice(options) => {
            let names: Vec<String> = options
                .iter()
                .map(|option| format!(":{}", option.name()))
                .collect();
            format!("one of {}", names.join(" or "))
        }
    }
}

fn settings_of(heap: &mut Heap, value: Value) -> Result<Vec<Setting>, String> {
    let wrong = || ":settings must be a map of names to {type: …} maps".to_owned();
    let mut settings = Vec::new();
    for (key, fields) in heap.entries(value).ok_or_else(wrong)? {
        let key = keyword(key).ok_or_else(wrong)?.to_owned();
        let mut kind = None;
        let mut default = None;
        for (field, given) in heap.entries(fields).ok_or_else(wrong)? {
            match keyword(field) {
                Some("type") => kind = Some(kind_of(heap, given)?),
                Some("default") => default = Some(given),
                _ => {}
            }
        }
        let kind = kind.ok_or_else(|| format!("The setting {key} needs a :type"))?;
        if let Some(default) = default
            && !fits(heap, &kind, default)
        {
            return Err(format!(
                "The default for {key} must be {}, got {}",
                describe(&kind),
                print::write(heap, default)
            ));
        }
        let default = default.map(|default| heap.hold(default));
        settings.push(Setting { key, kind, default });
    }
    Ok(settings)
}

fn manifest_of(heap: &mut Heap, value: Value) -> Result<Manifest, String> {
    let items = heap
        .elements(value)
        .ok_or_else(|| "plugin.ox must hold a (plugin name …) form".to_owned())?;
    let Some((Value::Symbol(name), options)) = items.split_first() else {
        return Err("plugin.ox must hold a (plugin name …) form".to_owned());
    };
    let name = name.name().to_owned();
    let mut manifest = Manifest {
        entry: format!("{name}.ox"),
        name,
        commands: Vec::new(),
        settings: Vec::new(),
        startup_when: None,
        keys: Vec::new(),
        start_screen: Vec::new(),
        opens_directories: false,
        modes: Vec::new(),
    };
    for pair in options.chunks(2) {
        let [key, value] = pair else {
            return Err(format!("{} has no value", print::write(heap, pair[0])));
        };
        match keyword(*key) {
            Some("commands") => manifest.commands = commands_of(heap, *value)?,
            Some("settings") => manifest.settings = settings_of(heap, *value)?,
            Some("startup-when") => {
                let setting = keyword(*value).ok_or_else(|| {
                    ":startup-when must name a setting, such as :mode-line".to_owned()
                })?;
                manifest.startup_when = Some(setting.to_owned());
            }
            Some("entry") => {
                let entry = heap
                    .text(*value)
                    .ok_or_else(|| ":entry must be a file name".to_owned())?;
                entry.clone_into(&mut manifest.entry);
            }
            Some("keys") => {
                manifest.keys = pairs_of(heap, *value, ":keys must be a map of keys to commands")?;
            }
            Some("start-screen") => {
                manifest.start_screen = pairs_of(
                    heap,
                    *value,
                    ":start-screen must be a map of commands to descriptions",
                )?;
            }
            Some("opens-directories") => manifest.opens_directories = value.truthy(),
            Some("modes") => manifest.modes = modes::declared(heap, *value)?,
            _ => {}
        }
    }
    Ok(manifest)
}

fn read_manifest(
    isolate: &mut Isolate,
    editor: &mut Editor,
    file: &str,
    text: &str,
) -> Option<Manifest> {
    let parsed = isolate
        .eval_named(file, text)
        .and_then(|value| manifest_of(isolate.heap_mut(), value).map_err(Error::new));
    match parsed {
        Ok(manifest) => Some(manifest),
        Err(error) => {
            log::report(editor, &error, Some((file, text)));
            None
        }
    }
}

fn folder_files(folder: &Path) -> Vec<(String, String, String)> {
    let mut files: Vec<(String, String, String)> = fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "ox"))
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_owned();
            let text = fs::read_to_string(&path).ok()?;
            Some((name, path.display().to_string(), text))
        })
        .collect();
    files.sort();
    files
}

fn bundled_files(plugin: &str, files: &[(&str, &str)]) -> Vec<(String, String, String)> {
    files
        .iter()
        .map(|(name, text)| {
            (
                (*name).to_owned(),
                format!("{plugin}/{name}"),
                (*text).to_owned(),
            )
        })
        .collect()
}

fn add_sources(
    isolate: &mut Isolate,
    plugin: &str,
    entry: &str,
    files: &[(String, String, String)],
) {
    for (file_name, file, text) in files {
        if file_name == "plugin.ox" || file_name == entry {
            continue;
        }
        let stem = file_name.trim_end_matches(".ox");
        isolate.add_source(&format!("{plugin}/{stem}"), file, text);
    }
}

fn stub(editor: &mut Editor, plugin: &str, command: &str, summary: &str) {
    let (plugin, queued) = (plugin.to_owned(), command.to_owned());
    editor.add_dynamic_command(DynamicSpec {
        name: command.to_owned(),
        summary: summary.to_owned(),
        scope: Scope::Global.into(),
        run: Arc::new(move |editor: &mut Editor| {
            if let Some(plugins) = editor.plugin_state_mut::<Plugins>() {
                plugins.waking.push(plugin.clone());
            }
            commands::queue(editor, &queued);
        }),
    });
}

fn stubs(editor: &mut Editor, plugin: &Plugin) {
    if !plugin.loaded {
        for (command, summary) in &plugin.manifest.commands {
            stub(editor, &plugin.manifest.name, command, summary);
        }
    }
    for (keys, command) in &plugin.manifest.keys {
        let Some(id) = editor.command_named(command) else {
            let missing = format!(
                "{}'s key {keys} names no command {command}",
                plugin.manifest.name
            );
            log::report(editor, &Error::new(missing), None);
            continue;
        };
        if let Err(error) = editor.bind(Scope::Global, keys, id) {
            log::report(editor, &Error::new(format!("{keys}: {error}")), None);
        }
    }
    for (command, description) in &plugin.manifest.start_screen {
        if let Some(id) = editor.command_named(command) {
            editor.set_splash_entry(id, description.clone());
        }
    }
    if plugin.manifest.opens_directories {
        hooks::open_directories(editor, &plugin.manifest.name);
    }
    for declared in &plugin.manifest.modes {
        modes::declare(editor, &plugin.manifest.name, declared);
    }
}

pub fn request_wake(editor: &mut Editor, name: &str) {
    if let Some(plugins) = editor.plugin_state_mut::<Plugins>() {
        plugins.waking.push(name.to_owned());
    }
}

pub fn discover(
    isolate: &mut Isolate,
    editor: &mut Editor,
    bundled: &[Bundle],
    home: Option<&Path>,
) {
    let mut known: Vec<Plugin> = Vec::new();
    for bundle in bundled {
        let files = bundled_files(bundle.name, bundle.files);
        let Some((_, file, text)) = files.iter().find(|(name, _, _)| name == "plugin.ox") else {
            continue;
        };
        if let Some(manifest) = read_manifest(isolate, editor, file, text) {
            add_sources(isolate, bundle.name, &manifest.entry, &files);
            known.push(Plugin {
                manifest,
                origin: Origin::Bundled(bundle.files),
                active: true,
                loaded: false,
                config: Vec::new(),
            });
        }
    }
    let mut folders: Vec<PathBuf> = home
        .map(|home| home.join("plugins"))
        .and_then(|plugins| fs::read_dir(plugins).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("plugin.ox").is_file())
        .collect();
    folders.sort();
    for folder in folders {
        let path = folder.join("plugin.ox");
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Some(manifest) = read_manifest(isolate, editor, &path.display().to_string(), &text)
        else {
            continue;
        };
        let origin = Origin::Folder(folder);
        match known
            .iter_mut()
            .find(|plugin| plugin.manifest.name == manifest.name)
        {
            Some(bundled) => {
                bundled.manifest = manifest;
                bundled.origin = origin;
            }
            None => known.push(Plugin {
                manifest,
                origin,
                active: false,
                loaded: false,
                config: Vec::new(),
            }),
        }
    }
    for plugin in known.iter().filter(|plugin| plugin.active) {
        stubs(editor, plugin);
    }
    editor.set_plugin_state(Plugins {
        known,
        waking: Vec::new(),
    });
}

fn name_of(heap: &Heap, value: Value) -> String {
    match value {
        Value::Symbol(name) => name.name().to_owned(),
        _ => print::display(heap, value),
    }
}

fn configure(
    heap: &mut Heap,
    editor: &mut Editor,
    plugins: &mut Plugins,
    name: &str,
    options: &[Value],
) -> Vec<String> {
    let Some(plugin) = plugins
        .known
        .iter_mut()
        .find(|plugin| plugin.manifest.name == name)
    else {
        return vec![format!("There is no plugin named {name}")];
    };
    let mut problems = Vec::new();
    let mut disabled = false;
    for pair in options.chunks(2) {
        let [option, value] = pair else {
            problems.push(format!("{} has no value", print::write(heap, pair[0])));
            continue;
        };
        match keyword(*option) {
            Some("disabled") => disabled = value.truthy(),
            Some("config") => {
                let Some(entries) = heap.entries(*value) else {
                    problems.push(format!("{name}'s :config must be a map"));
                    continue;
                };
                plugin.config.clear();
                for (key, given) in entries {
                    let key = keyword(key).map_or_else(|| name_of(heap, key), str::to_owned);
                    let Some(setting) = plugin
                        .manifest
                        .settings
                        .iter()
                        .find(|setting| setting.key == key)
                    else {
                        problems.push(format!("{name} has no setting named {key}"));
                        continue;
                    };
                    if !fits(heap, &setting.kind, given) {
                        problems.push(format!(
                            "{name}'s {key} must be {}, got {}",
                            describe(&setting.kind),
                            print::write(heap, given)
                        ));
                        continue;
                    }
                    plugin.config.push((key, heap.hold(given)));
                }
            }
            _ => problems.push(format!(
                "use-plugin takes :config and :disabled, not {}",
                print::write(heap, *option)
            )),
        }
    }
    plugin.active = !disabled;
    if disabled {
        for (command, _) in &plugin.manifest.commands {
            editor.remove_dynamic_command(command);
        }
    } else {
        stubs(editor, plugin);
    }
    problems
}

fn enable(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) {
    let name = name_of(heap, arguments[0]);
    let mut plugins = editor.take_plugin_state::<Plugins>().unwrap_or_default();
    let problems = configure(heap, editor, &mut plugins, &name, &arguments[1..]);
    editor.set_plugin_state(plugins);
    for problem in problems {
        log::report(editor, &Error::new(problem), None);
    }
}

fn setting(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = name_of(heap, arguments[0]);
    let key = keyword(arguments[1]).map_or_else(|| name_of(heap, arguments[1]), str::to_owned);
    let plugin = editor
        .plugin_state::<Plugins>()
        .and_then(|plugins| {
            plugins
                .known
                .iter()
                .find(|plugin| plugin.manifest.name == name)
        })
        .ok_or_else(|| Error::new(format!("There is no plugin named {name}")))?;
    let declared = plugin
        .manifest
        .settings
        .iter()
        .find(|setting| setting.key == key)
        .ok_or_else(|| Error::new(format!("{name} has no setting named {key}")))?;
    let hold = plugin
        .config
        .iter()
        .find(|(configured, _)| *configured == key)
        .map(|(_, hold)| *hold)
        .or(declared.default);
    Ok(hold.and_then(|hold| heap.held(hold)).unwrap_or(Value::Nil))
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("enable-plugin", 1, None, |heap, editor, arguments| {
        enable(heap, editor, arguments);
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>("plugin-setting", 2, Some(2), setting);
}

pub fn wake_at_startup(isolate: &Isolate, editor: &mut Editor) {
    let Some(plugins) = editor.plugin_state_mut::<Plugins>() else {
        return;
    };
    let woken: Vec<String> = plugins
        .known
        .iter()
        .filter(|plugin| plugin.active && !plugin.loaded)
        .filter(|plugin| {
            plugin.manifest.startup_when.as_ref().is_some_and(|key| {
                let configured = plugin
                    .config
                    .iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, hold)| *hold);
                let default = plugin
                    .manifest
                    .settings
                    .iter()
                    .find(|setting| setting.key == *key)
                    .and_then(|setting| setting.default);
                configured
                    .or(default)
                    .and_then(|hold| isolate.held(hold))
                    .is_some_and(Value::truthy)
            })
        })
        .map(|plugin| plugin.manifest.name.clone())
        .collect();
    plugins.waking.extend(woken);
}

pub fn reload_file(
    isolate: &mut Isolate,
    editor: &mut Editor,
    name: &str,
    file_name: &str,
    path: &Path,
) {
    let plugin = editor
        .plugin_state::<Plugins>()
        .and_then(|plugins| {
            plugins
                .known
                .iter()
                .find(|plugin| plugin.manifest.name == name)
        })
        .cloned();
    let Some(plugin) = plugin.filter(|plugin| plugin.loaded) else {
        return;
    };
    if file_name == "plugin.ox" {
        editor.show_message(format!(
            "{name}'s plugin.ox is read again when oxmacs starts"
        ));
        return;
    }
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let file = path.display().to_string();
    let module = if file_name == plugin.manifest.entry {
        name.to_owned()
    } else {
        let module = format!("{name}/{}", file_name.trim_end_matches(".ox"));
        isolate.add_source(&module, &file, &text);
        module
    };
    match isolate.eval_module_with(editor, &module, &file, &text) {
        Ok(()) => {
            views::redraw(editor);
            editor.show_message(format!("Reloaded {module}"));
        }
        Err(error) => log::report(editor, &error, Some((&file, &text))),
    }
}

pub fn wake_waiting(isolate: &mut Isolate, editor: &mut Editor) {
    let waking = editor
        .plugin_state_mut::<Plugins>()
        .map(|plugins| std::mem::take(&mut plugins.waking))
        .unwrap_or_default();
    for name in waking {
        wake(isolate, editor, &name);
    }
}

fn wake(isolate: &mut Isolate, editor: &mut Editor, name: &str) {
    let Some(plugins) = editor.plugin_state_mut::<Plugins>() else {
        return;
    };
    let Some(plugin) = plugins
        .known
        .iter_mut()
        .find(|plugin| plugin.manifest.name == name && plugin.active && !plugin.loaded)
    else {
        return;
    };
    plugin.loaded = true;
    let plugin = plugin.clone();
    let files = match &plugin.origin {
        Origin::Bundled(files) => bundled_files(name, files),
        Origin::Folder(folder) => folder_files(folder),
    };
    add_sources(isolate, name, &plugin.manifest.entry, &files);
    let Some((_, file, text)) = files
        .iter()
        .find(|(file_name, _, _)| *file_name == plugin.manifest.entry)
    else {
        let missing = format!("{name} has no entry file {}", plugin.manifest.entry);
        log::report(editor, &Error::new(missing), None);
        return;
    };
    let preamble = format!("(def (setting key) (plugin-setting '{name} key))");
    let loaded = isolate
        .eval_module_with(&mut (), name, "<plugin>", &preamble)
        .and_then(|()| isolate.eval_module_with(editor, name, file, text));
    if let Err(error) = loaded {
        log::report(editor, &error, Some((file, text)));
        return;
    }
    for (command, _) in &plugin.manifest.commands {
        if !commands::defined(editor, command) {
            let forgotten = format!("{name} didn't define the command {command}");
            log::report(editor, &Error::new(forgotten), None);
        }
    }
}
#[cfg(test)]
mod tests {
    use std::path::Path;

    use oxmacs_core::editor::Editor;
    use oxmacs_core::text::Buffer;

    use super::Bundle;
    use crate::{Ox, log};

    const MANIFEST: &str = r#"(plugin greeter
  :version "1.0.0"
  :summary "Says hello"
  :commands {greet: "Say hello"}
  :entry "greeter.ox")"#;

    const GREETER: Bundle = Bundle {
        name: "greeter",
        files: &[
            ("plugin.ox", MANIFEST),
            (
                "greeter.ox",
                "(module (import (greeter words)))\n(message \"greeter loaded\")\n(defcommand greet \"Say hello\" (message words/hello))",
            ),
            ("words.ox", "(def hello \"Hello from greeter\")"),
        ],
    };

    fn home_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        for (name, text) in files {
            let path = home.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        home
    }

    fn started(bundled: &'static [Bundle], home: &Path) -> (Editor, Ox) {
        let mut editor = Editor::new();
        let mut ox = Ox::with_bundled(bundled);
        ox.start(&mut editor, Some(home));
        (editor, ox)
    }

    fn run(editor: &mut Editor, ox: &mut Ox, command: &str) {
        let id = editor.command_named(command).unwrap();
        editor.execute(id);
        ox.tick(editor);
    }

    fn logged(editor: &Editor) -> String {
        editor
            .buffer_named(log::LOG)
            .and_then(|id| editor.buffer_by_id(id))
            .map(Buffer::contents)
            .unwrap_or_default()
    }

    #[test]
    fn bundled_plugins_wake_on_their_first_command() {
        let home = home_with(&[]);
        let (mut editor, mut ox) = started(&[GREETER], home.path());

        assert!(editor.command_named("greet").is_some());
        assert_eq!(editor.message(), None);

        run(&mut editor, &mut ox, "greet");

        assert_eq!(editor.message(), Some("Hello from greeter"));
        assert_eq!(logged(&editor), "");
    }

    #[test]
    fn your_plugins_wait_for_use_plugin() {
        let plugin = [
            ("plugins/greeter/plugin.ox", MANIFEST),
            (
                "plugins/greeter/greeter.ox",
                "(defcommand greet \"Say hello\" (message \"Hello from home\"))",
            ),
        ];
        let quiet = home_with(&plugin);
        let (editor, _) = started(&[], quiet.path());
        assert!(editor.command_named("greet").is_none());

        let mut files = plugin.to_vec();
        files.push(("init.ox", "(use-plugin greeter)"));
        let used = home_with(&files);
        let (mut editor, mut ox) = started(&[], used.path());
        run(&mut editor, &mut ox, "greet");

        assert_eq!(editor.message(), Some("Hello from home"));
    }

    #[test]
    fn your_copy_of_a_bundled_plugin_replaces_it() {
        let home = home_with(&[
            ("plugins/greeter/plugin.ox", MANIFEST),
            (
                "plugins/greeter/greeter.ox",
                "(defcommand greet \"Say hello\" (message \"Hello from my copy\"))",
            ),
        ]);
        let (mut editor, mut ox) = started(&[GREETER], home.path());

        run(&mut editor, &mut ox, "greet");

        assert_eq!(editor.message(), Some("Hello from my copy"));
    }

    #[test]
    fn use_plugin_can_turn_a_bundled_plugin_off() {
        let home = home_with(&[("init.ox", "(use-plugin greeter :disabled true)")]);
        let (editor, _) = started(&[GREETER], home.path());

        assert!(editor.command_named("greet").is_none());
    }

    #[test]
    fn mistakes_in_use_plugin_are_logged_without_stopping_init() {
        let home = home_with(&[(
            "init.ox",
            "(use-plugin nowhere)\n(defcommand after \"Still runs\" (message \"ok\"))",
        )]);
        let (editor, _) = started(&[GREETER], home.path());

        assert!(logged(&editor).contains("There is no plugin named nowhere"));
        assert!(editor.command_named("after").is_some());
    }

    #[test]
    fn a_plugin_that_fails_to_load_is_logged_with_its_file() {
        const BROKEN: Bundle = Bundle {
            name: "greeter",
            files: &[
                ("plugin.ox", MANIFEST),
                ("greeter.ox", "(message \"starting\")\n(/ 1 0)"),
            ],
        };
        let home = home_with(&[]);
        let (mut editor, mut ox) = started(&[BROKEN], home.path());

        run(&mut editor, &mut ox, "greet");

        assert!(
            logged(&editor).contains("greeter/greeter.ox:2:1: Division by zero"),
            "{}",
            logged(&editor)
        );
    }

    #[test]
    fn a_plugin_that_forgets_a_command_is_logged() {
        const FORGETFUL: Bundle = Bundle {
            name: "greeter",
            files: &[("plugin.ox", MANIFEST), ("greeter.ox", "(def nothing nil)")],
        };
        let home = home_with(&[]);
        let (mut editor, mut ox) = started(&[FORGETFUL], home.path());

        run(&mut editor, &mut ox, "greet");

        assert!(
            logged(&editor).contains("greeter didn't define the command greet"),
            "{}",
            logged(&editor)
        );
    }

    #[test]
    fn broken_manifests_are_logged_and_skipped() {
        let home = home_with(&[
            (
                "plugins/odd/plugin.ox",
                "(plugin odd :commands [not a map])",
            ),
            ("init.ox", "(use-plugin odd)"),
        ]);
        let (editor, _) = started(&[], home.path());

        assert!(
            logged(&editor).contains("plugin.ox: :commands must be a map of names to summaries"),
            "{}",
            logged(&editor)
        );
    }

    const SETTINGS: &str = r#"(plugin greeter
  :commands {greet: "Say hello" peek: "Read a missing setting"}
  :settings {name: {type: :string default: "world"}
             shout: {type: :bool default: false}
             style: {type: [:plain :fancy] default: :plain}
             chatty: {type: :bool default: false}}
  :startup-when :chatty)"#;

    const CONFIGURED: Bundle = Bundle {
        name: "greeter",
        files: &[
            ("plugin.ox", SETTINGS),
            (
                "greeter.ox",
                "(message \"greeter loaded\")\n(defcommand greet \"Say hello\" (message (str \"Hello \" (setting :name) (if (setting :shout) \"!\" \"\"))))\n(defcommand peek \"Read a missing setting\" (setting :missing))",
            ),
        ],
    };

    #[test]
    fn settings_fall_back_to_their_defaults() {
        let home = home_with(&[]);
        let (mut editor, mut ox) = started(&[CONFIGURED], home.path());

        run(&mut editor, &mut ox, "greet");

        assert_eq!(editor.message(), Some("Hello world"));
    }

    #[test]
    fn init_ox_configures_a_plugin() {
        let home = home_with(&[(
            "init.ox",
            "(use-plugin greeter :config {name: \"Ana\" shout: true})",
        )]);
        let (mut editor, mut ox) = started(&[CONFIGURED], home.path());

        run(&mut editor, &mut ox, "greet");

        assert_eq!(editor.message(), Some("Hello Ana!"));
    }

    #[test]
    fn wrong_settings_are_logged_and_keep_their_defaults() {
        let home = home_with(&[(
            "init.ox",
            "(use-plugin greeter :config {name: 5 colour: :red style: :odd})",
        )]);
        let (mut editor, mut ox) = started(&[CONFIGURED], home.path());

        run(&mut editor, &mut ox, "greet");

        let log = logged(&editor);
        assert!(
            log.contains("greeter's name must be a string, got 5"),
            "{log}"
        );
        assert!(log.contains("greeter has no setting named colour"), "{log}");
        assert!(
            log.contains("greeter's style must be one of :plain or :fancy, got :odd"),
            "{log}"
        );
        assert_eq!(editor.message(), Some("Hello world"));
    }

    #[test]
    fn a_setting_can_wake_a_plugin_at_startup() {
        let home = home_with(&[("init.ox", "(use-plugin greeter :config {chatty: true})")]);
        let (editor, _) = started(&[CONFIGURED], home.path());

        assert_eq!(editor.message(), Some("greeter loaded"));
    }

    #[test]
    fn plugins_only_read_settings_they_declare() {
        let home = home_with(&[]);
        let (mut editor, mut ox) = started(&[CONFIGURED], home.path());

        run(&mut editor, &mut ox, "peek");

        assert!(
            logged(&editor).contains("greeter has no setting named missing"),
            "{}",
            logged(&editor)
        );
    }
}
