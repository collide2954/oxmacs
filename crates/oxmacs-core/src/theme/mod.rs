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
    pub search_match: Color,
    pub search_current: Color,
    pub paren_match: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub syntax: SyntaxColors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
pub struct SyntaxColors {
    pub keyword: Color,
    pub string: Color,
    pub comment: Color,
    pub doc_comment: Color,
    pub function: Color,
    pub type_name: Color,
    pub constant: Color,
    pub macro_name: Color,
    pub attribute: Color,
    pub lifetime: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Highlight {
    Region,
    Heading,
    Link,
    Symlink,
    Marked,
    Flagged,
    SearchMatch,
    SearchCurrent,
    ParenMatch,
    Keyword,
    String,
    Comment,
    DocComment,
    Function,
    Type,
    Constant,
    Macro,
    Attribute,
    Lifetime,
    Success,
    Warning,
    Error,
    Added,
    Removed,
    Nick(u8),
}

pub const NICK_COLORS: u8 = 8;

const FACE_NAMES: &[(&str, Highlight)] = &[
    ("heading", Highlight::Heading),
    ("link", Highlight::Link),
    ("symlink", Highlight::Symlink),
    ("comment", Highlight::Comment),
    ("doc-comment", Highlight::DocComment),
    ("keyword", Highlight::Keyword),
    ("string", Highlight::String),
    ("function", Highlight::Function),
    ("type", Highlight::Type),
    ("constant", Highlight::Constant),
    ("attribute", Highlight::Attribute),
    ("macro", Highlight::Macro),
    ("lifetime", Highlight::Lifetime),
    ("success", Highlight::Success),
    ("warning", Highlight::Warning),
    ("error", Highlight::Error),
    ("added", Highlight::Added),
    ("removed", Highlight::Removed),
    ("marked", Highlight::Marked),
    ("flagged", Highlight::Flagged),
    ("region", Highlight::Region),
    ("search-match", Highlight::SearchMatch),
    ("search-current", Highlight::SearchCurrent),
    ("paren-match", Highlight::ParenMatch),
];

const CAPTURE_ALIASES: &[(&str, Highlight)] = &[
    ("comment.documentation", Highlight::DocComment),
    ("function.macro", Highlight::Macro),
];

pub fn face_named(name: &str) -> Option<Highlight> {
    if let Some(number) = name.strip_prefix("nick-") {
        return number
            .parse::<u8>()
            .ok()
            .filter(|number| *number < NICK_COLORS)
            .map(Highlight::Nick);
    }
    FACE_NAMES
        .iter()
        .chain(CAPTURE_ALIASES)
        .find(|(known, _)| *known == name)
        .map(|(_, highlight)| *highlight)
}

pub fn face_name(highlight: Highlight) -> Option<&'static str> {
    FACE_NAMES
        .iter()
        .find(|(_, known)| *known == highlight)
        .map(|(name, _)| *name)
}

pub const DIFF_TINT: f32 = 0.18;

const DIFF_STEPS: u8 = 6;

impl Face {
    pub fn mix(self, other: Self, amount: f32) -> Self {
        Self {
            foreground: self.foreground.mix(other.foreground, amount),
            background: self.background.mix(other.background, amount),
        }
    }
}

impl SyntaxColors {
    pub fn mix(self, other: Self, amount: f32) -> Self {
        Self {
            keyword: self.keyword.mix(other.keyword, amount),
            string: self.string.mix(other.string, amount),
            comment: self.comment.mix(other.comment, amount),
            doc_comment: self.doc_comment.mix(other.doc_comment, amount),
            function: self.function.mix(other.function, amount),
            type_name: self.type_name.mix(other.type_name, amount),
            constant: self.constant.mix(other.constant, amount),
            macro_name: self.macro_name.mix(other.macro_name, amount),
            attribute: self.attribute.mix(other.attribute, amount),
            lifetime: self.lifetime.mix(other.lifetime, amount),
        }
    }
}

const RAISED: f32 = 0.06;
const TINTED: f32 = 0.03;

impl Theme {
    pub fn surface(&self) -> Color {
        self.default.background.mix(self.default.foreground, RAISED)
    }

