use std::sync::OnceLock;

use iced::Font;
use iced::advanced::graphics::text::font_system;
use iced::font::Family;

pub const JETBRAINS_MONO_REGULAR: &[u8] = include_bytes!("../fonts/JetBrainsMono-Regular.ttf");
pub const EDITOR_FAMILY: &str = "JetBrains Mono";
pub const EDITOR: Font = Font::with_name(EDITOR_FAMILY);
pub const UI: Font = Font {
    family: Family::SansSerif,
    ..Font::DEFAULT
};

const SERIFS: &[&str] = &[
    "Literata",
    "Charter",
    "Iowan Old Style",
    "Georgia",
    "Palatino Linotype",
    "Palatino",
    "Book Antiqua",
    "Source Serif 4",
    "Noto Serif",
    "DejaVu Serif",
    "Liberation Serif",
    "Cambria",
    "Times New Roman",
    "Times",
];

const SANS: &[&str] = &[
    "Inter",
    "Segoe UI",
    "Helvetica Neue",
    "Noto Sans",
    "Open Sans",
    "DejaVu Sans",
    "Liberation Sans",
    "Arial",
];

fn installed(wanted: &'static [&'static str]) -> Option<&'static str> {
    let mut fonts = font_system().write().ok()?;
    let known: Vec<String> = fonts
        .raw()
        .db()
        .faces()
        .flat_map(|face| face.families.iter().map(|(name, _)| name.to_lowercase()))
        .collect();
    wanted
        .iter()
        .find(|name| known.contains(&name.to_lowercase()))
        .copied()
}

pub fn serif() -> Font {
    static CHOSEN: OnceLock<Option<&'static str>> = OnceLock::new();
    CHOSEN.get_or_init(|| installed(SERIFS)).map_or(
        Font {
            family: Family::Serif,
            ..Font::DEFAULT
        },
        Font::with_name,
    )
}

pub fn sans() -> Font {
    static CHOSEN: OnceLock<Option<&'static str>> = OnceLock::new();
    CHOSEN.get_or_init(|| installed(SANS)).map_or(
        Font {
            family: Family::SansSerif,
            ..Font::DEFAULT
        },
        Font::with_name,
    )
}

#[cfg(test)]
mod tests {
    use skrifa::instance::{LocationRef, Size};
    use skrifa::string::StringId;
    use skrifa::{FontRef, MetadataProvider};

    use super::*;

    fn bundled() -> FontRef<'static> {
        FontRef::new(JETBRAINS_MONO_REGULAR).unwrap()
    }

    #[test]
    fn bundled_font_family_matches_the_requested_editor_font() {
        let family = bundled()
            .localized_strings(StringId::FAMILY_NAME)
            .english_or_first()
            .map(|name| name.to_string());

        assert_eq!(family.as_deref(), Some(EDITOR_FAMILY));
    }

    #[test]
    fn bundled_font_is_monospaced() {
        let metrics = bundled().metrics(Size::unscaled(), LocationRef::default());

        assert!(metrics.is_monospace);
    }

    #[test]
    fn book_faces_are_installed_families_or_generic_ones() {
        assert_eq!(installed(&["No Such Family"]), None);
        for (font, wanted, generic) in [
            (serif(), SERIFS, Family::Serif),
            (sans(), SANS, Family::SansSerif),
        ] {
            match font.family {
                Family::Name(name) => assert!(wanted.contains(&name)),
                family => assert_eq!(family, generic),
            }
        }
    }
}
