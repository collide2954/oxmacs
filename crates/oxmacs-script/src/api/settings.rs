use std::path::Path;

use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::config::Config;
use oxmacs_core::config::schema::{self, Kind, SETTINGS, Setting};
use oxmacs_core::editor::Editor;
use serde_json::Value as Json;

use crate::ide::repl;

fn name_of(heap: &Heap, value: Value) -> Option<String> {
    match value {
        Value::Keyword(name) | Value::Symbol(name) => Some(name.name().to_owned()),
        _ => heap.text(value).map(str::to_owned),
    }
}

fn wanted(kind: Kind) -> String {
    match kind {
        Kind::Toggle => "true or false".to_owned(),
        Kind::Choice(options) => {
            let names: Vec<String> = options.iter().map(|name| format!(":{name}")).collect();
            format!("one of {}", names.join(", "))
        }
        Kind::Theme => "a theme name".to_owned(),
        Kind::Integer => "a whole number".to_owned(),
        Kind::Number => "a number".to_owned(),
        Kind::Text => "a string".to_owned(),
    }
}

fn to_json(heap: &Heap, setting: &Setting, value: Value) -> Result<Option<Json>, Error> {
    if value == Value::Nil {
        return Ok(None);
    }
    let converted = match (setting.kind, value) {
        (Kind::Toggle, Value::Bool(on)) => Some(Json::Bool(on)),
        (Kind::Integer | Kind::Number, Value::Int(number)) => Some(Json::from(number)),
        (Kind::Number, Value::Float(number)) => Some(Json::from(number)),
        (Kind::Choice(options), _) => name_of(heap, value)
            .filter(|name| options.contains(&name.as_str()))
            .map(Json::from),
        (Kind::Theme, _) => name_of(heap, value).map(Json::from),
        (Kind::Text, _) => heap.text(value).map(Json::from),
        _ => None,
    };
    converted.map(Some).ok_or_else(|| {
        Error::new(format!(
            "{} must be {}, got {}",
            setting.key,
            wanted(setting.kind),
            print::brief(heap, value)
        ))
    })
}

fn to_ox(heap: &mut Heap, setting: &Setting, value: Option<Json>) -> Result<Value, Error> {
    Ok(match value {
        None | Some(Json::Null) => Value::Nil,
        Some(Json::Bool(on)) => Value::Bool(on),
        Some(Json::Number(number)) if setting.kind == Kind::Number => {
            Value::Float(number.as_f64().unwrap_or_default())
        }
        Some(Json::Number(number)) => Value::Int(number.as_i64().unwrap_or_default()),
        Some(Json::String(text)) if matches!(setting.kind, Kind::Choice(_) | Kind::Theme) => {
            Value::keyword(&text)
        }
        Some(Json::String(text)) => heap.string(&text)?,
        Some(other) => heap.string(&other.to_string())?,
    })
}

#[derive(Debug)]
struct Saved(Config);

#[derive(Debug)]
struct Custom(Config);

pub fn mark_saved(editor: &mut Editor) {
    let config = editor.config().clone();
    editor.set_plugin_state(Saved(config));
}

pub fn mark_custom(editor: &mut Editor) {
    let config = editor.config().clone();
    editor.set_plugin_state(Custom(config));
}

fn as_map(heap: &mut Heap, config: &Config) -> Result<Value, Error> {
    let mut items = Vec::new();
    for setting in SETTINGS {
        items.push(Value::keyword(setting.key));
        items.push(to_ox(heap, setting, config.get(setting.key))?);
    }
    heap.map_from(&items)
}

fn saved_config(editor: &Editor) -> Config {
    editor
        .plugin_state::<Saved>()
        .map_or_else(Config::default, |Saved(config)| config.clone())
}

fn custom_config(editor: &Editor) -> Config {
    editor
        .plugin_state::<Custom>()
        .map_or_else(Config::default, |Custom(config)| config.clone())
}

fn home(heap: &mut Heap, editor: &Editor) -> Result<Value, Error> {
    match editor.init_path().and_then(Path::parent) {
        Some(home) => heap
            .string(&home.display().to_string())
            .map_err(Error::from),
        None => Ok(Value::Nil),
    }
}