    pub fn tint(&self) -> Color {
        self.default.background.mix(self.default.foreground, TINTED)
    }

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
            search_match: self.search_match.mix(other.search_match, amount),
            search_current: self.search_current.mix(other.search_current, amount),
            paren_match: self.paren_match.mix(other.paren_match, amount),
            success: self.success.mix(other.success, amount),
            warning: self.warning.mix(other.warning, amount),
            error: self.error.mix(other.error, amount),
            syntax: self.syntax.mix(other.syntax, amount),
        }
    }

    pub fn highlight(&self, highlight: Highlight) -> Color {
        match highlight {
            Highlight::Region => self.region,
            Highlight::Heading => self.heading,
            Highlight::Link => self.link,
            Highlight::Symlink => self.symlink,
            Highlight::Marked => self.marked,
            Highlight::Flagged => self.flagged,
            Highlight::SearchMatch => self.search_match,
            Highlight::SearchCurrent => self.search_current,
            Highlight::ParenMatch => self.paren_match,
            Highlight::Keyword => self.syntax.keyword,
            Highlight::String => self.syntax.string,
            Highlight::Comment => self.syntax.comment,
            Highlight::DocComment => self.syntax.doc_comment,
            Highlight::Function => self.syntax.function,
            Highlight::Type => self.syntax.type_name,
            Highlight::Constant => self.syntax.constant,
            Highlight::Macro => self.syntax.macro_name,
            Highlight::Attribute => self.syntax.attribute,
            Highlight::Lifetime => self.syntax.lifetime,
            Highlight::Success => self.success,
            Highlight::Warning => self.warning,
            Highlight::Error => self.error,
            Highlight::Added => self.diff_tint(self.success),
            Highlight::Removed => self.diff_tint(self.error),
            Highlight::Nick(index) => self.nick(index),
        }
    }

    fn diff_tint(&self, color: Color) -> Color {
        (1..=DIFF_STEPS)
            .rev()
            .map(|step| {
                let amount = DIFF_TINT * f32::from(step) / f32::from(DIFF_STEPS);
                self.default.background.mix(color, amount)
            })
            .find(|tint| self.default.foreground.contrast_ratio(*tint) >= READABLE)
            .unwrap_or(self.default.background)
    }

    fn nick(&self, index: u8) -> Color {
        let palette = [
            self.syntax.keyword,
            self.syntax.string,
            self.syntax.function,
            self.syntax.type_name,
            self.syntax.constant,
            self.syntax.macro_name,
            self.syntax.attribute,
            self.syntax.lifetime,
        ];
        palette[usize::from(index) % palette.len()]
    }
}

pub fn builtin() -> [&'static Theme; 2] {
    [&ADWAITA, &ADWAITA_DARK]
}

pub fn all() -> Vec<&'static Theme> {
    builtin().to_vec()
}

pub fn find(name: &str) -> Option<&'static Theme> {
    builtin().into_iter().find(|theme| theme.name == name)
}

pub fn counterpart(theme: &Theme) -> &'static Theme {
    for_appearance(match theme.appearance {
        Appearance::Light => Appearance::Dark,
        Appearance::Dark => Appearance::Light,
    })
}

const READABLE: f64 = 4.5;

const TEXT: [Highlight; 16] = [
    Highlight::Heading,
    Highlight::Link,
    Highlight::Symlink,
    Highlight::Keyword,
    Highlight::String,
    Highlight::Comment,
    Highlight::DocComment,
    Highlight::Function,
    Highlight::Type,
    Highlight::Constant,
    Highlight::Macro,
    Highlight::Attribute,
    Highlight::Lifetime,
    Highlight::Success,
    Highlight::Warning,
    Highlight::Error,
];

const TINTS: [Highlight; 7] = [
    Highlight::Marked,
    Highlight::Added,
    Highlight::Removed,
    Highlight::Flagged,
    Highlight::SearchMatch,
    Highlight::SearchCurrent,
    Highlight::ParenMatch,
];

