use crate::display::LineStyle;
use crate::text::Buffer;
use crate::theme::Highlight;

#[derive(Debug, Default)]
pub(super) struct Page {
    pub(super) text: String,
    pub(super) styles: Vec<LineStyle>,
}

impl Page {
    pub(super) fn line(&mut self, parts: &[(&str, Option<Highlight>)]) {
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

    pub(super) fn paragraph(&mut self, text: &str, width: usize) {
        for line in fill(text, width) {
            self.line(&[(&line, None)]);
        }
    }

    pub(super) fn heading(&mut self, title: &str) {
        self.line(&[(title, Some(Highlight::Heading))]);
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
        let needed = line.chars().count() + 1 + word.chars().count();
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
