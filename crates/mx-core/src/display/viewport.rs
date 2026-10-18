use crate::display::{Layout, ScreenLine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    top: ScreenLine,
    rows: usize,
    columns: usize,
}

impl Viewport {
    pub fn new(rows: usize, columns: usize) -> Self {
        Self {
            top: ScreenLine::default(),
            rows: rows.max(1),
            columns,
        }
    }

    pub fn top(&self) -> ScreenLine {
        self.top
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn wrap_width(&self) -> usize {
        self.columns.saturating_sub(1)
    }

    pub fn set_top(&mut self, top: ScreenLine) {
        self.top = top;
    }

    pub fn resize(&mut self, rows: usize, columns: usize) {
        self.rows = rows.max(1);
        self.columns = columns;
    }

    pub fn shows(&self, spot: ScreenLine, layout: &Layout<'_>) -> bool {
        spot >= self.top && layout.distance(self.top, spot, self.rows - 1).is_some()
    }

    pub fn reveal(&mut self, spot: ScreenLine, layout: &Layout<'_>) {
        self.top = layout.step(self.top, 0);
        if !self.shows(spot, layout) {
            self.top = layout.step(spot, -(self.rows / 2).cast_signed());
        }
    }

    pub fn scroll_by(&mut self, delta: isize, layout: &Layout<'_>) {
        self.top = layout.step(self.top, delta);
    }

    pub fn bottom(&self, layout: &Layout<'_>) -> ScreenLine {
        layout.step(self.top, (self.rows - 1).cast_signed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Buffer;

    fn numbered(count: usize) -> Buffer {
        let text: Vec<String> = (0..count).map(|line| line.to_string()).collect();
        Buffer::with_text("t", &text.join("\n"))
    }

    fn at(line: usize, row: usize) -> ScreenLine {
        ScreenLine { line, row }
    }

    #[test]
    fn starts_at_the_top() {
        let viewport = Viewport::new(10, 80);

        assert_eq!(viewport.top(), at(0, 0));
        assert_eq!(viewport.rows(), 10);
        assert_eq!(viewport.columns(), 80);
    }

    #[test]
    fn visible_lines_do_not_scroll() {
        let buffer = numbered(100);
        let layout = Layout::new(&buffer, 0, 8);
        let mut viewport = Viewport::new(10, 0);

        viewport.reveal(at(9, 0), &layout);

        assert_eq!(viewport.top(), at(0, 0));
    }

    #[test]
    fn lines_below_the_view_recenter() {
        let buffer = numbered(100);
        let layout = Layout::new(&buffer, 0, 8);
        let mut viewport = Viewport::new(10, 0);

        viewport.reveal(at(10, 0), &layout);

        assert_eq!(viewport.top(), at(5, 0));
    }

    #[test]
    fn lines_above_the_view_recenter_without_going_negative() {
        let buffer = numbered(100);
        let layout = Layout::new(&buffer, 0, 8);
        let mut viewport = Viewport::new(10, 0);
        viewport.reveal(at(40, 0), &layout);

        viewport.reveal(at(30, 0), &layout);
        assert_eq!(viewport.top(), at(25, 0));

        viewport.reveal(at(2, 0), &layout);
        assert_eq!(viewport.top(), at(0, 0));
    }

    #[test]
    fn scrolling_moves_the_top_within_the_text() {
        let buffer = numbered(101);
        let layout = Layout::new(&buffer, 0, 8);
        let mut viewport = Viewport::new(10, 0);

        viewport.scroll_by(5, &layout);
        assert_eq!(viewport.top(), at(5, 0));

        viewport.scroll_by(-50, &layout);
        assert_eq!(viewport.top(), at(0, 0));

        viewport.scroll_by(500, &layout);
        assert_eq!(viewport.top(), at(100, 0));
    }

    #[test]
    fn wrapped_rows_count_as_rows() {
        let buffer = Buffer::with_text("t", &"x".repeat(15));
        let layout = Layout::new(&buffer, 3, 8);
        let mut viewport = Viewport::new(4, 4);

        assert!(viewport.shows(at(0, 3), &layout));
        viewport.reveal(at(0, 4), &layout);

        assert_eq!(viewport.top(), at(0, 2));
        assert_eq!(viewport.bottom(&layout), at(0, 4));
    }

    #[test]
    fn the_wrap_width_leaves_a_column_for_the_continuation_mark() {
        assert_eq!(Viewport::new(10, 80).wrap_width(), 79);
        assert_eq!(Viewport::new(10, 0).wrap_width(), 0);
    }

    #[test]
    fn always_shows_at_least_one_row() {
        assert_eq!(Viewport::new(0, 0).rows(), 1);
    }
}