pub fn problems(theme: &Theme) -> Vec<String> {
    let mut found = Vec::new();
    let background = theme.default.background;
    let text = theme.default.foreground;
    for highlight in TEXT
        .into_iter()
        .chain((0..NICK_COLORS).map(Highlight::Nick))
    {
        let ratio = theme.highlight(highlight).contrast_ratio(background);
        if ratio < READABLE {
            found.push(format!("{highlight:?} on the background: {ratio:.2}"));
        }
    }
    for highlight in TINTS {
        let tint = theme.highlight(highlight);
        let ratio = text.contrast_ratio(tint);
        if ratio < READABLE {
            found.push(format!("text on {highlight:?}: {ratio:.2}"));
        }
        if tint == background {
            found.push(format!("{highlight:?} is the background color"));
        }
        if tint == theme.region {
            found.push(format!("{highlight:?} is the region color"));
        }
    }
    if theme.search_match == theme.search_current {
        found.push("SearchCurrent is the SearchMatch color".to_owned());
    }
    for (face, label) in [
        (
            Face {
                foreground: text,
                background: theme.region,
            },
            "text on the region",
        ),
        (theme.default, "default text"),
        (theme.mode_line, "mode-line text"),
    ] {
        let ratio = face.foreground.contrast_ratio(face.background);
        if ratio < READABLE {
            found.push(format!("{label}: {ratio:.2}"));
        }
    }
    let darker = background.relative_luminance() < text.relative_luminance();
    let mismatch = match (theme.appearance, darker) {
        (Appearance::Dark, false) => Some("Dark but the background is lighter"),
        (Appearance::Light, true) => Some("Light but the background is darker"),
        _ => None,
    };
    if let Some(mismatch) = mismatch {
        found.push(format!("appearance is {mismatch} than the text"));
    }
    found
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
    fn the_builtin_themes_have_no_readability_problems() {
        for theme in builtin() {
            assert_eq!(problems(theme), Vec::<String>::new(), "{}", theme.name);
        }
    }

    #[test]
    fn problems_name_each_unreadable_or_clashing_face() {
        let murky = Theme {
            name: "murky",
            syntax: SyntaxColors {
                comment: ADWAITA.default.background,
                ..ADWAITA.syntax
            },
            marked: ADWAITA.region,
            search_current: ADWAITA.search_match,
            mode_line: Face {
                foreground: Color::hex(0xFF_FF_FF),
                background: Color::hex(0xFF_FF_FF),
            },
            appearance: Appearance::Dark,
            ..ADWAITA
        };

        assert_eq!(
            problems(&murky),
            [
                "Comment on the background: 1.00",
                "Marked is the region color",
                "SearchCurrent is the SearchMatch color",
                "mode-line text: 1.00",
                "appearance is Dark but the background is lighter than the text",
            ]
        );
    }
    #[test]
    fn nick_colors_cycle_through_distinct_colors() {
        let colors: Vec<Color> = (0..NICK_COLORS)
            .map(|index| ADWAITA.highlight(Highlight::Nick(index)))
            .collect();

        for (index, color) in colors.iter().enumerate() {
            assert!(!colors[..index].contains(color), "{index}");
        }
        assert_eq!(ADWAITA.highlight(Highlight::Nick(NICK_COLORS)), colors[0]);
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
    fn diff_lines_get_soft_tints_of_success_and_error() {
        for theme in builtin() {
            let background = theme.default.background;
            let added = theme.highlight(Highlight::Added);
            let removed = theme.highlight(Highlight::Removed);

            assert_eq!(added, background.mix(theme.success, DIFF_TINT));
            assert_eq!(removed, background.mix(theme.error, DIFF_TINT));
            assert!(theme.default.foreground.contrast_ratio(added) >= READABLE);
            assert!(theme.default.foreground.contrast_ratio(removed) >= READABLE);
        }
    }

    #[test]
    fn diff_tints_soften_where_text_would_be_hard_to_read() {
        let dim = Theme {
            default: Face {
                foreground: Color::hex(0x93_A1_A1),
                background: Color::hex(0x00_2B_36),
            },
            ..ADWAITA_DARK
        };

        let added = dim.highlight(Highlight::Added);

        assert_ne!(added, dim.default.background.mix(dim.success, DIFF_TINT));
        assert_ne!(added, dim.default.background);
        assert!(dim.default.foreground.contrast_ratio(added) >= READABLE);
    }

    #[test]
    fn chrome_colors_sit_between_the_background_and_the_text() {
        for theme in builtin() {
            let background = theme.default.background;
            let raised = theme.surface().contrast_ratio(background);
            let tinted = theme.tint().contrast_ratio(background);
            let dimmed = theme.default.foreground.dimmed(background);

            assert!(tinted > 1.0 && raised > tinted, "{}", theme.name);
            assert!(dimmed.contrast_ratio(background) >= 4.5, "{}", theme.name);
            assert!(
                dimmed.contrast_ratio(background)
                    < theme.default.foreground.contrast_ratio(background)
            );
        }
    }

    #[test]
    fn faces_have_one_name_each_with_tree_sitter_captures_as_aliases() {
        for (name, highlight) in FACE_NAMES {
            assert_eq!(face_named(name), Some(*highlight));
            assert_eq!(face_name(*highlight), Some(*name));
        }
        assert_eq!(
            face_named("comment.documentation"),
            Some(Highlight::DocComment)
        );
        assert_eq!(face_named("function.macro"), Some(Highlight::Macro));
        assert_eq!(face_name(Highlight::DocComment), Some("doc-comment"));
        assert_eq!(face_named("nick-3"), Some(Highlight::Nick(3)));
        assert_eq!(face_named("nick-8"), None);
        assert_eq!(face_named("nope"), None);
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
    fn each_theme_has_a_counterpart_of_the_other_appearance() {
        assert_eq!(counterpart(&ADWAITA).name, "adwaita-dark");
        assert_eq!(counterpart(&ADWAITA_DARK).name, "adwaita");
    }

    #[test]
    fn os_appearance_selects_the_matching_adwaita_variant() {
        assert_eq!(for_appearance(Appearance::Light).name, "adwaita");
        assert_eq!(for_appearance(Appearance::Dark).name, "adwaita-dark");
    }
}
