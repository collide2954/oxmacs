pub mod schema;

use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
    pub delete_to_trash: bool,
    pub splash: bool,
    pub format_on_save: bool,
    pub clock: bool,
    pub clock_format: String,
    pub rust_lint: RustLint,
    pub auto_save: bool,
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
        let config: Self = toml::from_str(text).map_err(|error| syntax_error(text, &error))?;
        config.validate()?;
        Ok(config)
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
        let config: Self = toml::Value::Table(table)
            .try_into()
            .map_err(|error: toml::de::Error| ConfigError::Syntax(error.message().to_owned()))?;
        config.validate()?;
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

    #[test]
    fn every_setting_in_the_schema_is_a_config_key() {
        let full = Config {
            theme: Some("adwaita".to_owned()),
            font_family: Some("Iosevka".to_owned()),
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
    fn theme_choices_are_the_builtin_themes() {
        let names: Vec<&str> = theme::builtin().iter().map(|theme| theme.name).collect();

        assert_eq!(
            schema::find("theme").map(|setting| setting.kind),
            Some(schema::Kind::Choice(&["adwaita", "adwaita-dark"]))
        );
        assert_eq!(names, ["adwaita", "adwaita-dark"]);
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

    #[test]
    fn rust_lints_use_cargo_check_unless_configured() {
        let read = |text| Config::parse(text).map(|config| config.rust_lint);

        assert_eq!(Config::default().rust_lint, RustLint::Check);
        assert_eq!(read("rust-lint = \"clippy\"").unwrap(), RustLint::Clippy);
        assert_eq!(read("rust-lint = \"off\"").unwrap(), RustLint::Off);
        assert!(read("rust-lint = \"miri\"").is_err());
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
