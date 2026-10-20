use std::ops::Range;

use iced::widget::text::Span;
use iced::widget::{container, rich_text, span, text};
use iced::{Element, Font, Length};
use oxmacs_core::editor::{Editor, Search};
use oxmacs_core::theme::{Highlight, Theme as Faces};

use crate::style;

pub fn view<'a, Message: 'a>(
    editor: &'a Editor,
    total: usize,
    faces: Faces,
    font: Font,
    size: f32,
    frame: Option<container::Style>,
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
            let (input, highlights) = if minibuffer.is_secret() {
                (masked(&input), Vec::new())
            } else {
                (input, editor.minibuffer_highlights())
            };
            let pieces = input_pieces(&input, minibuffer.input().point(), &highlights);
            let note = minibuffer
                .note()
                .map(|note| format!(" {note}"))
                .unwrap_or_default();
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
            let mut spans: Vec<Span<'a, (), Font>> = vec![
                span(count).color(style::color(faces.shadow)),
                span(minibuffer.prompt()).color(style::color(faces.minibuffer_prompt)),
            ];
            spans.extend(pieces.into_iter().map(|(text, highlight, cursor)| {
                if cursor {
                    span(text)
                        .color(style::color(faces.default.background))
                        .background(style::color(faces.cursor))
                } else if let Some(highlight) = highlight {
                    span(text).color(style::color(faces.highlight(highlight)))
                } else {
                    span(text)
                }
            }));
            spans.push(span(note));
            rich_text(spans).font(font).size(size).into()
        }
        (None, None) => text(editor.echo().unwrap_or(" ").to_owned())
            .font(font)
            .size(size)
            .into(),
    };
    let area = container(content).width(Length::Fill).padding([1, 4]);
    match frame {
        Some(frame) => area.style(move |_| frame).into(),
        None => area.into(),
    }
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

fn masked(secret: &str) -> String {
    "•".repeat(secret.chars().count())
}

type Piece = (String, Option<Highlight>, bool);

fn input_pieces(text: &str, point: usize, highlights: &[(Range<usize>, Highlight)]) -> Vec<Piece> {
    let face = |column: usize| {
        highlights
            .iter()
            .find(|(range, _)| range.contains(&column))
            .map(|(_, highlight)| *highlight)
    };
    let mut pieces: Vec<Piece> = Vec::new();
    for (column, c) in text.chars().enumerate() {
        let cursor = column == point;
        let highlight = face(column);
        match pieces.last_mut() {
            Some((piece, last, false)) if !cursor && *last == highlight => piece.push(c),
            _ => pieces.push((c.to_string(), highlight, cursor)),
        }
    }
    if point >= text.chars().count() {
        pieces.push((" ".to_owned(), None, true));
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str, cursor: bool) -> Piece {
        (text.to_owned(), None, cursor)
    }

    #[test]
    fn cursor_splits_text_around_the_char_at_point() {
        assert_eq!(
            input_pieces("abc", 1, &[]),
            [plain("a", false), plain("b", true), plain("c", false)]
        );
    }

    #[test]
    fn cursor_at_end_shows_a_space() {
        assert_eq!(
            input_pieces("abc", 3, &[]),
            [plain("abc", false), plain(" ", true)]
        );
    }

    #[test]
    fn highlighted_input_is_split_by_face() {
        let highlights = [(0..3, Highlight::Function), (6..8, Highlight::Type)];

        assert_eq!(
            input_pieces("[ ] x #t", 8, &highlights),
            [
                ("[ ]".to_owned(), Some(Highlight::Function), false),
                plain(" x ", false),
                ("#t".to_owned(), Some(Highlight::Type), false),
                plain(" ", true),
            ]
        );
    }

    #[test]
    fn failing_queries_split_where_matching_stopped() {
        assert_eq!(split_failing("héllo", Some(2)), ("hé", "llo"));
        assert_eq!(split_failing("foo", Some(3)), ("foo", ""));
        assert_eq!(split_failing("foo", None), ("foo", ""));
    }

    #[test]
    fn secrets_show_one_dot_per_character() {
        assert_eq!(masked("hünter2"), "•••••••");
        assert_eq!(masked(""), "");
    }

    #[test]
    fn cursor_position_counts_chars_not_bytes() {
        assert_eq!(
            input_pieces("héllo", 2, &[]),
            [plain("hé", false), plain("l", true), plain("lo", false)]
        );
    }
}
