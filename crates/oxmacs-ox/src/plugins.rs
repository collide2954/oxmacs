use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ox::heap::{Heap, Hold};
use ox::symbol::Sym;
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::command::{DynamicSpec, Scope};
use oxmacs_core::editor::Editor;
use semver::Version;

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
    List,
    Choice(Vec<Sym>),
}

#[derive(Debug, Clone)]
struct Setting {
    key: String,
    kind: Kind,
    default: Option<Hold>,
    doc: Option<String>,
}

#[derive(Debug, Clone)]
struct Manifest {
    name: String,
    version: Version,
    summary: String,
    commands: Vec<(String, String)>,
    settings: Vec<Setting>,
    startup_when: Option<String>,
    entry: String,
    keys: Vec<(String, String)>,
    mode_keys: ModeKeys,
    start_screen: Vec<(String, String)>,
    opens_directories: bool,
    opt_in: bool,
    modes: Vec<modes::Declared>,
    wakes_for: Vec<String>,
}

#[derive(Debug, Clone)]
struct Plugin {
    manifest: Manifest,
    origin: Origin,
    active: bool,
    loaded: bool,
    config: Vec<(String, Hold)>,
    saved: Vec<(String, Hold)>,
    custom: Vec<(String, Hold)>,
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

type ModeKeys = Vec<(String, Vec<(String, String)>)>;

fn mode_keys_of(heap: &Heap, value: Value) -> Result<ModeKeys, String> {
    let wrong = ":mode-keys must map mode names to maps of keys to commands";
    heap.entries(value)
        .ok_or_else(|| wrong.to_owned())?
        .into_iter()
        .map(|(mode, keys)| {
            let mode = match mode {
                Value::Keyword(name) | Value::Symbol(name) => name.name().to_owned(),
                _ => heap.text(mode).ok_or_else(|| wrong.to_owned())?.to_owned(),
            };
            Ok((mode, pairs_of(heap, keys, wrong)?))
        })
        .collect()
}

fn kind_of(heap: &Heap, value: Value) -> Result<Kind, String> {
    match keyword(value) {
        Some("string") => Ok(Kind::Text),
        Some("int") => Ok(Kind::Int),
        Some("number") => Ok(Kind::Number),
        Some("bool") => Ok(Kind::Flag),
        Some("list") => Ok(Kind::List),
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
                ":type must be :string, :int, :number, :bool, :list or a vector of keywords"
                    .to_owned()
            }),
    }
}

fn fits(heap: &Heap, kind: &Kind, value: Value) -> bool {
    match kind {
        Kind::Text => heap.text(value).is_some(),
        Kind::Int => matches!(value, Value::Int(_)),
        Kind::Number => matches!(value, Value::Int(_) | Value::Float(_)),
        Kind::Flag => matches!(value, Value::Bool(_)),
        Kind::List => heap.text(value).is_none() && heap.elements(value).is_some(),
        Kind::Choice(options) => matches!(value, Value::Keyword(name) if options.contains(&name)),
    }
}

fn describe(kind: &Kind) -> String {
    match kind {
        Kind::Text => "a string".to_owned(),
        Kind::Int => "a whole number".to_owned(),
        Kind::Number => "a number".to_owned(),
        Kind::Flag => "true or false".to_owned(),
        Kind::List => "a vector".to_owned(),
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
        let mut doc = None;
        for (field, given) in heap.entries(fields).ok_or_else(wrong)? {
            match keyword(field) {
                Some("type") => kind = Some(kind_of(heap, given)?),
                Some("default") => default = Some(given),
                Some("doc") => doc = heap.text(given).map(str::to_owned),
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
                print::brief(heap, default)
            ));
        }
        let default = default.map(|default| heap.hold(default));
        settings.push(Setting {
            key,
            kind,
            default,
            doc,
        });
    }
    Ok(settings)
}

