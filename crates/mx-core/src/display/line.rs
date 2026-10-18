use std::fmt::Write;
use std::iter;
use std::ops::Range;

use unicode_width::UnicodeWidthChar;

enum Glyph {
    Text(char),
    Spaces(usize),
    Caret(char),
    Octal(u32),
}

impl Glyph {
    fn of(c: char, column: usize, tab_width: usize) -> Self {
        match c {
            '\t' => Self::Spaces(tab_width.max(1) - column % tab_width.max(1)),
            '\u{7f}' => Self::Caret('?'),
            c if c < ' ' => Self::Caret(char::from(b'@' + c as u8)),
            c if c.is_control() => Self::Octal(c.into()),
            c => Self::Text(c),
        }
    }

    fn width(&self) -> usize {
        match self {
            Self::Text(c) => c.width().unwrap_or(0),
            Self::Spaces(count) => *count,
            Self::Caret(_) => 2,
            Self::Octal(_) => 4,
        }
    }

    fn render(&self, out: &mut String) {
        match self {
            Self::Text(c) => out.push(*c),
            Self::Spaces(count) => out.extend(iter::repeat_n(' ', *count)),
            Self::Caret(c) => {
                out.push('^');
                out.push(*c);
            }
            Self::Octal(code) => {
                let _ = write!(out, "\\{code:o}");
            }
        }
    }
}

