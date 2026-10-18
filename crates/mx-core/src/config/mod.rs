use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::Deserialize;
use thiserror::Error;

use crate::theme;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub theme: Option<String>,
    pub font_family: Option<String>,
    pub font_size: f32,
    pub tab_width: usize,
    pub animations: bool,
    pub cursor_blink_interval: u64,
    pub on_disk_change: OnDiskChange,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnDiskChange {
    #[default]
    Revert,
    Ask,
    Ignore,
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
        Ok(())
    }
}

pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("mx").join("config.toml"))
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
    fn default_path_is_config_toml_in_an_mx_directory() {
        if let Some(path) = default_path() {
            assert!(path.ends_with(Path::new("mx").join("config.toml")));
        }
    }
}
