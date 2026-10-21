use std::ops::Range;

use oxmacs_core::display::LineStyle;
use oxmacs_core::theme::Highlight;

#[derive(Debug, Default)]
pub(crate) struct Line {
    text: String,
    width: usize,
    style: LineStyle,
    pills: Vec<(Range<usize>, Highlight)>,
}

impl Line {
    pub(crate) fn push(&mut self, text: &str, face: Option<Highlight>) -> Range<usize> {
        let start = self.width;
        self.text.push_str(text);
        self.width += text.chars().count();
        let range = start..self.width;
        if let Some(face) = face
            && !range.is_empty()
        {
            self.style.spans.push((range.clone(), face));
        }
        range
    }

    pub(crate) fn pill(&mut self, text: &str, face: Highlight) {
        let range = self.push(text, Some(face));
        self.pills.push((range, face));
    }

    pub(crate) fn pad(&mut self, column: usize) {
        let spaces = column.saturating_sub(self.width);
        self.push(&" ".repeat(spaces), None);
    }

    pub(crate) fn width(&self) -> usize {
        self.width
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Lines<Row> {
    pub(crate) text: String,
    pub(crate) rows: Vec<Row>,
    pub(crate) styles: Vec<LineStyle>,
    pub(crate) pills: Vec<Vec<(Range<usize>, Highlight)>>,
}

impl<Row> Default for Lines<Row> {
    fn default() -> Self {
        Self {
            text: String::new(),
            rows: Vec::new(),
            styles: Vec::new(),
            pills: Vec::new(),
        }
    }
}

impl<Row> Lines<Row> {
    pub(crate) fn add(&mut self, row: Row, line: Line) {
        if !self.rows.is_empty() {
            self.text.push('\n');
        }
        self.text.push_str(&line.text);
        self.rows.push(row);
        self.styles.push(line.style);
        self.pills.push(line.pills);
    }

    pub(crate) fn style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    pub(crate) fn pills(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
        self.pills.get(line).cloned().unwrap_or_default()
    }
}

pub(crate) fn thousands(number: u64) -> String {
    let digits = number.to_string();
    let mut out = String::new();
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_collect_text_spans_and_pills() {
        let mut lines: Lines<u8> = Lines::default();
        let mut line = Line::default();
        line.push("id", Some(Highlight::Heading));
        line.pad(5);
        line.pill("PK", Highlight::Constant);
        lines.add(1, line);
        lines.add(2, Line::default());

        assert_eq!(lines.text, "id   PK\n");
        assert_eq!(lines.rows, [1, 2]);
        assert_eq!(
            lines.style(0).spans,
            [(0..2, Highlight::Heading), (5..7, Highlight::Constant)]
        );
        assert_eq!(lines.pills(0), [(5..7, Highlight::Constant)]);
        assert_eq!(lines.pills(9), []);
    }

    #[test]
    fn large_numbers_get_separators() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(3_503), "3,503");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