pub fn render_line(text: &str, tab_width: usize) -> String {
    let mut rendered = String::with_capacity(text.len());
    let mut column = 0;
    for c in text.chars() {
        let glyph = Glyph::of(c, column, tab_width);
        glyph.render(&mut rendered);
        column += glyph.width();
    }
    rendered
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment<F> {
    pub column: usize,
    pub text: String,
    pub face: Option<F>,
}

pub fn segments<F: Copy + PartialEq>(
    text: &str,
    spans: &[(Range<usize>, F)],
    tab_width: usize,
) -> Vec<Segment<F>> {
    let mut segments: Vec<Segment<F>> = Vec::new();
    let mut column = 0;
    for (index, c) in text.chars().enumerate() {
        let face = spans
            .iter()
            .find(|(range, _)| range.contains(&index))
            .map(|(_, face)| *face);
        let glyph = Glyph::of(c, column, tab_width);
        match segments.last_mut() {
            Some(segment) if segment.face == face => glyph.render(&mut segment.text),
            _ => {
                let mut segment = Segment {
                    column,
                    text: String::new(),
                    face,
                };
                glyph.render(&mut segment.text);
                segments.push(segment);
            }
        }
        column += glyph.width();
    }
    segments
}

pub fn display_column(text: &str, chars: usize, tab_width: usize) -> usize {
    text.chars().take(chars).fold(0, |column, c| {
        column + Glyph::of(c, column, tab_width).width()
    })
}

pub fn char_at_column(text: &str, column: usize, tab_width: usize) -> usize {
    let mut start = 0;
    for (index, c) in text.chars().enumerate() {
        let end = start + Glyph::of(c, start, tab_width).width();
        if column < end {
            return if column - start <= end - column {
                index
            } else {
                index + 1
            };
        }
        start = end;
    }
    text.chars().count()
}

pub fn highlight_columns(
    text: &str,
    line: Range<usize>,
    selection: &Range<usize>,
    tab_width: usize,
) -> Option<(usize, Option<usize>)> {
    let start = selection.start.max(line.start);
    let covers_newline = selection.end > line.end;
    if selection.is_empty() || selection.end <= line.start || start > line.end {
        return None;
    }
    if start == line.end && !covers_newline {
        return None;
    }
    let start_column = display_column(text, start - line.start, tab_width);
    let end_column =
        (!covers_newline).then(|| display_column(text, selection.end - line.start, tab_width));
    Some((start_column, end_column))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c1_control_characters_render_as_octal_escapes() {
        assert_eq!(render_line("\u{80}", 8), "\\200");
        assert_eq!(display_column("\u{9f}x", 1, 8), 4);
    }

    #[test]
    fn plain_text_renders_unchanged() {
        assert_eq!(render_line("hello", 8), "hello");
    }

    #[test]
    fn tabs_expand_to_the_next_tab_stop() {
        assert_eq!(render_line("\tx", 4), "    x");
        assert_eq!(render_line("ab\tx", 4), "ab  x");
        assert_eq!(render_line("abcd\tx", 4), "abcd    x");
    }

    #[test]
    fn control_characters_render_in_caret_notation() {
        assert_eq!(render_line("a\rb", 8), "a^Mb");
        assert_eq!(render_line("\u{c}", 8), "^L");
        assert_eq!(render_line("\u{7f}", 8), "^?");
    }

    #[test]
    fn display_column_counts_cells_before_a_char_index() {
        assert_eq!(display_column("hello", 3, 8), 3);
        assert_eq!(display_column("a\tb", 2, 8), 8);
        assert_eq!(display_column("a\rb", 2, 8), 3);
    }

    #[test]
    fn wide_characters_take_two_cells() {
        assert_eq!(display_column("日本語", 2, 8), 4);
    }

    #[test]
    fn columns_map_back_to_chars() {
        assert_eq!(char_at_column("hello", 3, 8), 3);
        assert_eq!(char_at_column("hi", 10, 8), 2);
        assert_eq!(char_at_column("", 5, 8), 0);
    }

    #[test]
    fn columns_inside_wide_glyphs_snap_to_the_nearest_boundary() {
        assert_eq!(char_at_column("\tx", 3, 8), 0);
        assert_eq!(char_at_column("\tx", 5, 8), 1);
        assert_eq!(char_at_column("\tx", 9, 8), 2);
        assert_eq!(char_at_column("日本", 1, 8), 0);
        assert_eq!(char_at_column("日本", 2, 8), 1);
        assert_eq!(char_at_column("日本", 3, 8), 1);
    }

    fn segment(column: usize, text: &str, face: Option<char>) -> Segment<char> {
        Segment {
            column,
            text: text.to_owned(),
            face,
        }
    }

    #[test]
    fn unstyled_text_is_one_segment() {
        assert_eq!(segments::<char>("abc", &[], 8), [segment(0, "abc", None)]);
        assert_eq!(segments::<char>("", &[], 8), []);
    }

    #[test]
    fn spans_split_the_line_into_segments() {
        assert_eq!(
            segments("abcdef", &[(2..4, 'x')], 8),
            [
                segment(0, "ab", None),
                segment(2, "cd", Some('x')),
                segment(4, "ef", None)
            ]
        );
    }

    #[test]
    fn segments_start_at_display_columns() {
        assert_eq!(
            segments("\tab", &[(1..2, 'x')], 4),
            [
                segment(0, "    ", None),
                segment(4, "a", Some('x')),
                segment(5, "b", None)
            ]
        );
        assert_eq!(
            segments("日x", &[(1..2, 'x')], 8),
            [segment(0, "日", None), segment(2, "x", Some('x'))]
        );
    }

    #[test]
    fn highlight_within_a_line_spans_its_columns() {
        assert_eq!(
            highlight_columns("hello", 0..5, &(1..3), 8),
            Some((1, Some(3)))
        );
    }

    #[test]
    fn highlight_covering_the_newline_extends_to_the_edge() {
        assert_eq!(highlight_columns("ab", 0..2, &(1..5), 8), Some((1, None)));
        assert_eq!(highlight_columns("ab", 0..2, &(2..3), 8), Some((2, None)));
    }

    #[test]
    fn highlight_starting_on_an_earlier_line_begins_at_column_zero() {
        assert_eq!(
            highlight_columns("cd", 3..5, &(0..4), 8),
            Some((0, Some(1)))
        );
    }

    #[test]
    fn highlight_uses_display_columns() {
        assert_eq!(
            highlight_columns("\tx", 0..2, &(1..2), 8),
            Some((8, Some(9)))
        );
    }

    #[test]
    fn lines_outside_the_selection_have_no_highlight() {
        assert_eq!(highlight_columns("cd", 3..5, &(0..2), 8), None);
        assert_eq!(highlight_columns("ab", 0..2, &(4..6), 8), None);
        assert_eq!(highlight_columns("ab", 0..2, &(1..1), 8), None);
    }
}
