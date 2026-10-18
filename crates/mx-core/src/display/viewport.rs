use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    top: usize,
    rows: usize,
}

impl Viewport {
    pub fn new(rows: usize) -> Self {
        Self {
            top: 0,
            rows: rows.max(1),
        }
    }

    pub fn top(&self) -> usize {
        self.top
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn lines(&self) -> Range<usize> {
        self.top..self.top + self.rows
    }

    pub fn reveal(&mut self, line: usize) {
        if !self.lines().contains(&line) {
            self.top = line.saturating_sub(self.rows / 2);
        }
    }

    pub fn scroll_by(&mut self, delta: isize, last_line: usize) {
        self.top = self.top.saturating_add_signed(delta).min(last_line);
    }

    pub fn resize(&mut self, rows: usize, line: usize) {
        self.rows = rows.max(1);
        self.reveal(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_the_top() {
        let viewport = Viewport::new(10);

        assert_eq!(viewport.top(), 0);
        assert_eq!(viewport.lines(), 0..10);
    }

    #[test]
    fn visible_line_does_not_scroll() {
        let mut viewport = Viewport::new(10);

        viewport.reveal(9);

        assert_eq!(viewport.top(), 0);
    }

    #[test]
    fn line_below_view_recenters() {
        let mut viewport = Viewport::new(10);

        viewport.reveal(10);

        assert_eq!(viewport.top(), 5);
        assert_eq!(viewport.lines(), 5..15);
    }

    #[test]
    fn line_above_view_recenters_without_going_negative() {
        let mut viewport = Viewport::new(10);
        viewport.reveal(40);

        viewport.reveal(30);
        assert_eq!(viewport.top(), 25);

        viewport.reveal(2);
        assert_eq!(viewport.top(), 0);
    }

    #[test]
    fn resizing_keeps_a_line_visible() {
        let mut viewport = Viewport::new(10);
        viewport.reveal(9);

        viewport.resize(4, 9);

        assert!(viewport.lines().contains(&9));
    }

    #[test]
    fn scrolling_moves_the_top_within_the_text() {
        let mut viewport = Viewport::new(10);

        viewport.scroll_by(5, 100);
        assert_eq!(viewport.top(), 5);

        viewport.scroll_by(-50, 100);
        assert_eq!(viewport.top(), 0);

        viewport.scroll_by(500, 100);
        assert_eq!(viewport.top(), 100);
    }

    #[test]
    fn always_shows_at_least_one_row() {
        let viewport = Viewport::new(0);

        assert_eq!(viewport.rows(), 1);
    }
}
