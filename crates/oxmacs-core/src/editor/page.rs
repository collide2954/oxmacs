use crate::display::LineStyle;
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

    pub(super) fn heading(&mut self, title: &str) {
        self.line(&[(title, Some(Highlight::Heading))]);
    }
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
}
