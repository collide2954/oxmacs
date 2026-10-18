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
#[must_use]
pub struct Face {
    pub foreground: Color,
    pub background: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
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
    pub heading: Color,
    pub link: Color,
    pub symlink: Color,
    pub marked: Color,
    pub flagged: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Highlight {
    Heading,
    Link,
    Symlink,
    Marked,
    Flagged,
}

impl Face {
    pub fn mix(self, other: Self, amount: f32) -> Self {
        Self {
            foreground: self.foreground.mix(other.foreground, amount),
            background: self.background.mix(other.background, amount),
        }
    }
}

impl Theme {
    pub fn mix(&self, other: &Self, amount: f32) -> Self {
        Self {
            name: other.name,
            appearance: other.appearance,
            default: self.default.mix(other.default, amount),
            cursor: self.cursor.mix(other.cursor, amount),
            region: self.region.mix(other.region, amount),
            mode_line: self.mode_line.mix(other.mode_line, amount),
            mode_line_inactive: self
                .mode_line_inactive
                .mix(other.mode_line_inactive, amount),
            mode_line_border: self.mode_line_border.mix(other.mode_line_border, amount),
            minibuffer_prompt: self.minibuffer_prompt.mix(other.minibuffer_prompt, amount),
            completion_current: self
                .completion_current
                .mix(other.completion_current, amount),
            shadow: self.shadow.mix(other.shadow, amount),
            heading: self.heading.mix(other.heading, amount),
            link: self.link.mix(other.link, amount),
            symlink: self.symlink.mix(other.symlink, amount),
            marked: self.marked.mix(other.marked, amount),
            flagged: self.flagged.mix(other.flagged, amount),
        }
    }

    pub fn highlight(&self, highlight: Highlight) -> Color {
        match highlight {
            Highlight::Heading => self.heading,
            Highlight::Link => self.link,
            Highlight::Symlink => self.symlink,
            Highlight::Marked => self.marked,
            Highlight::Flagged => self.flagged,
        }
    }
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
    fn mixing_themes_blends_every_face_and_takes_the_target_identity() {
        let halfway = ADWAITA.mix(&ADWAITA_DARK, 0.5);

        assert_eq!(halfway.name, "adwaita-dark");
        assert_eq!(halfway.appearance, Appearance::Dark);
        assert_eq!(
            halfway.default.background,
            ADWAITA
                .default
                .background
                .mix(ADWAITA_DARK.default.background, 0.5)
        );
        assert_eq!(
            halfway.completion_current.foreground,
            ADWAITA
                .completion_current
                .foreground
                .mix(ADWAITA_DARK.completion_current.foreground, 0.5)
        );
        assert_eq!(ADWAITA.mix(&ADWAITA_DARK, 1.0), ADWAITA_DARK);
    }

    #[test]
    fn highlights_are_readable() {
        for theme in builtin() {
            for highlight in [Highlight::Heading, Highlight::Link, Highlight::Symlink] {
                let ratio = theme
                    .highlight(highlight)
                    .contrast_ratio(theme.default.background);
                assert!(ratio >= 4.5, "{} {highlight:?}: {ratio:.2}", theme.name);
            }
            for highlight in [Highlight::Marked, Highlight::Flagged] {
                let ratio = theme
                    .default
                    .foreground
                    .contrast_ratio(theme.highlight(highlight));
                assert!(ratio >= 4.5, "{} {highlight:?}: {ratio:.2}", theme.name);
                assert_ne!(theme.highlight(highlight), theme.default.background);
            }
        }
    }

    #[test]
    fn mixing_blends_the_highlights() {
        let halfway = ADWAITA.mix(&ADWAITA_DARK, 0.5);

        assert_eq!(
            halfway.highlight(Highlight::Marked),
            ADWAITA
                .highlight(Highlight::Marked)
                .mix(ADWAITA_DARK.highlight(Highlight::Marked), 0.5)
        );
    }

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