fn change(heap: &Heap, editor: &mut Editor, changes: Value) -> Result<Value, Error> {
    let entries = heap
        .entries(changes)
        .ok_or_else(|| Error::new("settings takes a map, such as {tab-width: 4}"))?;
    let mut config = editor.config().clone();
    for (key, value) in entries {
        let key = name_of(heap, key).unwrap_or_else(|| print::brief(heap, key));
        let setting = schema::find(&key)
            .ok_or_else(|| Error::new(format!("There is no setting named {key}")))?;
        config = config
            .with(&key, to_json(heap, setting, value)?)
            .map_err(|error| Error::new(error.to_string()))?;
    }
    if &config != editor.config() {
        editor.reconfigure(&config);
    }
    Ok(Value::Nil)
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Toggle => "toggle",
        Kind::Choice(_) => "choice",
        Kind::Theme => "theme",
        Kind::Integer => "integer",
        Kind::Number => "number",
        Kind::Text => "text",
    }
}

fn described(
    heap: &mut Heap,
    editor: &Editor,
    setting: &Setting,
    configs: [&Config; 3],
) -> Result<Value, Error> {
    let choices: Vec<Value> = match setting.kind {
        Kind::Choice(options) => options.iter().map(|name| Value::keyword(name)).collect(),
        Kind::Theme => editor
            .themes()
            .iter()
            .map(|theme| Value::keyword(theme.name))
            .collect(),
        _ => Vec::new(),
    };
    let choices = if choices.is_empty() {
        Value::Nil
    } else {
        heap.vector_from(&choices)?
    };
    let group = heap.string(setting.group)?;
    let doc = heap.string(setting.doc)?;
    let unset = match setting.unset {
        Some(unset) => heap.string(unset)?,
        None => Value::Nil,
    };
    let default = Config::default().get(setting.key);
    let default = to_ox(heap, setting, default)?;
    let [value, saved, custom] = configs;
    let value = to_ox(heap, setting, value.get(setting.key))?;
    let saved = to_ox(heap, setting, saved.get(setting.key))?;
    let custom = to_ox(heap, setting, custom.get(setting.key))?;
    heap.map_from(&[
        Value::keyword("key"),
        Value::keyword(setting.key),
        Value::keyword("owner"),
        Value::Nil,
        Value::keyword("group"),
        group,
        Value::keyword("kind"),
        Value::keyword(kind_name(setting.kind)),
        Value::keyword("doc"),
        doc,
        Value::keyword("unset"),
        unset,
        Value::keyword("default"),
        default,
        Value::keyword("choices"),
        choices,
        Value::keyword("value"),
        value,
        Value::keyword("saved"),
        saved,
        Value::keyword("custom"),
        custom,
    ])
}

