use iced::widget::text::Span;
use iced::widget::{container, rich_text, span, text};
use iced::{Element, Font, Length};
use oxmacs_core::editor::{Editor, Search};
use oxmacs_core::theme::Theme as Faces;

use crate::style;

pub fn view<'a, Message: 'a>(
    editor: &'a Editor,
    faces: Faces,
    font: Font,
    size: f32,
) -> Element<'a, Message> {
    let content: Element<'a, Message> = match (editor.question(), editor.minibuffer()) {
        (None, None) if let Some(replace) = editor.replacing() => {
            let spans: [Span<'a, (), Font>; 2] = [
                span(replace.prompt()).color(style::color(faces.minibuffer_prompt)),
                span(" ")
                    .color(style::color(faces.default.background))
                    .background(style::color(faces.cursor)),
            ];
            rich_text(spans).font(font).size(size).into()
        }
        (None, None) if let Some(search) = editor.search() => {
            rich_text(search_spans(search, faces))
                .font(font)
                .size(size)
                .into()
        }
        (Some(question), _) => {
            let spans: [Span<'a, (), Font>; 4] = [
                span(question.prompt()).color(style::color(faces.minibuffer_prompt)),
                span(question.answer()),
                span(" ")
                    .color(style::color(faces.default.background))
                    .background(style::color(faces.cursor)),
                span(
                    editor
                        .message()
                        .map(|message| format!("  {message}"))
                        .unwrap_or_default(),
                ),
            ];
            rich_text(spans).font(font).size(size).into()
        }
        (None, Some(minibuffer)) => {
            let input = minibuffer.input().contents();
            let (before, at, after) = split_at_cursor(&input, minibuffer.input().point());
            let note = minibuffer
                .note()
                .map(|note| format!(" {note}"))
                .unwrap_or_default();
            let total = editor.completions().len();
            let position = if total == 0 {
                0
            } else {
                minibuffer.selected() + 1
            };
            let count = if minibuffer.completes() {
                format!("{position}/{total} ")
            } else {
                String::new()
            };
            let spans: [Span<'a, (), Font>; 6] = [
                span(count).color(style::color(faces.shadow)),
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
        (None, None) => text(editor.echo().unwrap_or(" ").to_owned())
            .font(font)
            .size(size)
            .into(),
    };
    container(content)
        .width(Length::Fill)
        .padding([1, 4])
        .into()
}

fn search_spans(search: &Search, faces: Faces) -> [Span<'static, (), Font>; 5] {
    let count = search
        .count()
        .map(|(current, total)| format!("{current}/{total} "))
        .unwrap_or_default();
    let (matched, failing) = split_failing(search.query(), search.failing_from());
    let note = search
        .note()
        .map(|note| format!(" {note}"))
        .unwrap_or_default();
    [
        span(count).color(style::color(faces.shadow)),
        span(search.prompt()).color(style::color(faces.minibuffer_prompt)),
        span(matched.to_owned()),
        span(failing.to_owned()).background(style::color(faces.flagged)),
        span(note),
    ]
}

pub fn split_failing(query: &str, failing_from: Option<usize>) -> (&str, &str) {
    let at = failing_from
        .and_then(|chars| query.char_indices().nth(chars))
        .map_or(query.len(), |(index, _)| index);
    query.split_at(at)
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
    fn failing_queries_split_where_matching_stopped() {
        assert_eq!(split_failing("héllo", Some(2)), ("hé", "llo"));
        assert_eq!(split_failing("foo", Some(3)), ("foo", ""));
        assert_eq!(split_failing("foo", None), ("foo", ""));
    }

    #[test]
    fn cursor_position_counts_chars_not_bytes() {
        assert_eq!(split_at_cursor("héllo", 2), ("hé", "l", "lo"));
    }
}