fn version_of(heap: &Heap, name: &str, value: Value) -> Result<Version, String> {
    heap.text(value)
        .and_then(|text| Version::parse(text).ok())
        .ok_or_else(|| {
            format!(
                "{name}'s :version must be a version such as 1.2.0, got {}",
                print::brief(heap, value)
            )
        })
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
        version: Version::new(0, 0, 0),
        summary: String::new(),
        name,
        commands: Vec::new(),
        settings: Vec::new(),
        startup_when: None,
        keys: Vec::new(),
        mode_keys: Vec::new(),
        start_screen: Vec::new(),
        opens_directories: false,
        opt_in: false,
        modes: Vec::new(),
        wakes_for: Vec::new(),
    };
    for pair in options.chunks(2) {
        let [key, value] = pair else {
            return Err(format!("{} has no value", print::brief(heap, pair[0])));
        };
        match keyword(*key) {
            Some("version") => manifest.version = version_of(heap, &manifest.name, *value)?,
            Some("summary") => {
                heap.text(*value)
                    .ok_or_else(|| ":summary must be text".to_owned())?
                    .clone_into(&mut manifest.summary);
            }
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
            Some("mode-keys") => manifest.mode_keys = mode_keys_of(heap, *value)?,
            Some("start-screen") => {
                manifest.start_screen = pairs_of(
                    heap,
                    *value,
                    ":start-screen must be a map of commands to descriptions",
                )?;
            }
            Some("opens-directories") => manifest.opens_directories = value.truthy(),
            Some("opt-in") => manifest.opt_in = value.truthy(),
            Some("modes") => manifest.modes = modes::declared(heap, *value)?,
            Some("wakes-for") => {
                manifest.wakes_for = heap
                    .elements(*value)
                    .filter(|_| heap.text(*value).is_none())
                    .and_then(|items| {
                        items
                            .into_iter()
                            .map(|item| heap.text(item).map(str::to_owned))
                            .collect()
                    })
                    .ok_or_else(|| ":wakes-for must be a vector of mode names".to_owned())?;
            }
            _ => {
                return Err(format!(
                    "{}'s plugin.ox has no key {}",
                    manifest.name,
                    print::brief(heap, *key)
                ));
            }
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
    for (mode, keys) in &plugin.manifest.mode_keys {
        bind_mode_keys(editor, &plugin.manifest.name, mode, keys);
    }
}

fn bind_mode_keys(editor: &mut Editor, plugin: &str, mode: &str, keys: &[(String, String)]) {
    let Some(scope) = editor.language_scope(mode) else {
        let missing = format!("{plugin}'s :mode-keys name no mode {mode}");
        log::report(editor, &Error::new(missing), None);
        return;
    };
    for (keys, command) in keys {
        let Some(id) = editor.command_named(command) else {
            let missing = format!("{plugin}'s key {keys} in {mode} names no command {command}");
            log::report(editor, &Error::new(missing), None);
            continue;
        };
        if let Err(error) = editor.bind(scope, keys, id) {
            log::report(editor, &Error::new(format!("{keys}: {error}")), None);
        }
    }
}

pub fn request_wake(editor: &mut Editor, name: &str) {
    if let Some(plugins) = editor.plugin_state_mut::<Plugins>() {
        plugins.waking.push(name.to_owned());
    }
}

pub fn woken_by_modes(editor: &Editor) -> Vec<String> {
    let Some(plugins) = editor.plugin_state::<Plugins>() else {
        return Vec::new();
    };
    let waiting: Vec<&Plugin> = plugins
        .known
        .iter()
        .filter(|plugin| plugin.active && !plugin.loaded && !plugin.manifest.wakes_for.is_empty())
        .collect();
    if waiting.is_empty() {
        return Vec::new();
    }
    let open: Vec<&str> = editor
        .buffer_ids()
        .into_iter()
        .filter_map(|id| editor.buffer_language(id))
        .map(oxmacs_core::syntax::Language::name)
        .collect();
    waiting
        .into_iter()
        .filter(|plugin| {
            plugin
                .manifest
                .wakes_for
                .iter()
                .any(|mode| open.contains(&mode.as_str()))
        })
        .map(|plugin| plugin.manifest.name.clone())
        .collect()
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
            let active = !manifest.opt_in;
            known.push(Plugin {
                manifest,
                origin: Origin::Bundled(bundle.files),
                active,
                loaded: false,
                config: Vec::new(),
                saved: Vec::new(),
                custom: Vec::new(),
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
                saved: Vec::new(),
                custom: Vec::new(),
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

fn merge(heap: &mut Heap, plugin: &mut Plugin, config: Value) -> Vec<String> {
    let name = plugin.manifest.name.clone();
    let Some(entries) = heap.entries(config) else {
        return vec![format!("{name}'s :config must be a map")];
    };
    let mut problems = Vec::new();
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
        if given != Value::Nil && !fits(heap, &setting.kind, given) {
            problems.push(format!(
                "{name}'s {key} must be {}, got {}",
                describe(&setting.kind),
                print::brief(heap, given)
            ));
            continue;
        }
        if let Some(at) = plugin.config.iter().position(|(known, _)| *known == key) {
            let (_, old) = plugin.config.remove(at);
            heap.release(old);
        }
        if given != Value::Nil {
            plugin.config.push((key, heap.hold(given)));
        }
    }
    problems
}

pub fn reset_configs(heap: &mut Heap, editor: &mut Editor) {
    let Some(plugins) = editor.plugin_state_mut::<Plugins>() else {
        return;
    };
    for plugin in &mut plugins.known {
        for (_, hold) in plugin.config.drain(..) {
            heap.release(hold);
        }
    }
}

fn snapshot(heap: &mut Heap, config: &[(String, Hold)], into: &mut Vec<(String, Hold)>) {
    for (_, hold) in into.drain(..) {
        heap.release(hold);
    }
    for (key, hold) in config {
        if let Some(value) = heap.held(*hold) {
            into.push((key.clone(), heap.hold(value)));
        }
    }
}

pub fn mark_saved(heap: &mut Heap, editor: &mut Editor) {
    if let Some(plugins) = editor.plugin_state_mut::<Plugins>() {
        for plugin in &mut plugins.known {
            snapshot(heap, &plugin.config, &mut plugin.saved);
        }
    }
}

pub fn mark_custom(heap: &mut Heap, editor: &mut Editor) {
    if let Some(plugins) = editor.plugin_state_mut::<Plugins>() {
        for plugin in &mut plugins.known {
            snapshot(heap, &plugin.config, &mut plugin.custom);
        }
    }
}

fn kind_value(heap: &mut Heap, kind: &Kind) -> Result<(Value, Value), Error> {
    let made =
        |made: Result<Value, ox::heap::HeapFull>| made.map_err(|full| Error::new(full.to_string()));
    Ok(match kind {
        Kind::Text => (Value::keyword("string"), Value::Nil),
        Kind::Int => (Value::keyword("int"), Value::Nil),
        Kind::Number => (Value::keyword("number"), Value::Nil),
        Kind::Flag => (Value::keyword("bool"), Value::Nil),
        Kind::List => (Value::keyword("list"), Value::Nil),
        Kind::Choice(options) => {
            let options: Vec<Value> = options.iter().map(|name| Value::Keyword(*name)).collect();
            (Value::keyword("choice"), made(heap.vector_from(&options))?)
        }
    })
}

fn configured(heap: &Heap, config: &[(String, Hold)], key: &str) -> Value {
    config
        .iter()
        .find(|(known, _)| known == key)
        .and_then(|(_, hold)| heap.held(*hold))
        .unwrap_or(Value::Nil)
}

fn plugin_settings(heap: &mut Heap, editor: &mut Editor) -> Result<Value, Error> {
    let made =
        |made: Result<Value, ox::heap::HeapFull>| made.map_err(|full| Error::new(full.to_string()));
    let plugins = editor.take_plugin_state::<Plugins>().unwrap_or_default();
    let mut found = Vec::new();
    for plugin in plugins.known.iter().filter(|plugin| plugin.active) {
        for setting in &plugin.manifest.settings {
            let (kind, choices) = kind_value(heap, &setting.kind)?;
            let name = made(heap.string(&plugin.manifest.name))?;
            let doc = match &setting.doc {
                Some(doc) => made(heap.string(doc))?,
                None => Value::Nil,
            };
            let default = setting
                .default
                .and_then(|hold| heap.held(hold))
                .unwrap_or(Value::Nil);
            let value = configured(heap, &plugin.config, &setting.key);
            let saved = configured(heap, &plugin.saved, &setting.key);
            let custom = configured(heap, &plugin.custom, &setting.key);
            found.push(heap.map_from(&[
                Value::keyword("plugin"),
                name,
                Value::keyword("key"),
                Value::keyword(&setting.key),
                Value::keyword("type"),
                kind,
                Value::keyword("choices"),
                choices,
                Value::keyword("doc"),
                doc,
                Value::keyword("default"),
                default,
                Value::keyword("value"),
                value,
                Value::keyword("saved"),
                saved,
                Value::keyword("custom"),
                custom,
            ])?);
        }
    }
    editor.set_plugin_state(plugins);
    made(heap.vector_from(&found))
}

fn configure_plugin(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) {
    let name = name_of(heap, arguments[0]);
    let mut plugins = editor.take_plugin_state::<Plugins>().unwrap_or_default();
    let problems = match plugins
        .known
        .iter_mut()
        .find(|plugin| plugin.manifest.name == name)
    {
        Some(plugin) => merge(heap, plugin, arguments[1]),
        None => vec![format!("There is no plugin named {name}")],
    };
    editor.set_plugin_state(plugins);
    for problem in problems {
        log::report(editor, &Error::new(problem), None);
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
            problems.push(format!("{} has no value", print::brief(heap, pair[0])));
            continue;
        };
        match keyword(*option) {
            Some("disabled") => disabled = value.truthy(),
            Some("config") => problems.extend(merge(heap, plugin, *value)),
            _ => problems.push(format!(
                "use-plugin takes :config and :disabled, not {}",
                print::brief(heap, *option)
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
    isolate.define_host_native::<Editor>(
        "configure-plugin",
        2,
        Some(2),
        |heap, editor, arguments| {
            configure_plugin(heap, editor, arguments);
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native::<Editor>("plugin-settings", 0, Some(0), |heap, editor, _| {
        plugin_settings(heap, editor)
    });
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
        start_now(isolate, editor, path);
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
    match isolate.within(crate::PATIENT, |isolate| {
        isolate.eval_module_with(editor, &module, &file, &text)
    }) {
        Ok(()) => {
            views::redraw(editor);
            editor.show_message(format!("Reloaded {module}"));
        }
        Err(error) => log::report(editor, &error, Some((&file, &text))),
    }
}

pub(crate) fn module_of(editor: &Editor, plugin: &str, file_name: &str) -> String {
    let entry = editor.plugin_state::<Plugins>().and_then(|plugins| {
        plugins
            .known
            .iter()
            .find(|known| known.manifest.name == plugin)
            .map(|known| known.manifest.entry.clone())
    });
    if entry.as_deref() == Some(file_name) {
        plugin.to_owned()
    } else {
        format!("{plugin}/{}", file_name.trim_end_matches(".ox"))
    }
}

pub fn wake_waiting(isolate: &mut Isolate, editor: &mut Editor) {
    let waking = editor
        .plugin_state_mut::<Plugins>()
        .map(|plugins| std::mem::take(&mut plugins.waking))
        .unwrap_or_default();
    for name in waking {
        let _ = wake(isolate, editor, &name);
    }
}

fn wake(isolate: &mut Isolate, editor: &mut Editor, name: &str) -> bool {
    let Some(plugins) = editor.plugin_state_mut::<Plugins>() else {
        return false;
    };
    let Some(plugin) = plugins
        .known
        .iter_mut()
        .find(|plugin| plugin.manifest.name == name && plugin.active && !plugin.loaded)
    else {
        return false;
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
        return false;
    };
    let preamble = format!("(def (setting key) (plugin-setting '{name} key))");
    let loaded = isolate
        .eval_module_with(&mut (), name, "<plugin>", &preamble)
        .and_then(|()| {
            isolate.within(crate::PATIENT, |isolate| {
                isolate.eval_module_with(editor, name, file, text)
            })
        });
    if let Err(error) = loaded {
        log::report(editor, &error, Some((file, text)));
        return false;
    }
    let mut complete = true;
    for (command, _) in &plugin.manifest.commands {
        if !commands::defined(editor, command) {
            let forgotten = format!("{name} didn't define the command {command}");
            log::report(editor, &Error::new(forgotten), None);
            complete = false;
        }
    }
    complete
}

fn start_now(isolate: &mut Isolate, editor: &mut Editor, path: &Path) {
    let Some(folder) = path.parent() else {
        return;
    };
    let manifest_path = folder.join("plugin.ox");
    let Ok(text) = fs::read_to_string(&manifest_path) else {
        editor.show_message("A plugin starts once its folder has a plugin.ox");
        return;
    };
    let Some(manifest) =
        read_manifest(isolate, editor, &manifest_path.display().to_string(), &text)
    else {
        return;
    };
    let name = manifest.name.clone();
    if !folder.join(&manifest.entry).is_file() {
        editor.show_message(format!(
            "The {name} plugin starts once {} exists",
            manifest.entry
        ));
        return;
    }
    let origin = Origin::Folder(folder.to_path_buf());
    let Some(plugins) = editor.plugin_state_mut::<Plugins>() else {
        return;
    };
    match plugins
        .known
        .iter_mut()
        .find(|plugin| plugin.manifest.name == name)
    {
        Some(known) => {
            known.manifest = manifest;
            known.origin = origin;
            known.active = true;
        }
        None => plugins.known.push(Plugin {
            manifest,
            origin,
            active: true,
            loaded: false,
            config: Vec::new(),
            saved: Vec::new(),
            custom: Vec::new(),
        }),
    }
    let Some(plugin) = plugins
        .known
        .iter()
        .find(|plugin| plugin.manifest.name == name)
        .cloned()
    else {
        return;
    };
    stubs(editor, &plugin);
    if wake(isolate, editor, &name) {
        views::redraw(editor);
        editor.show_message(format!(
            "Started the {name} plugin; (use-plugin {name}) in init.ox keeps it on"
        ));
    }
}
#[cfg(test)]
mod tests {
    use std::path::Path;

    use oxmacs_core::editor::Editor;
    use oxmacs_core::text::Buffer;

    use super::Bundle;
    use crate::testing::{home_with, settle};
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

    fn started(bundled: &'static [Bundle], home: &Path) -> (Editor, Ox) {
        let mut editor = Editor::new();
        let mut ox = Ox::with_bundled(bundled);
        ox.start(&mut editor, Some(home));
        (editor, ox)
    }

    fn run(editor: &mut Editor, ox: &mut Ox, command: &str) {
        let id = editor.command_named(command).unwrap();
        editor.execute(id);
        settle(editor, ox);
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
             chatty: {type: :bool default: false}
             friends: {type: :list default: []}}
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
    fn config_merges_key_by_key_with_init_ox_last() {
        let home = home_with(&[
            (
                "custom.ox",
                "(configure-plugin 'greeter {name: \"Ana\" shout: true})",
            ),
            ("init.ox", "(use-plugin greeter :config {shout: false})"),
        ]);
        let (mut editor, mut ox) = started(&[CONFIGURED], home.path());

        run(&mut editor, &mut ox, "greet");

        assert_eq!(editor.message(), Some("Hello Ana"));
    }

    #[test]
    fn nil_puts_a_plugin_setting_back_to_its_default() {
        let home = home_with(&[(
            "init.ox",
            "(use-plugin greeter :config {name: \"Ana\"})\n(configure-plugin 'greeter {name: nil})",
        )]);
        let (mut editor, mut ox) = started(&[CONFIGURED], home.path());

        run(&mut editor, &mut ox, "greet");

        assert_eq!(editor.message(), Some("Hello world"));
        assert_eq!(logged(&editor), "");
    }

    #[test]
    fn list_settings_hold_vectors_of_any_data() {
        let home = home_with(&[(
            "init.ox",
            "(use-plugin greeter :config {friends: [\"Bo\" {name: \"Cy\" port: 6697}]})",
        )]);
        let (mut editor, mut ox) = started(&[CONFIGURED], home.path());

        assert_eq!(
            ox.eval(&mut editor, "(plugin-setting 'greeter :friends)")
                .unwrap(),
            r#"["Bo" {name: "Cy" port: 6697}]"#
        );
        ox.eval(&mut editor, "(configure-plugin 'greeter {friends: 5})")
            .unwrap();
        let log = logged(&editor);
        assert!(
            log.contains("greeter's friends must be a vector, got 5"),
            "{log}"
        );
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

    const OPT_IN: Bundle = Bundle {
        name: "toy",
        files: &[
            (
                "plugin.ox",
                "(plugin toy :commands {play: \"Play\"} :opt-in true :entry \"toy.ox\")",
            ),
            ("toy.ox", "(defcommand play \"Play\" (message \"Playing\"))"),
        ],
    };

    const MODE_KEYS: Bundle = Bundle {
        name: "shouter",
        files: &[
            (
                "plugin.ox",
                r#"(plugin shouter :commands {shout: "Shout"} :mode-keys {"R" {"C-c s" shout} "Cobol" {"C-c s" shout}})"#,
            ),
            (
                "shouter.ox",
                "(defcommand shout \"Shout\" (message \"Shouting in R\"))",
            ),
        ],
    };

    #[test]
    fn mode_keys_wake_their_plugin_in_buffers_of_that_mode() {
        let home = home_with(&[]);
        let (mut editor, mut ox) = started(&[MODE_KEYS], home.path());

        crate::testing::press(&mut editor, &mut ox, "C-c s");
        assert_eq!(editor.message(), Some("C-c s is undefined"));
        editor.set_major_mode(Some(oxmacs_core::syntax::Language::R));
        crate::testing::press(&mut editor, &mut ox, "C-c s");

        assert_eq!(editor.message(), Some("Shouting in R"));
        assert!(
            logged(&editor).contains("shouter's :mode-keys name no mode Cobol"),
            "{}",
            logged(&editor)
        );
    }

    const WAKES_FOR: Bundle = Bundle {
        name: "watcher",
        files: &[
            (
                "plugin.ox",
                r#"(plugin watcher :commands {watch: "Watch"} :wakes-for ["R"])"#,
            ),
            (
                "watcher.ox",
                "(defcommand watch \"Watch\" nil) (message \"Watching R\")",
            ),
        ],
    };

    #[test]
    fn plugins_wake_once_a_buffer_of_a_mode_they_wake_for_is_open() {
        let home = home_with(&[]);
        let (mut editor, mut ox) = started(&[WAKES_FOR], home.path());
        settle(&mut editor, &mut ox);
        assert_ne!(editor.message(), Some("Watching R"));

        editor.set_major_mode(Some(oxmacs_core::syntax::Language::R));
        settle(&mut editor, &mut ox);

        assert_eq!(editor.message(), Some("Watching R"));
        assert_eq!(logged(&editor), "");
    }

    #[test]
    fn opt_in_plugins_wait_for_use_plugin() {
        let home = home_with(&[]);
        let (editor, _) = started(&[OPT_IN], home.path());
        assert!(editor.command_named("play").is_none());

        let home = home_with(&[("init.ox", "(use-plugin toy)")]);
        let (mut editor, mut ox) = started(&[OPT_IN], home.path());
        run(&mut editor, &mut ox, "play");

        assert_eq!(editor.message(), Some("Playing"));
        assert_eq!(logged(&editor), "");
    }

    const LOOSE: Bundle = Bundle {
        name: "loose",
        files: &[
            (
                "plugin.ox",
                "(plugin loose :version \"1.0\" :commands {lax: \"Lax\"})",
            ),
            ("loose.ox", "(defcommand lax \"Lax\" (message \"lax\"))"),
        ],
    };

    const TYPO: Bundle = Bundle {
        name: "typo",
        files: &[
            (
                "plugin.ox",
                "(plugin typo :version \"1.0.0\" :descripton \"Oops\" :commands {oops: \"Oops\"})",
            ),
            ("typo.ox", "(defcommand oops \"Oops\" (message \"oops\"))"),
        ],
    };

    #[test]
    fn manifests_need_a_semantic_version_and_known_keys() {
        let home = home_with(&[]);
        let (editor, _) = started(&[LOOSE, TYPO], home.path());

        assert!(editor.command_named("lax").is_none());
        assert!(editor.command_named("oops").is_none());
        let shown = logged(&editor);
        assert!(
            shown.contains("loose's :version must be a version such as 1.2.0, got \"1.0\""),
            "{shown}"
        );
        assert!(
            shown.contains("typo's plugin.ox has no key :descripton"),
            "{shown}"
        );
    }

    #[test]
    fn a_setting_wakes_a_plugin_once_oxmacs_is_running() {
        let home = home_with(&[("init.ox", "(use-plugin greeter :config {chatty: true})")]);
        let (mut editor, mut ox) = started(&[CONFIGURED], home.path());
        assert_eq!(editor.message(), None);

        settle(&mut editor, &mut ox);

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
