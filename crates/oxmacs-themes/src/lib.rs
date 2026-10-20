mod list;
mod palette;

use std::sync::LazyLock;

use oxmacs_core::applet::Applet;
use oxmacs_core::editor::Editor;
use oxmacs_core::theme::Theme;
use oxmacs_gui::applet::GuiApplet;

pub use palette::Palette;

static THEMES: LazyLock<Vec<Theme>> =
    LazyLock::new(|| list::PALETTES.iter().map(Palette::theme).collect());

#[derive(Debug)]
pub struct Themes;

impl Applet for Themes {
    fn name(&self) -> &'static str {
        "extra-themes"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn opt_in(&self) -> bool {
        true
    }

    fn themes(&self) -> &'static [Theme] {
        &THEMES
    }

    fn install(&self, _editor: &mut Editor) {}
}

impl GuiApplet for Themes {}

#[cfg(test)]
mod tests {
    use oxmacs_core::config::Config;
    use oxmacs_core::theme::{self, Appearance, Color};

    use super::*;

    #[test]
    fn every_theme_is_readable() {
        for theme in THEMES.iter() {
            assert_eq!(
                theme::problems(theme),
                Vec::<String>::new(),
                "{}",
                theme.name
            );
        }
    }

    #[test]
    fn names_are_new_unique_and_kebab_case() {
        for (index, theme) in THEMES.iter().enumerate() {
            assert!(
                theme
                    .name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{}",
                theme.name
            );
            assert!(!theme.name.starts_with("adwaita"), "{}", theme.name);
            assert!(
                THEMES[..index].iter().all(|other| other.name != theme.name),
                "{}",
                theme.name
            );
        }
    }

    #[test]
    fn backgrounds_keep_their_original_colors() {
        let background = |name: &str| {
            THEMES
                .iter()
                .find(|theme| theme.name == name)
                .map(|theme| theme.default.background)
        };

        assert_eq!(background("gruvbox-dark"), Some(Color::hex(0x28_28_28)));
        assert_eq!(background("catppuccin-latte"), Some(Color::hex(0xEF_F1_F5)));
        assert_eq!(background("modus-vivendi"), Some(Color::hex(0x00_00_00)));
    }

    #[test]
    fn installing_offers_every_theme_and_lets_config_name_them() {
        let mut editor = Editor::new();

        editor.install(&Themes);

        assert_eq!(editor.themes().len(), 2 + THEMES.len());
        assert!(Config::parse("theme = \"tokyonight-storm\"").is_ok());
        assert_eq!(
            editor.installed().last().map(|applet| applet.name),
            Some("extra-themes")
        );
    }

    #[test]
    fn toggling_stays_within_a_family_when_it_has_both_appearances() {
        let mut editor = Editor::new();
        editor.install(&Themes);
        let pairs = [
            ("gruvbox-dark", "gruvbox-light"),
            ("catppuccin-latte", "catppuccin-mocha"),
            ("tokyonight-day", "tokyonight"),
            ("rose-pine-dawn", "rose-pine"),
            ("dark-plus", "light-plus"),
            ("modus-operandi", "modus-vivendi"),
            ("material-lighter", "material-ocean"),
        ];

        for (from, to) in pairs {
            let from = theme::find(from).unwrap();
            assert_eq!(theme::counterpart(from).name, to);
        }
        let dark = THEMES
            .iter()
            .filter(|theme| theme.appearance == Appearance::Dark)
            .count();
        assert!(dark > THEMES.len() / 2);
    }
}
