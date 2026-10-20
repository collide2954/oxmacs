use unicode_width::UnicodeWidthStr;

use crate::display::LineStyle;
use crate::text::Buffer;
use crate::theme::Highlight;

#[derive(Debug, Default)]
pub struct Page {
    pub(super) text: String,
    pub(super) styles: Vec<LineStyle>,
}

impl Page {
    pub fn line(&mut self, parts: &[(&str, Option<Highlight>)]) {
        let mut style = LineStyle::default();
        let mut column = 0;
        for (part, highlight) in parts {
            let width = part.chars().count();
            if let Some(highlight) = highlight {
                style.spans.push((column..column + width, *highlight));
            }
            self.text.push_str(part);
            column += width;
        }
        self.text.push('\n');
        self.styles.push(style);
    }

    pub fn paragraph(&mut self, text: &str, width: usize) {
        for line in fill(text, width) {
            self.line(&[(&line, None)]);
        }
    }

    pub fn heading(&mut self, title: &str) {
        self.line(&[(title, Some(Highlight::Heading))]);
    }

    pub fn into_parts(self) -> (String, Vec<LineStyle>) {
        (self.text, self.styles)
    }

    pub(super) fn fill(&self, buffer: &mut Buffer) {
        let point = buffer.point();
        let line = buffer.line_of(point);
        let column = point - buffer.line_start(line);
        buffer.set_text(&self.text);
        buffer.mark_saved();
        let line = line.min(buffer.line_of(buffer.len_chars()));
        buffer.set_point((buffer.line_start(line) + column).min(buffer.line_end(line)));
    }
}

pub(super) fn fill(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let needed = line.width() + 1 + word.width();
        if !line.is_empty() && needed > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

pub(super) fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut clipped: String = text.chars().take(width.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_collect_text_and_highlighted_spans() {
        let mut page = Page::default();

        page.heading("Title");
        page.line(&[("key ", None), ("value", Some(Highlight::Link))]);

        assert_eq!(page.text, "Title\nkey value\n");
        assert_eq!(page.styles[0].spans, [(0..5, Highlight::Heading)]);
        assert_eq!(page.styles[1].spans, [(4..9, Highlight::Link)]);
    }

    #[test]
    fn paragraphs_break_between_words_to_fit_the_width() {
        let mut page = Page::default();

        page.paragraph("one two three fourteen sixteen-letters x", 9);

        assert_eq!(page.text, "one two\nthree\nfourteen\nsixteen-letters\nx\n");
        assert_eq!(page.styles.len(), 5);
    }

    #[test]
    fn clipping_keeps_short_text_and_ends_long_text_with_an_ellipsis() {
        assert_eq!(clip("short", 8), "short");
        assert_eq!(clip("exactly8", 8), "exactly8");
        assert_eq!(clip("much too long", 8), "much to…");
    }

    #[test]
    fn wide_characters_fill_by_their_display_width() {
        assert_eq!(fill("日本 語", 5), ["日本", "語"]);
    }

    #[test]
    fn filling_a_buffer_keeps_point_on_its_line_and_column() {
        let mut page = Page::default();
        page.line(&[("first line", None)]);
        page.line(&[("second", None)]);
        let mut buffer = Buffer::with_text("t", "old\nsecond line here\n");
        buffer.set_point(4 + 8);

        page.fill(&mut buffer);

        assert_eq!(buffer.contents(), "first line\nsecond\n");
        assert_eq!(buffer.point(), 11 + 6);
        assert!(!buffer.is_modified());
    }
}
