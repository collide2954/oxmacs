use iced::Font;

pub const JETBRAINS_MONO_REGULAR: &[u8] = include_bytes!("../fonts/JetBrainsMono-Regular.ttf");
pub const EDITOR_FAMILY: &str = "JetBrains Mono";
pub const EDITOR: Font = Font::with_name(EDITOR_FAMILY);

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
}
