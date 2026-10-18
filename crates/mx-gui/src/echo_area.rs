use iced::widget::text::Span;
use iced::widget::{container, rich_text, span, text};
use iced::{Element, Font, Length};
use mx_core::editor::Editor;

use crate::style;

pub fn view<'a, Message: 'a>(editor: &'a Editor, font: Font, size: f32) -> Element<'a, Message> {
    let content: Element<'a, Message> = match editor.minibuffer() {
        Some(minibuffer) => {
            let faces = editor.theme();
            let input = minibuffer.input().contents();
            let (before, at, after) = split_at_cursor(&input, minibuffer.input().point());
            let note = minibuffer
                .note()
                .map(|note| format!(" {note}"))
                .unwrap_or_default();
            let spans: [Span<'a, (), Font>; 5] = [
                span(minibuffer.prompt()).color(style::color(faces.minibuffer_prompt)),
                span(before.to_owned()),
                span(at.to_owned())
                    .color(style::color(faces.default.background))
                    .background(style::color(faces.cursor)),
                span(after.to_owned()),
                span(note),
            ];
            rich_text(spans).font(font).size(size).into()
        }
        None => text(editor.message().unwrap_or(" ").to_owned())
            .font(font)
            .size(size)
            .into(),
    };
    container(content)
        .width(Length::Fill)
        .padding([1, 4])
        .into()
}

pub fn split_at_cursor(text: &str, point: usize) -> (&str, &str, &str) {
    let start = text
        .char_indices()
        .nth(point)
        .map_or(text.len(), |(index, _)| index);
    let (before, rest) = text.split_at(start);
    let width = rest.chars().next().map_or(0, char::len_utf8);
    let (at, after) = rest.split_at(width);
    (before, if at.is_empty() { " " } else { at }, after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_splits_text_around_the_char_at_point() {
        assert_eq!(split_at_cursor("abc", 1), ("a", "b", "c"));
    }

    #[test]
    fn cursor_at_end_shows_a_space() {
        assert_eq!(split_at_cursor("abc", 3), ("abc", " ", ""));
    }

    #[test]
    fn cursor_position_counts_chars_not_bytes() {
        assert_eq!(split_at_cursor("héllo", 2), ("hé", "l", "lo"));
    }
}
