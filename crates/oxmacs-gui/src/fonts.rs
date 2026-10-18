use iced::Font;

pub const JETBRAINS_MONO_REGULAR: &[u8] = include_bytes!("../fonts/JetBrainsMono-Regular.ttf");
pub const EDITOR_FAMILY: &str = "JetBrains Mono";
pub const EDITOR: Font = Font::with_name(EDITOR_FAMILY);

#[cfg(test)]
mod tests {
    use super::*;
    use ttf_parser::{Face, name_id};

    #[test]
    fn bundled_font_family_matches_the_requested_editor_font() {
        let face = Face::parse(JETBRAINS_MONO_REGULAR, 0).unwrap();
        let family = face
            .names()
            .into_iter()
            .filter(|name| name.name_id == name_id::FAMILY)
            .find_map(|name| name.to_string());

        assert_eq!(family.as_deref(), Some(EDITOR_FAMILY));
    }

    #[test]
    fn bundled_font_is_monospaced() {
        let face = Face::parse(JETBRAINS_MONO_REGULAR, 0).unwrap();

        assert!(face.is_monospaced());
    }
}