fn schema_of(heap: &mut Heap, editor: &mut Editor) -> Result<Value, Error> {
    let (saved, custom) = (saved_config(editor), custom_config(editor));
    let now = editor.config().clone();
    let mut entries = Vec::new();
    for setting in SETTINGS {
        entries.push(described(heap, editor, setting, [&now, &saved, &custom])?);
    }
    entries.extend(crate::runtime::plugins::described(heap, editor)?);
    heap.vector_from(&entries).map_err(Error::from)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "settings")
        .arity(0, Some(2))
        .host::<Editor>(|heap, editor, arguments| match arguments {
            [] => as_map(heap, editor.config()),
            [changes] => change(heap, editor, *changes),
            [plugin, changes] => {
                crate::runtime::plugins::configure_named(heap, editor, *plugin, *changes)
            }
            _ => unreachable!("settings takes at most two arguments"),
        });
    isolate
        .native("ox", "settings-schema")
        .arity(0, Some(0))
        .host::<Editor>(|heap, editor, _| schema_of(heap, editor));

    isolate
        .native("ox", "reload-init")
        .arity(0, Some(0))
        .host::<Editor>(|_, editor, _| {
            repl::reload_init(editor);
            Ok(Value::Nil)
        });
    isolate
        .native("ox", "oxmacs-home")
        .arity(0, Some(0))
        .host::<Editor>(|heap, editor, _| home(heap, editor));
    isolate
        .native("ox", "plugin-folder")
        .arity(1, Some(1))
        .keywords(&["cache"])
        .host::<Editor>(|heap, editor, arguments| {
            let name = match arguments[0] {
                Value::Symbol(name) | Value::Keyword(name) => name.name().to_owned(),
                other => print::display(heap, other),
            };
            if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
                return Err(Error::new(format!(
                    "plugin-folder expects a plugin's name, got {}",
                    print::brief(heap, arguments[0])
                )));
            }
            match editor.folders() {
                Some(folders) => {
                    let folder = folders.plugin(&name, arguments[1].truthy());
                    heap.string(&folder.display().to_string())
                        .map_err(Error::from)
                }
                None => Ok(Value::Nil),
            }
        });
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::{Editor, Effect};

    use crate::Ox;

    fn evaluated(source: &str) -> (Editor, Result<String, String>) {
        let mut editor = Editor::new();
        let result = Ox::new()
            .eval(&mut editor, source)
            .map_err(|error| error.message);
        (editor, result)
    }

    #[test]
    fn settings_change_the_editor_at_once() {
        let (mut editor, result) = evaluated(
            "(settings {tab-width: 4 theme: :adwaita-dark line-numbers: \"relative\" font-family: \"Iosevka\" font-size: 13})",
        );

        assert_eq!(result, Ok("nil".to_owned()));
        let config = editor.config();
        assert_eq!(config.tab_width, 4);
        assert_eq!(config.theme.as_deref(), Some("adwaita-dark"));
        assert_eq!(
            config.line_numbers,
            oxmacs_core::config::LineNumbers::Relative
        );
        assert_eq!(config.font_family.as_deref(), Some("Iosevka"));
        assert!((config.font_size - 13.0).abs() < f32::EPSILON);
        assert_eq!(editor.tab_width(), 4);
        assert!(editor.take_effects().contains(&Effect::Reconfigure));
    }

    #[test]
    fn settings_read_back_as_ox_values() {
        let (_, result) = evaluated(
            "(settings {theme: :adwaita clock: false}) (let [now (settings)] [now.theme now.tab-width now.font-size now.clock now.font-family now.window-mode])",
        );

        assert_eq!(
            result,
            Ok("[:adwaita 8 11.0 false nil :windowed]".to_owned())
        );
    }

    #[test]
    fn nil_puts_a_setting_back_to_its_default() {
        let (editor, result) = evaluated(
            "(settings {theme: :adwaita tab-width: 2}) (settings {theme: nil tab-width: nil})",
        );

        assert_eq!(result, Ok("nil".to_owned()));
        assert_eq!(editor.config().theme, None);
        assert_eq!(editor.config().tab_width, 8);
    }

    #[test]
    fn mistakes_are_named_and_change_nothing() {
        for (source, message) in [
            (
                "(settings {tab-width: \"wide\"})",
                "tab-width must be a whole number, got \"wide\"",
            ),
            (
                "(settings {font-sise: 12})",
                "There is no setting named font-sise",
            ),
            ("(settings {tab-width: 0})", "tab-width must be at least 1"),
            (
                "(settings {tab-width: 2 line-numbers: :roman})",
                "line-numbers must be one of :absolute, :relative, :off, got :roman",
            ),
            (
                "(settings {theme: :solarized})",
                "unknown theme `solarized`",
            ),
            (
                "(settings [1 2])",
                "settings takes a map, such as {tab-width: 4}",
            ),
        ] {
            let (editor, result) = evaluated(source);

            assert_eq!(result, Err(message.to_owned()), "{source}");
            assert_eq!(editor.config(), &oxmacs_core::config::Config::default());
        }
    }

    #[test]
    fn the_schema_describes_every_setting() {
        let (_, result) = evaluated(
            "(let [all (settings-schema) theme (first all) tabs (find (fn [setting] (= setting.key :tab-width)) all)] [(count all) theme.kind theme.unset (first theme.choices) tabs.kind tabs.default tabs.group])",
        );

        assert_eq!(
            result,
            Ok(format!(
                "[{} :theme \"follow OS\" :adwaita :integer 8 \"Editing\"]",
                oxmacs_core::config::schema::SETTINGS.len()
            ))
        );
    }
}
