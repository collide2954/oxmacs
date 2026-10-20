pub mod schema;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use toml::Spanned;
use toml::de::{DeTable, DeValue};

use crate::{clock, files, theme};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is its own config.toml setting"
)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    pub font_size: f32,
    pub tab_width: usize,
    pub animations: bool,
    pub cursor_blink_interval: u64,
    pub on_disk_change: OnDiskChange,
    pub delete_to_trash: bool,
    pub splash: bool,
    pub format_on_save: bool,
    pub clock: bool,
    pub clock_format: String,
    pub rust_lint: RustLint,
    pub auto_save: bool,
    pub line_numbers: LineNumbers,
    pub line_number_separator: bool,
    pub window_mode: WindowMode,
    pub mode_line_style: ModeLineStyle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather_location: Option<String>,
    pub weather_units: WeatherUnits,
    pub mode_line_weather: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub irc_server: Vec<IrcServer>,
    #[serde(skip_serializing_if = "IrcBouncer::is_default")]
    pub irc_bouncer: IrcBouncer,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r_program: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air_program: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub atlas_vault: Vec<AtlasVault>,
    #[serde(skip)]
    pub(crate) applets: BTreeMap<String, toml::Table>,
}

const CORE_TABLES: &[&str] = &["irc-bouncer"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moved {
    pub from: &'static str,
    pub table: &'static str,
    pub key: &'static str,
}

const MOVED: &[Moved] = &[];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct AtlasVault {
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl AtlasVault {
    pub fn directory(&self, home: Option<&Path>) -> PathBuf {
        match (self.path.strip_prefix("~"), home) {
            (Some(rest), Some(home)) => home.join(rest.trim_start_matches(['/', '\\'])),
            _ => PathBuf::from(&self.path),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnDiskChange {
    #[default]
    Revert,
    Ask,
    Ignore,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineNumbers {
    #[default]
    Absolute,
    Relative,
    Off,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowMode {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModeLineStyle {
    #[default]
    Classic,
    Modern,
    Minimal,
    Powerline,
    Boxed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct IrcServer {
    pub name: String,
    pub host: String,
    #[serde(default = "irc_port")]
    pub port: u16,
    #[serde(default = "enabled")]
    pub tls: bool,
    pub nick: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realname: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sasl: Option<String>,
    #[serde(default)]
    pub password: PasswordSource,
}

impl IrcServer {
    pub fn username(&self) -> &str {
        self.username.as_deref().unwrap_or(&self.nick)
    }

    pub fn realname(&self) -> &str {
        self.realname.as_deref().unwrap_or("oxmacs")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct IrcBouncer {
    pub mode: BouncerMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    pub tls: bool,
    pub password: PasswordSource,
    pub listen: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<PathBuf>,
}

impl Default for IrcBouncer {
    fn default() -> Self {
        Self {
            mode: BouncerMode::Off,
            address: None,
            tls: true,
            password: PasswordSource::Keychain,
            listen: "127.0.0.1:0".to_owned(),
            certificate: None,
            key: None,
        }
    }
}

impl IrcBouncer {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BouncerMode {
    #[default]
    Off,
    Local,
    Remote,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PasswordSource {
    #[default]
    Keychain,
    Ask,
    Env(String),
    Text(String),
}

fn irc_port() -> u16 {
    6697
}

fn enabled() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeatherUnits {
    #[default]
    Metric,
    Imperial,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RustLint {
    #[default]
    Check,
    Clippy,
    Off,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: None,
            font_family: None,
            font_size: 11.0,
            tab_width: 8,
            animations: true,
            cursor_blink_interval: 400,
            on_disk_change: OnDiskChange::default(),
            delete_to_trash: true,
            splash: true,
            format_on_save: false,
            clock: true,
            clock_format: "%H:%M".to_owned(),
            rust_lint: RustLint::default(),
            auto_save: true,
            line_numbers: LineNumbers::default(),
            line_number_separator: false,
            window_mode: WindowMode::default(),
            mode_line_style: ModeLineStyle::default(),
            weather_location: None,
            weather_units: WeatherUnits::default(),
            mode_line_weather: false,
            irc_server: Vec::new(),
            irc_bouncer: IrcBouncer::default(),
            r_program: None,
            air_program: None,
            atlas_vault: Vec::new(),
            applets: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0}")]
    Syntax(String),
    #[error("unknown theme `{0}`")]
    UnknownTheme(String),
    #[error("tab-width must be at least 1")]
    TabWidth,
    #[error("font-size must be a positive number of points")]
    FontSize,
    #[error("clock-format: {0}")]
    ClockFormat(String),
    #[error("irc-server: {0}")]
    IrcServer(String),
    #[error("irc-bouncer: {0}")]
    IrcBouncer(String),
    #[error("atlas-vault: {0}")]
    AtlasVault(String),
    #[error("{}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("{}: {reason}", path.display())]
    Migrate { path: PathBuf, reason: String },
    #[error("{}: {source}", path.display())]
    Invalid {
        path: PathBuf,
        source: Box<ConfigError>,
    },
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        Self::parse_with(text, MOVED)
    }

    fn parse_with(text: &str, moved: &[Moved]) -> Result<Self, ConfigError> {
        let root = DeTable::parse(text).map_err(|error| syntax_error(text, &error))?;
        let span = root.span();
        let mut core = DeTable::new();
        let mut names = Vec::new();
        let mut relocated = Vec::new();
        for (key, value) in root.into_inner() {
            let name: &str = key.get_ref();
            if let Some(entry) = moved.iter().find(|entry| entry.from == name) {
                relocated.push(*entry);
            } else if matches!(value.get_ref(), DeValue::Table(_)) && !CORE_TABLES.contains(&name) {
                names.push(name.to_owned());
            } else {
                core.insert(key, value);
            }
        }
        let mut config = Self::deserialize(toml::de::Deserializer::from(Spanned::new(span, core)))
            .map_err(|error| syntax_error(text, &error))?;
        if !names.is_empty() || !relocated.is_empty() {
            let mut all: toml::Table =
                toml::from_str(text).map_err(|error| syntax_error(text, &error))?;
            for name in names {
                if let Some(toml::Value::Table(table)) = all.remove(&name) {
                    config.applets.insert(name, table);
                }
            }
            for entry in relocated {
                if let Some(value) = all.remove(entry.from) {
                    config
                        .applets
                        .entry(entry.table.to_owned())
                        .or_default()
                        .entry(entry.key)
                        .or_insert(value);
                }
            }
        }
        config.validate()?;
        Ok(config)
    }

    pub fn value(&self, table: Option<&str>, key: &str) -> Option<toml::Value> {
        match table {
            None => self.get(key),
            Some(table) => self.applets.get(table)?.get(key).cloned(),
        }
    }

    pub fn with_value(
        &self,
        table: Option<&str>,
        key: &str,
        value: Option<toml::Value>,
    ) -> Result<Self, ConfigError> {
        let Some(name) = table else {
            return self.with(key, value);
        };
        let mut config = self.clone();
        let entries = config.applets.entry(name.to_owned()).or_default();
        match value {
            Some(value) => {
                entries.insert(key.to_owned(), value);
            }
            None => {
                entries.remove(key);
            }
        }
        if entries.is_empty() {
            config.applets.remove(name);
        }
        Ok(config)
    }

    pub fn applet_tables(&self) -> Vec<&str> {
        self.applets.keys().map(String::as_str).collect()
    }

    pub fn applet<T: DeserializeOwned + Default>(&self, name: &str) -> Result<T, String> {
        match self.applets.get(name) {
            None => Ok(T::default()),
            Some(table) => toml::Value::Table(table.clone())
                .try_into()
                .map_err(|error: toml::de::Error| format!("[{name}] {}", error.message())),
        }
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        Self::parse(&text).map_err(|source| ConfigError::Invalid {
            path: path.to_owned(),
            source: Box::new(source),
        })
    }

    pub fn get(&self, key: &str) -> Option<toml::Value> {
        match toml::Value::try_from(self) {
            Ok(toml::Value::Table(mut table)) => table.remove(key),
            _ => None,
        }
    }

    pub fn with(&self, key: &str, value: Option<toml::Value>) -> Result<Self, ConfigError> {
        let Ok(toml::Value::Table(mut table)) = toml::Value::try_from(self) else {
            return Err(ConfigError::Syntax(format!("cannot change {key}")));
        };
        match value {
            Some(value) => table.insert(key.to_owned(), value),
            None => table.remove(key),
        };
        let mut config: Self = toml::Value::Table(table)
            .try_into()
            .map_err(|error: toml::de::Error| ConfigError::Syntax(error.message().to_owned()))?;
        config.validate()?;
        config.applets.clone_from(&self.applets);
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if let Some(name) = &self.theme
            && theme::find(name).is_none()
        {
            return Err(ConfigError::UnknownTheme(name.clone()));
        }
        if self.tab_width == 0 {
            return Err(ConfigError::TabWidth);
        }
        if !(self.font_size.is_finite() && self.font_size > 0.0) {
            return Err(ConfigError::FontSize);
        }
        let sample = jiff::Timestamp::UNIX_EPOCH.to_zoned(jiff::tz::TimeZone::UTC);
        clock::text(&self.clock_format, &sample).map_err(ConfigError::ClockFormat)?;
        if self.irc_bouncer.mode == BouncerMode::Remote && self.irc_bouncer.address.is_none() {
            return Err(ConfigError::IrcBouncer(
                "mode = \"remote\" needs an address".to_owned(),
            ));
        }
        for (index, server) in self.irc_server.iter().enumerate() {
            if server.nick.trim().is_empty() {
                return Err(ConfigError::IrcServer(format!(
                    "{} needs a nick",
                    server.name
                )));
            }
            if self.irc_server[..index]
                .iter()
                .any(|earlier| earlier.name == server.name)
            {
                return Err(ConfigError::IrcServer(format!(
                    "two servers are named {}",
                    server.name
                )));
            }
        }
        for (index, vault) in self.atlas_vault.iter().enumerate() {
            if vault.path.trim().is_empty() {
                return Err(ConfigError::AtlasVault(format!(
                    "{} needs a path",
                    vault.name
                )));
            }
            if self.atlas_vault[..index]
                .iter()
                .any(|earlier| earlier.name == vault.name)
            {
                return Err(ConfigError::AtlasVault(format!(
                    "two vaults are named {}",
                    vault.name
                )));
            }
        }
        Ok(())
    }
}

pub fn migrate_file(path: &Path) -> Result<Option<String>, ConfigError> {
    migrate_file_with(path, MOVED)
}

fn migrate_file_with(path: &Path, moved: &[Moved]) -> Result<Option<String>, ConfigError> {
    if !path.is_file() {
        return Ok(None);
    }
    let failed = |reason: String| ConfigError::Migrate {
        path: path.to_owned(),
        reason,
    };
    let loaded = files::load(path).map_err(|error| failed(error.to_string()))?;
    let Some((text, entries)) = migrate_with(&loaded.text, moved) else {
        return Ok(None);
    };
    files::save(path, &text, loaded.format).map_err(|error| failed(error.to_string()))?;
    let mut tables: Vec<(&str, Vec<&str>)> = Vec::new();
    for entry in &entries {
        match tables.iter_mut().find(|(table, _)| *table == entry.table) {
            Some((_, keys)) => keys.push(entry.from),
            None => tables.push((entry.table, vec![entry.from])),
        }
    }
    let parts: Vec<String> = tables
        .iter()
        .map(|(table, keys)| format!("{} to [{table}]", keys.join(", ")))
        .collect();
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    Ok(Some(format!("Moved {} in {name}", parts.join("; "))))
}

fn migrate_with(text: &str, moved: &[Moved]) -> Option<(String, Vec<Moved>)> {
    let mut document: toml_edit::DocumentMut = text.parse().ok()?;
    let mut done = Vec::new();
    for entry in moved {
        let Some((key, item)) = document.remove_entry(entry.from) else {
            continue;
        };
        done.push(*entry);
        if !document.contains_key(entry.table) {
            document.insert(entry.table, toml_edit::table());
        }
        let Some(table) = document
            .get_mut(entry.table)
            .and_then(toml_edit::Item::as_table_mut)
        else {
            continue;
        };
        if table.contains_key(entry.key) {
            continue;
        }
        let mut renamed = toml_edit::Key::new(entry.key);
        *renamed.leaf_decor_mut() = key.leaf_decor().clone();
        table.insert_formatted(&renamed, item);
    }
    (!done.is_empty()).then(|| (document.to_string(), done))
}

pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("oxmacs").join("config.toml"))
}

pub fn rewrite(text: &str, config: &Config) -> Result<String, ConfigError> {
    let on_disk = Config::parse(text)?;
    let mut document: toml_edit::DocumentMut = text
        .parse()
        .map_err(|error: toml_edit::TomlError| ConfigError::Syntax(error.message().to_owned()))?;
    for setting in schema::SETTINGS {
        let wanted = config.get(setting.key);
        if wanted == on_disk.get(setting.key) {
            continue;
        }
        let Some(wanted) = wanted else {
            document.remove(setting.key);
            continue;
        };
        let old = document
            .get(setting.key)
            .and_then(toml_edit::Item::as_value);
        document[setting.key] = toml_edit::Item::Value(edited(&wanted, old)?);
    }
    let empty = toml::Table::new();
    let names: BTreeSet<&String> = config
        .applets
        .keys()
        .chain(on_disk.applets.keys())
        .collect();
    for name in names {
        let wanted = config.applets.get(name).unwrap_or(&empty);
        let found = on_disk.applets.get(name).unwrap_or(&empty);
        let keys: BTreeSet<&String> = wanted.keys().chain(found.keys()).collect();
        for key in keys {
            if wanted.get(key) == found.get(key) {
                continue;
            }
            if !document.contains_key(name) {
                document.insert(name, toml_edit::table());
            }
            let Some(table) = document
                .get_mut(name)
                .and_then(toml_edit::Item::as_table_like_mut)
            else {
                continue;
            };
            match wanted.get(key) {
                None => {
                    table.remove(key);
                }
                Some(value) => {
                    let old = table.get(key).and_then(toml_edit::Item::as_value);
                    let value = edited(value, old)?;
                    table.insert(key, toml_edit::Item::Value(value));
                }
            }
        }
    }
    Ok(document.to_string())
}

fn edited(
    wanted: &toml::Value,
    old: Option<&toml_edit::Value>,
) -> Result<toml_edit::Value, ConfigError> {
    let mut value: toml_edit::Value = wanted
        .to_string()
        .parse()
        .map_err(|error: toml_edit::TomlError| ConfigError::Syntax(error.message().to_owned()))?;
    if let Some(old) = old {
        *value.decor_mut() = old.decor().clone();
    }
    Ok(value)
}

fn edit_error(error: &toml_edit::TomlError) -> ConfigError {
    ConfigError::Syntax(error.message().to_owned())
}

fn table_of(value: &impl Serialize) -> Result<toml_edit::Table, ConfigError> {
    let fields = toml::to_string(value).map_err(|error| ConfigError::Syntax(error.to_string()))?;
    let parsed: toml_edit::DocumentMut = fields.parse().map_err(|error| edit_error(&error))?;
    let mut table = parsed.as_table().clone();
    table.set_implicit(false);
    Ok(table)
}

fn push_table(document: &mut toml_edit::DocumentMut, key: &str, table: toml_edit::Table) {
    if let Some(toml_edit::Item::ArrayOfTables(tables)) = document.get_mut(key) {
        tables.push(table);
    } else {
        let mut tables = toml_edit::ArrayOfTables::new();
        tables.push(table);
        document.insert(key, toml_edit::Item::ArrayOfTables(tables));
    }
}

pub fn add_atlas_vault(text: &str, vault: &AtlasVault) -> Result<String, ConfigError> {
    let mut document: toml_edit::DocumentMut = text.parse().map_err(|error| edit_error(&error))?;
    push_table(&mut document, "atlas-vault", table_of(vault)?);
    let rewritten = document.to_string();
    Config::parse(&rewritten)?;
    Ok(rewritten)
}

pub fn add_irc_server(
    text: &str,
    server: &IrcServer,
    keep_alive: bool,
) -> Result<String, ConfigError> {
    let mut document: toml_edit::DocumentMut = text.parse().map_err(|error| edit_error(&error))?;
    let mut table = table_of(server)?;
    if let Some(password) = table.remove("password") {
        let value = match password {
            toml_edit::Item::Table(inner) => {
                toml_edit::Value::InlineTable(inner.into_inline_table())
            }
            other => other
                .into_value()
                .map_err(|_| ConfigError::Syntax("password".to_owned()))?,
        };
        table.insert("password", toml_edit::Item::Value(value));
    }
    push_table(&mut document, "irc-server", table);
    if keep_alive {
        if !document.contains_table("irc-bouncer") {
            document.insert("irc-bouncer", toml_edit::table());
        }
        document["irc-bouncer"]["mode"] = toml_edit::value("local");
    }
    let rewritten = document.to_string();
    Config::parse(&rewritten)?;
    Ok(rewritten)
}

fn syntax_error(text: &str, error: &toml::de::Error) -> ConfigError {
    let line = error
        .span()
        .and_then(|span| text.get(..span.start))
        .map(|before| before.matches('\n').count() + 1);
    ConfigError::Syntax(match line {
        Some(line) => format!("line {line}: {}", error.message()),
        None => error.message().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct Dice {
        sides: u8,
    }

    #[test]
    fn tables_core_does_not_know_are_kept_for_applets() {
        let config =
            Config::parse("tab-width = 4\n\n[dice]\nsides = 8\n\n[elsewhere]\nx = 1\n").unwrap();

        assert_eq!(config.tab_width, 4);
        assert_eq!(config.applet::<Dice>("dice"), Ok(Dice { sides: 8 }));
        assert_eq!(config.applet::<Dice>("absent"), Ok(Dice::default()));
        assert!(config.applet::<Dice>("elsewhere").is_err());
        assert_eq!(config.applet_tables(), ["dice", "elsewhere"]);
        assert_eq!(
            config
                .with("tab-width", Some(toml::Value::Integer(2)))
                .unwrap()
                .applet::<Dice>("dice"),
            Ok(Dice { sides: 8 })
        );
    }

    const MOVES: &[Moved] = &[
        Moved {
            from: "dice-sides",
            table: "dice",
            key: "sides",
        },
        Moved {
            from: "dice-color",
            table: "dice",
            key: "color",
        },
    ];

    #[test]
    fn moved_keys_still_work_from_their_old_place() {
        let config = Config::parse_with("tab-width = 2\ndice-sides = 8\n", MOVES).unwrap();
        assert_eq!(config.tab_width, 2);
        assert_eq!(config.applet::<Dice>("dice"), Ok(Dice { sides: 8 }));

        let both = Config::parse_with("dice-sides = 8\n\n[dice]\nsides = 12\n", MOVES).unwrap();
        assert_eq!(both.applet::<Dice>("dice"), Ok(Dice { sides: 12 }));
    }

    #[test]
    fn migrating_moves_old_keys_into_their_table_with_their_comments() {
        let text = "# mine\ntab-width = 2\n# how many\ndice-sides = 8 # big\n";

        let (rewritten, moved) = migrate_with(text, MOVES).unwrap();

        assert_eq!(moved, [MOVES[0]]);
        assert_eq!(
            rewritten,
            "# mine\ntab-width = 2\n\n[dice]\n# how many\nsides = 8 # big\n"
        );
        assert_eq!(migrate_with(&rewritten, MOVES), None);
        assert_eq!(migrate_with("not = [valid", MOVES), None);
    }

    #[test]
    fn migrating_keeps_a_value_already_in_the_table() {
        let (rewritten, _) = migrate_with("dice-sides = 8\n\n[dice]\nsides = 12\n", MOVES).unwrap();

        assert!(!rewritten.contains("dice-sides"), "{rewritten}");
        assert!(rewritten.contains("sides = 12"), "{rewritten}");
        assert!(!rewritten.contains("sides = 8"), "{rewritten}");
    }

    #[test]
    fn config_files_are_migrated_once_with_a_note_of_what_moved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "dice-color = \"red\"\ndice-sides = 8\n").unwrap();

        let note = migrate_file_with(&path, MOVES).unwrap();

        assert_eq!(
            note.as_deref(),
            Some("Moved dice-sides, dice-color to [dice] in config.toml")
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[dice]\nsides = 8\ncolor = \"red\"\n"
        );
        assert_eq!(migrate_file_with(&path, MOVES).unwrap(), None);
        assert_eq!(
            migrate_file_with(&dir.path().join("missing.toml"), MOVES).unwrap(),
            None
        );
    }

    #[test]
    fn unknown_plain_keys_are_still_mistakes() {
        assert!(Config::parse("dice = 8\n").is_err());
        assert!(Config::parse("[irc-bouncer]\nmode = \"local\"\n").is_ok());
    }

    #[test]
    fn empty_config_uses_defaults() {
        let config = Config::parse("").unwrap();

        assert_eq!(config, Config::default());
        assert_eq!(config.theme, None);
        assert_eq!(config.font_family, None);
        assert!((config.font_size - 11.0).abs() < f32::EPSILON);
        assert_eq!(config.tab_width, 8);
        assert!(config.animations);
    }

    #[test]
    fn cursor_blink_interval_defaults_to_400ms_and_accepts_zero() {
        assert_eq!(Config::default().cursor_blink_interval, 400);
        assert_eq!(
            Config::parse("cursor-blink-interval = 0")
                .unwrap()
                .cursor_blink_interval,
            0
        );
    }

    #[test]
    fn on_disk_change_defaults_to_revert() {
        let read = |text| Config::parse(text).map(|config| config.on_disk_change);

        assert_eq!(Config::default().on_disk_change, OnDiskChange::Revert);
        assert_eq!(read("on-disk-change = \"ask\"").unwrap(), OnDiskChange::Ask);
        assert_eq!(
            read("on-disk-change = \"ignore\"").unwrap(),
            OnDiskChange::Ignore
        );
        assert!(read("on-disk-change = \"panic\"").is_err());
    }

    #[test]
    fn deleting_moves_to_the_trash_unless_turned_off() {
        assert!(Config::default().delete_to_trash);
        assert!(
            !Config::parse("delete-to-trash = false")
                .unwrap()
                .delete_to_trash
        );
    }

    #[test]
    fn the_clock_shows_hours_and_minutes_unless_configured() {
        let config = Config::default();
        assert!(config.clock);
        assert_eq!(config.clock_format, "%H:%M");

        let config = Config::parse("clock = false\nclock-format = \"%a %H:%M\"").unwrap();
        assert!(!config.clock);
        assert_eq!(config.clock_format, "%a %H:%M");
    }

    #[test]
    fn invalid_clock_formats_are_rejected() {
        let error = Config::parse("clock-format = \"%H %\"").unwrap_err();

        assert!(error.to_string().starts_with("clock-format: "), "{error}");
    }

    #[test]
    fn formatting_on_save_is_opt_in() {
        assert!(!Config::default().format_on_save);
        assert!(
            Config::parse("format-on-save = true")
                .unwrap()
                .format_on_save
        );
    }

    fn libera(password: PasswordSource) -> IrcServer {
        IrcServer {
            name: "libera".to_owned(),
            host: "irc.libera.chat".to_owned(),
            port: 6697,
            tls: true,
            nick: "brett".to_owned(),
            username: None,
            realname: None,
            channels: vec!["#emacs".to_owned(), "#rust".to_owned()],
            sasl: Some("brett".to_owned()),
            password,
        }
    }

    #[test]
    fn irc_servers_are_added_without_disturbing_the_file() {
        let text = "# my settings\ntheme = \"adwaita\" # light\n";

        let added = add_irc_server(text, &libera(PasswordSource::Keychain), false).unwrap();

        assert_eq!(
            added,
            "# my settings\ntheme = \"adwaita\" # light\n\n[[irc-server]]\nname = \"libera\"\nhost = \"irc.libera.chat\"\nport = 6697\ntls = true\nnick = \"brett\"\nchannels = [\"#emacs\", \"#rust\"]\nsasl = \"brett\"\npassword = \"keychain\"\n"
        );
        let config = Config::parse(&added).unwrap();
        assert_eq!(config.theme.as_deref(), Some("adwaita"));
        assert_eq!(config.irc_server, [libera(PasswordSource::Keychain)]);
        assert_eq!(config.irc_bouncer, IrcBouncer::default());
    }

    #[test]
    fn added_servers_can_turn_on_the_local_bouncer_and_read_env_passwords() {
        let text = "[irc-bouncer]\ntls = true\n";
        let server = libera(PasswordSource::Env("LIBERA_PASSWORD".to_owned()));

        let added = add_irc_server(text, &server, true).unwrap();

        let config = Config::parse(&added).unwrap();
        assert_eq!(config.irc_bouncer.mode, BouncerMode::Local);
        assert_eq!(config.irc_server, [server]);
        assert!(
            added.contains("password = { env = \"LIBERA_PASSWORD\" }"),
            "{added}"
        );
        let again = add_irc_server(
            &added,
            &IrcServer {
                name: "oftc".to_owned(),
                host: "irc.oftc.net".to_owned(),
                ..libera(PasswordSource::Ask)
            },
            false,
        )
        .unwrap();
        assert_eq!(Config::parse(&again).unwrap().irc_server.len(), 2);
        assert_eq!(
            add_irc_server(&again, &libera(PasswordSource::Ask), false)
                .unwrap_err()
                .to_string(),
            "irc-server: two servers are named libera"
        );
    }

    #[test]
    fn atlas_vaults_are_named_folders() {
        let config = Config::parse(
            "[[atlas-vault]]\nname = \"personal\"\npath = \"~/Atlas\"\n\n[[atlas-vault]]\nname = \"work\"\npath = \"C:/Work/notes\"\ncolor = \"blue\"\n",
        )
        .unwrap();

        let [personal, work] = &config.atlas_vault[..] else {
            panic!("{:?}", config.atlas_vault);
        };
        assert_eq!(personal.name, "personal");
        assert_eq!(personal.color, None);
        assert_eq!(work.color.as_deref(), Some("blue"));
        let home = Path::new("/home/brett");
        assert_eq!(personal.directory(Some(home)), home.join("Atlas"));
        assert_eq!(work.directory(Some(home)), PathBuf::from("C:/Work/notes"));
    }

    #[test]
    fn atlas_vaults_need_unique_names_and_a_path() {
        let twice = "[[atlas-vault]]\nname = \"a\"\npath = \"x\"\n[[atlas-vault]]\nname = \"a\"\npath = \"y\"\n";
        assert_eq!(
            Config::parse(twice).unwrap_err().to_string(),
            "atlas-vault: two vaults are named a"
        );
        assert_eq!(
            Config::parse("[[atlas-vault]]\nname = \"a\"\npath = \"\"\n")
                .unwrap_err()
                .to_string(),
            "atlas-vault: a needs a path"
        );
    }

    #[test]
    fn atlas_vaults_are_added_in_place() {
        let text = "theme = \"adwaita\"\n";
        let vault = AtlasVault {
            name: "personal".to_owned(),
            path: "~/Atlas".to_owned(),
            color: None,
        };

        let added = add_atlas_vault(text, &vault).unwrap();

        assert_eq!(
            added,
            "theme = \"adwaita\"\n\n[[atlas-vault]]\nname = \"personal\"\npath = \"~/Atlas\"\n"
        );
        assert_eq!(Config::parse(&added).unwrap().atlas_vault, [vault]);
    }

    #[test]
    fn the_air_program_is_found_unless_set() {
        assert_eq!(Config::default().air_program, None);
        assert_eq!(
            Config::parse("air-program = \"C:/tools/air.exe\"")
                .unwrap()
                .air_program
                .as_deref(),
            Some("C:/tools/air.exe")
        );
    }

    #[test]
    fn every_setting_in_the_schema_is_a_config_key() {
        let full = Config {
            theme: Some("adwaita".to_owned()),
            font_family: Some("Iosevka".to_owned()),
            r_program: Some("R".to_owned()),
            air_program: Some("air".to_owned()),
            weather_location: Some("Berlin".to_owned()),
            ..Config::default()
        };
        let toml::Value::Table(table) = toml::Value::try_from(&full).unwrap() else {
            panic!("config is not a table");
        };
        let mut keys: Vec<&str> = table.keys().map(String::as_str).collect();
        let mut schema: Vec<&str> = schema::SETTINGS.iter().map(|setting| setting.key).collect();
        keys.sort_unstable();
        schema.sort_unstable();

        assert_eq!(keys, schema);
    }

    #[test]
    fn the_theme_setting_picks_from_the_known_themes() {
        assert_eq!(
            schema::find("theme").map(|setting| setting.kind),
            Some(schema::Kind::Theme)
        );
    }

    #[test]
    fn settings_are_read_and_changed_by_key() {
        let config = Config::default();
        assert_eq!(config.get("tab-width"), Some(toml::Value::Integer(8)));
        assert_eq!(config.get("theme"), None);

        let config = config
            .with("tab-width", Some(toml::Value::Integer(4)))
            .unwrap()
            .with(
                "theme",
                Some(toml::Value::String("adwaita-dark".to_owned())),
            )
            .unwrap();
        assert_eq!(config.tab_width, 4);
        assert_eq!(config.theme.as_deref(), Some("adwaita-dark"));

        assert_eq!(config.with("theme", None).unwrap().theme, None);
    }

    #[test]
    fn invalid_changes_are_refused() {
        let config = Config::default();

        assert_eq!(
            config
                .with("tab-width", Some(toml::Value::Integer(0)))
                .unwrap_err()
                .to_string(),
            "tab-width must be at least 1"
        );
        assert!(
            config
                .with("rust-lint", Some(toml::Value::String("miri".to_owned())))
                .is_err()
        );
    }

    fn rewritten(text: &str, key: &str, value: Option<toml::Value>) -> String {
        let config = Config::parse(text).unwrap().with(key, value).unwrap();
        rewrite(text, &config).unwrap()
    }

    #[test]
    fn rewriting_changes_only_the_edited_keys_and_keeps_comments() {
        let text = "# my config\ntheme = \"adwaita\" # light\n\n# tabs\ntab-width = 4 # wide\n";

        assert_eq!(
            rewritten(text, "tab-width", Some(toml::Value::Integer(2))),
            "# my config\ntheme = \"adwaita\" # light\n\n# tabs\ntab-width = 2 # wide\n"
        );
    }

    #[test]
    fn rewriting_adds_new_keys_and_removes_unset_ones() {
        assert_eq!(
            rewritten(
                "",
                "rust-lint",
                Some(toml::Value::String("clippy".to_owned()))
            ),
            "rust-lint = \"clippy\"\n"
        );
        assert_eq!(
            rewritten("theme = \"adwaita\"\ntab-width = 4\n", "theme", None),
            "tab-width = 4\n"
        );
    }

    #[test]
    fn broken_files_are_not_rewritten() {
        assert!(rewrite("tab-width = \n", &Config::default()).is_err());
        assert!(rewrite("tab-width = 0\n", &Config::default()).is_err());
    }

    #[test]
    fn rust_lints_use_cargo_check_unless_configured() {
        let read = |text| Config::parse(text).map(|config| config.rust_lint);

        assert_eq!(Config::default().rust_lint, RustLint::Check);
        assert_eq!(read("rust-lint = \"clippy\"").unwrap(), RustLint::Clippy);
        assert_eq!(read("rust-lint = \"off\"").unwrap(), RustLint::Off);
        assert!(read("rust-lint = \"miri\"").is_err());
    }

    #[test]
    fn line_numbers_are_absolute_unless_configured() {
        let read = |text| Config::parse(text).map(|config| config.line_numbers);

        assert_eq!(Config::default().line_numbers, LineNumbers::Absolute);
        assert_eq!(
            read("line-numbers = \"relative\"").unwrap(),
            LineNumbers::Relative
        );
        assert_eq!(read("line-numbers = \"off\"").unwrap(), LineNumbers::Off);
    }

    #[test]
    fn the_line_number_separator_is_opt_in() {
        assert!(!Config::default().line_number_separator);
        assert!(
            Config::parse("line-number-separator = true")
                .unwrap()
                .line_number_separator
        );
    }

    #[test]
    fn the_window_opens_windowed_unless_configured() {
        let read = |text| Config::parse(text).map(|config| config.window_mode);

        assert_eq!(Config::default().window_mode, WindowMode::Windowed);
        assert_eq!(
            read("window-mode = \"maximized\"").unwrap(),
            WindowMode::Maximized
        );
        assert_eq!(
            read("window-mode = \"fullscreen\"").unwrap(),
            WindowMode::Fullscreen
        );
        assert!(read("window-mode = \"minimized\"").is_err());
    }

    #[test]
    fn the_mode_line_is_classic_unless_configured() {
        let read = |text: &str| Config::parse(text).map(|config| config.mode_line_style);

        assert_eq!(Config::default().mode_line_style, ModeLineStyle::Classic);
        for (name, style) in [
            ("modern", ModeLineStyle::Modern),
            ("minimal", ModeLineStyle::Minimal),
            ("powerline", ModeLineStyle::Powerline),
            ("boxed", ModeLineStyle::Boxed),
        ] {
            assert_eq!(
                read(&format!("mode-line-style = \"{name}\"")).unwrap(),
                style
            );
        }
        assert!(read("mode-line-style = \"fancy\"").is_err());
    }

    #[test]
    fn weather_is_metric_and_off_the_mode_line_unless_configured() {
        let config = Config::default();
        assert_eq!(config.weather_location, None);
        assert_eq!(config.weather_units, WeatherUnits::Metric);
        assert!(!config.mode_line_weather);

        let config = Config::parse(
            "weather-location = \"Berlin\"\nweather-units = \"imperial\"\nmode-line-weather = true",
        )
        .unwrap();
        assert_eq!(config.weather_location.as_deref(), Some("Berlin"));
        assert_eq!(config.weather_units, WeatherUnits::Imperial);
        assert!(config.mode_line_weather);
    }

    #[test]
    fn irc_servers_are_tables_with_sensible_defaults() {
        let config = Config::parse(
            r##"
[[irc-server]]
name = "libera"
host = "irc.libera.chat"
nick = "oxtest"
channels = ["#oxmacs-test"]
sasl = "oxtest"

[[irc-server]]
name = "local"
host = "localhost"
port = 6667
tls = false
nick = "me"
password = { env = "LOCAL_IRC" }
"##,
        )
        .unwrap();

        let libera = &config.irc_server[0];
        assert_eq!(libera.port, 6697);
        assert!(libera.tls);
        assert_eq!(libera.username(), "oxtest");
        assert_eq!(libera.realname(), "oxmacs");
        assert_eq!(libera.channels, ["#oxmacs-test"]);
        assert_eq!(libera.password, PasswordSource::Keychain);

        let local = &config.irc_server[1];
        assert_eq!((local.port, local.tls), (6667, false));
        assert_eq!(local.password, PasswordSource::Env("LOCAL_IRC".to_owned()));
        assert_eq!(local.sasl, None);
    }

    #[test]
    fn irc_password_sources_are_spelled_plainly() {
        let read = |password: &str| {
            Config::parse(&format!(
                "[[irc-server]]\nname = \"n\"\nhost = \"h\"\nnick = \"x\"\npassword = {password}\n"
            ))
            .map(|config| config.irc_server[0].password.clone())
        };

        assert_eq!(read("\"ask\"").unwrap(), PasswordSource::Ask);
        assert_eq!(read("\"keychain\"").unwrap(), PasswordSource::Keychain);
        assert_eq!(
            read("{ text = \"hunter2\" }").unwrap(),
            PasswordSource::Text("hunter2".to_owned())
        );
        assert!(read("\"sometimes\"").is_err());
    }

    #[test]
    fn the_irc_bouncer_is_off_unless_configured() {
        let bouncer = Config::default().irc_bouncer;
        assert_eq!(bouncer.mode, BouncerMode::Off);
        assert_eq!(bouncer.listen, "127.0.0.1:0");

        let config = Config::parse(
            "[irc-bouncer]\nmode = \"remote\"\naddress = \"bnc.example.org:6697\"\npassword = \"ask\"\n",
        )
        .unwrap();
        assert_eq!(config.irc_bouncer.mode, BouncerMode::Remote);
        assert_eq!(
            config.irc_bouncer.address.as_deref(),
            Some("bnc.example.org:6697")
        );
        assert!(config.irc_bouncer.tls);
        assert_eq!(config.irc_bouncer.password, PasswordSource::Ask);

        let daemon = Config::parse(
            "[irc-bouncer]\nlisten = \"0.0.0.0:6697\"\ncertificate = \"cert.pem\"\nkey = \"key.pem\"\n",
        )
        .unwrap()
        .irc_bouncer;
        assert_eq!(daemon.listen, "0.0.0.0:6697");
        assert_eq!(daemon.certificate.as_deref(), Some(Path::new("cert.pem")));
    }

    #[test]
    fn remote_bouncers_need_an_address() {
        let error = Config::parse("[irc-bouncer]\nmode = \"remote\"\n").unwrap_err();

        assert_eq!(
            error.to_string(),
            "irc-bouncer: mode = \"remote\" needs an address"
        );
    }

    #[test]
    fn irc_servers_need_distinct_names_and_a_nick() {
        let error = Config::parse(
            "[[irc-server]]\nname = \"a\"\nhost = \"h\"\nnick = \"x\"\n[[irc-server]]\nname = \"a\"\nhost = \"h\"\nnick = \"y\"\n",
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "irc-server: two servers are named a");

        let error =
            Config::parse("[[irc-server]]\nname = \"a\"\nhost = \"h\"\nnick = \"\"\n").unwrap_err();
        assert_eq!(error.to_string(), "irc-server: a needs a nick");
    }

    #[test]
    fn auto_save_is_on_unless_turned_off() {
        assert!(Config::default().auto_save);
        assert!(!Config::parse("auto-save = false").unwrap().auto_save);
    }

    #[test]
    fn the_splash_shows_unless_turned_off() {
        assert!(Config::default().splash);
        assert!(!Config::parse("splash = false").unwrap().splash);
    }

    #[test]
    fn animations_can_be_turned_off() {
        assert!(!Config::parse("animations = false").unwrap().animations);
    }

    #[test]
    fn kebab_case_keys_are_read() {
        let config = Config::parse(
            "theme = \"adwaita-dark\"\nfont-family = \"Iosevka\"\nfont-size = 13.5\ntab-width = 4\n",
        )
        .unwrap();

        assert_eq!(config.theme.as_deref(), Some("adwaita-dark"));
        assert_eq!(config.font_family.as_deref(), Some("Iosevka"));
        assert!((config.font_size - 13.5).abs() < f32::EPSILON);
        assert_eq!(config.tab_width, 4);
    }

    #[test]
    fn unknown_keys_are_rejected_with_their_line() {
        let error = Config::parse("theme = \"adwaita\"\nfont-sise = 12\n").unwrap_err();

        let message = error.to_string();
        assert!(message.starts_with("line 2: "), "{message}");
        assert!(message.contains("font-sise"), "{message}");
    }

    #[test]
    fn wrong_types_are_rejected() {
        assert!(Config::parse("tab-width = \"wide\"").is_err());
    }

    #[test]
    fn unknown_themes_are_rejected() {
        let error = Config::parse("theme = \"solarized\"").unwrap_err();

        assert_eq!(error.to_string(), "unknown theme `solarized`");
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        assert_eq!(
            Config::parse("tab-width = 0").unwrap_err().to_string(),
            "tab-width must be at least 1"
        );
        assert_eq!(
            Config::parse("font-size = 0").unwrap_err().to_string(),
            "font-size must be a positive number of points"
        );
        assert!(Config::parse("font-size = nan").is_err());
    }

    #[test]
    fn missing_file_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();

        let config = Config::load(&dir.path().join("config.toml")).unwrap();

        assert_eq!(config, Config::default());
    }

    #[test]
    fn existing_file_is_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "tab-width = 2\n").unwrap();

        assert_eq!(Config::load(&path).unwrap().tab_width, 2);
    }

    #[test]
    fn file_errors_name_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "tab-width = 0\n").unwrap();

        let message = Config::load(&path).unwrap_err().to_string();

        assert!(message.contains("config.toml"), "{message}");
        assert!(
            message.ends_with("tab-width must be at least 1"),
            "{message}"
        );
    }

    #[test]
    fn default_path_is_config_toml_in_an_oxmacs_directory() {
        if let Some(path) = default_path() {
            assert!(path.ends_with(Path::new("oxmacs").join("config.toml")));
        }
    }
}
