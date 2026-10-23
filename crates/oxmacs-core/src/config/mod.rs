pub mod schema;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::{clock, theme};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is its own setting"
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    #[error("unknown setting `{0}`")]
    UnknownSetting(String),
    #[error("{key}: {message}")]
    Value { key: String, message: String },
    #[error("unknown theme `{0}`")]
    UnknownTheme(String),
    #[error("tab-width must be at least 1")]
    TabWidth,
    #[error("font-size must be a positive number of points")]
    FontSize,
    #[error("clock-format: {0}")]
    ClockFormat(String),
}

impl Config {
    pub fn from_settings(settings: &Value) -> Result<Self, ConfigError> {
        let Value::Object(entries) = settings else {
            return Err(ConfigError::Value {
                key: "settings".to_owned(),
                message: "expected a map of settings".to_owned(),
            });
        };
        entries
            .iter()
            .try_fold(Self::default(), |config, (key, value)| {
                config.with(key, value.clone())
            })
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        self.entries().remove(key)
    }

    fn entries(&self) -> Map<String, Value> {
        match serde_json::to_value(self) {
            Ok(Value::Object(entries)) => entries,
            _ => Map::new(),
        }
    }

    pub fn with(&self, key: &str, value: impl Into<Option<Value>>) -> Result<Self, ConfigError> {
        if schema::find(key).is_none() {
            return Err(ConfigError::UnknownSetting(key.to_owned()));
        }
        let mut entries = self.entries();
        match value.into() {
            None | Some(Value::Null) => entries.remove(key),
            Some(value) => entries.insert(key.to_owned(), value),
        };
        let config: Self =
            serde_json::from_value(Value::Object(entries)).map_err(|error| ConfigError::Value {
                key: key.to_owned(),
                message: error.to_string(),
            })?;
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn read(settings: &Value) -> Result<Config, ConfigError> {
        Config::from_settings(settings)
    }

    #[test]
    fn no_settings_use_the_defaults() {
        let config = read(&json!({})).unwrap();

        assert_eq!(config, Config::default());
        assert_eq!(config.theme, None);
        assert_eq!(config.font_family, None);
        assert!((config.font_size - 11.0).abs() < f32::EPSILON);
        assert_eq!(config.tab_width, 8);
        assert!(config.animations);
    }

    #[test]
    fn settings_are_read_by_their_kebab_case_names() {
        let config = read(&json!({
            "theme": "adwaita-dark",
            "font-family": "Iosevka",
            "font-size": 13.5,
            "tab-width": 4
        }))
        .unwrap();

        assert_eq!(config.theme.as_deref(), Some("adwaita-dark"));
        assert_eq!(config.font_family.as_deref(), Some("Iosevka"));
        assert!((config.font_size - 13.5).abs() < f32::EPSILON);
        assert_eq!(config.tab_width, 4);
    }

    #[test]
    fn whole_numbers_are_fine_for_number_settings() {
        assert!((read(&json!({"font-size": 12})).unwrap().font_size - 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn unknown_settings_are_named() {
        assert_eq!(
            read(&json!({"font-sise": 12})).unwrap_err().to_string(),
            "unknown setting `font-sise`"
        );
    }

    #[test]
    fn wrong_types_name_their_setting() {
        let message = read(&json!({"tab-width": "wide"})).unwrap_err().to_string();

        assert!(message.starts_with("tab-width: invalid type"), "{message}");
    }

    #[test]
    fn cursor_blink_interval_defaults_to_400ms_and_accepts_zero() {
        assert_eq!(Config::default().cursor_blink_interval, 400);
        assert_eq!(
            read(&json!({"cursor-blink-interval": 0}))
                .unwrap()
                .cursor_blink_interval,
            0
        );
    }

    #[test]
    fn on_disk_change_defaults_to_revert() {
        let mode = |name: &str| read(&json!({ "on-disk-change": name })).map(|c| c.on_disk_change);

        assert_eq!(Config::default().on_disk_change, OnDiskChange::Revert);
        assert_eq!(mode("ask").unwrap(), OnDiskChange::Ask);
        assert_eq!(mode("ignore").unwrap(), OnDiskChange::Ignore);
        assert!(mode("panic").is_err());
    }

    #[test]
    fn the_clock_shows_hours_and_minutes_unless_configured() {
        let config = Config::default();
        assert!(config.clock);
        assert_eq!(config.clock_format, "%H:%M");

        let config = read(&json!({"clock": false, "clock-format": "%a %H:%M"})).unwrap();
        assert!(!config.clock);
        assert_eq!(config.clock_format, "%a %H:%M");
    }

    #[test]
    fn invalid_clock_formats_are_rejected() {
        let error = read(&json!({"clock-format": "%H %"})).unwrap_err();

        assert!(error.to_string().starts_with("clock-format: "), "{error}");
    }

    #[test]
    fn opt_in_and_opt_out_settings_keep_their_defaults() {
        let config = Config::default();
        assert!(!config.format_on_save);
        assert!(!config.line_number_separator);
        assert!(config.auto_save);
        assert!(config.splash);
        assert_eq!(config.air_program, None);

        let config = read(&json!({
            "format-on-save": true,
            "line-number-separator": true,
            "auto-save": false,
            "splash": false,
            "animations": false,
            "air-program": "C:/tools/air.exe"
        }))
        .unwrap();
        assert!(config.format_on_save);
        assert!(config.line_number_separator);
        assert!(!config.auto_save);
        assert!(!config.splash);
        assert!(!config.animations);
        assert_eq!(config.air_program.as_deref(), Some("C:/tools/air.exe"));
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
        let mut keys: Vec<String> = full.entries().keys().cloned().collect();
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
        assert_eq!(
            read(&json!({"theme": "solarized"}))
                .unwrap_err()
                .to_string(),
            "unknown theme `solarized`"
        );
    }

    #[test]
    fn settings_are_read_and_changed_by_key() {
        let config = Config::default();
        assert_eq!(config.get("tab-width"), Some(json!(8)));
        assert_eq!(config.get("theme"), None);

        let config = config
            .with("tab-width", json!(4))
            .unwrap()
            .with("theme", json!("adwaita-dark"))
            .unwrap();
        assert_eq!(config.tab_width, 4);
        assert_eq!(config.theme.as_deref(), Some("adwaita-dark"));

        assert_eq!(config.with("theme", None).unwrap().theme, None);
        assert_eq!(config.with("tab-width", Value::Null).unwrap().tab_width, 8);
    }

    #[test]
    fn invalid_changes_are_refused() {
        let config = Config::default();

        assert_eq!(
            config.with("tab-width", json!(0)).unwrap_err(),
            ConfigError::TabWidth
        );
        assert!(config.with("rust-lint", json!("miri")).is_err());
        assert_eq!(
            config.with("font-size", json!(0)).unwrap_err().to_string(),
            "font-size must be a positive number of points"
        );
    }

    #[test]
    fn rust_lints_use_cargo_check_unless_configured() {
        let lint = |name: &str| read(&json!({ "rust-lint": name })).map(|c| c.rust_lint);

        assert_eq!(Config::default().rust_lint, RustLint::Check);
        assert_eq!(lint("clippy").unwrap(), RustLint::Clippy);
        assert_eq!(lint("off").unwrap(), RustLint::Off);
        assert!(lint("miri").is_err());
    }

    #[test]
    fn line_numbers_are_absolute_unless_configured() {
        let numbers = |name: &str| read(&json!({ "line-numbers": name })).map(|c| c.line_numbers);

        assert_eq!(Config::default().line_numbers, LineNumbers::Absolute);
        assert_eq!(numbers("relative").unwrap(), LineNumbers::Relative);
        assert_eq!(numbers("off").unwrap(), LineNumbers::Off);
    }

    #[test]
    fn the_window_opens_windowed_unless_configured() {
        let mode = |name: &str| read(&json!({ "window-mode": name })).map(|c| c.window_mode);

        assert_eq!(Config::default().window_mode, WindowMode::Windowed);
        assert_eq!(mode("maximized").unwrap(), WindowMode::Maximized);
        assert_eq!(mode("fullscreen").unwrap(), WindowMode::Fullscreen);
        assert!(mode("minimized").is_err());
    }

    #[test]
    fn the_mode_line_is_classic_unless_configured() {
        let style =
            |name: &str| read(&json!({ "mode-line-style": name })).map(|c| c.mode_line_style);

        assert_eq!(Config::default().mode_line_style, ModeLineStyle::Classic);
        for (name, wanted) in [
            ("modern", ModeLineStyle::Modern),
            ("minimal", ModeLineStyle::Minimal),
            ("powerline", ModeLineStyle::Powerline),
            ("boxed", ModeLineStyle::Boxed),
        ] {
            assert_eq!(style(name).unwrap(), wanted);
        }
        assert!(style("fancy").is_err());
    }
}
