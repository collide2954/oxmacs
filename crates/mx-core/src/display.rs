use std::fmt::Write;
use std::iter;

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

pub fn display_column(text: &str, chars: usize, tab_width: usize) -> usize {
    text.chars().take(chars).fold(0, |column, c| {
        column + Glyph::of(c, column, tab_width).width()
    })
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
}
