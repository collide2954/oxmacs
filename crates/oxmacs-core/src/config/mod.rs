pub mod schema;

use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use toml::Spanned;
use toml::de::{DeTable, DeValue};

use crate::{clock, theme};

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
    pub r_program: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air_program: Option<String>,
    #[serde(skip)]
    pub(crate) ignored: Vec<String>,
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
            r_program: None,
            air_program: None,
            ignored: Vec::new(),
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
    #[error("{}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },

    #[error("{}: {source}", path.display())]
    Invalid {
        path: PathBuf,
        source: Box<ConfigError>,
    },
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let root = DeTable::parse(text).map_err(|error| syntax_error(text, &error))?;
        let span = root.span();
        let mut core = DeTable::new();
        let mut ignored = Vec::new();
        for (key, value) in root.into_inner() {
            if is_table(value.get_ref()) {
                ignored.push(key.get_ref().to_string());
            } else {
                core.insert(key, value);
            }
        }
        let mut config = Self::deserialize(toml::de::Deserializer::from(Spanned::new(span, core)))
            .map_err(|error| syntax_error(text, &error))?;
        config.ignored = ignored;
        config.validate()?;
        Ok(config)
    }

    pub fn ignored_tables(&self) -> &[String] {
        &self.ignored
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
        config.ignored.clone_from(&self.ignored);
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
        Ok(())
    }
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

fn is_table(value: &DeValue<'_>) -> bool {
    match value {
        DeValue::Table(_) => true,
        DeValue::Array(items) => {
            !items.is_empty()
                && items
                    .iter()
                    .all(|item| matches!(item.get_ref(), DeValue::Table(_)))
        }
        _ => false,
    }
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

    #[test]
    fn tables_are_left_alone_and_named_as_not_read() {
        let config = Config::parse(
            "tab-width = 4\n\n[weather]\nlocation = \"Oslo\"\n\n[[irc-server]]\nname = \"libera\"\n",
        )
        .unwrap();

        assert_eq!(config.tab_width, 4);
        assert_eq!(config.ignored_tables(), ["irc-server", "weather"]);
        assert_eq!(
            config
                .with("tab-width", Some(toml::Value::Integer(2)))
                .unwrap()
                .ignored_tables(),
            ["irc-server", "weather"]
        );
    }
    #[test]
    fn unknown_plain_keys_are_still_mistakes() {
        assert!(Config::parse("dice = 8\n").is_err());
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
