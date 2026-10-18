mod builtin;
mod color;

pub use builtin::{ADWAITA, ADWAITA_DARK};
pub use color::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Appearance {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Face {
    pub foreground: Color,
    pub background: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Theme {
    pub name: &'static str,
    pub appearance: Appearance,
    pub default: Face,
    pub cursor: Color,
    pub region: Color,
    pub mode_line: Face,
    pub mode_line_inactive: Face,
    pub mode_line_border: Color,
    pub minibuffer_prompt: Color,
    pub completion_current: Face,
    pub shadow: Color,
}

pub fn builtin() -> [&'static Theme; 2] {
    [&ADWAITA, &ADWAITA_DARK]
}

pub fn find(name: &str) -> Option<&'static Theme> {
    builtin().into_iter().find(|theme| theme.name == name)
}

pub fn for_appearance(appearance: Appearance) -> &'static Theme {
    match appearance {
        Appearance::Light => &ADWAITA,
        Appearance::Dark => &ADWAITA_DARK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_themes_are_found_by_name() {
        assert_eq!(find("adwaita").map(|theme| theme.name), Some("adwaita"));
        assert_eq!(
            find("adwaita-dark").map(|theme| theme.name),
            Some("adwaita-dark")
        );
        assert!(find("no-such-theme").is_none());
    }

    #[test]
    fn os_appearance_selects_the_matching_adwaita_variant() {
        assert_eq!(for_appearance(Appearance::Light).name, "adwaita");
        assert_eq!(for_appearance(Appearance::Dark).name, "adwaita-dark");
    }

    #[test]
    fn appearance_matches_the_default_background() {
        for theme in builtin() {
            let background = theme.default.background.relative_luminance();
            let foreground = theme.default.foreground.relative_luminance();
            let expected = if background < foreground {
                Appearance::Dark
            } else {
                Appearance::Light
            };
            assert_eq!(theme.appearance, expected, "{}", theme.name);
        }
    }

    #[test]
    fn text_faces_meet_wcag_aa_contrast() {
        for theme in builtin() {
            for (face, label) in [(theme.default, "default"), (theme.mode_line, "mode-line")] {
                let ratio = face.foreground.contrast_ratio(face.background);
                assert!(ratio >= 4.5, "{} {label}: {ratio:.2}", theme.name);
            }
        }
    }
}
