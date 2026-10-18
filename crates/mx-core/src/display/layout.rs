use crate::display::{Row, char_at_column, display_column, wrap};
use crate::text::Buffer;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScreenLine {
    pub line: usize,
    pub row: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Layout<'a> {
    buffer: &'a Buffer,
    width: usize,
    tab_width: usize,
}

impl<'a> Layout<'a> {
    pub fn new(buffer: &'a Buffer, width: usize, tab_width: usize) -> Self {
        Self {
            buffer,
            width,
            tab_width,
        }
    }

    pub fn rows(&self, line: usize) -> Vec<Row> {
        wrap(&self.buffer.line_text(line), self.width, self.tab_width)
    }

    pub fn screen_line(&self, position: usize) -> ScreenLine {
        let line = self.buffer.line_of(position);
        let index = position.min(self.buffer.len_chars()) - self.buffer.line_start(line);
        let row = self
            .rows(line)
            .iter()
            .rposition(|row| row.chars.start <= index)
            .unwrap_or(0);
        ScreenLine { line, row }
    }

    pub fn column(&self, position: usize) -> usize {
        let spot = self.screen_line(position);
        let text = self.buffer.line_text(spot.line);
        let index = position.min(self.buffer.len_chars()) - self.buffer.line_start(spot.line);
        let start = self.rows(spot.line)[spot.row].column;
        display_column(&text, index, self.tab_width) - start
    }

    pub fn position_at(&self, spot: ScreenLine, column: usize) -> usize {
        let spot = self.step(spot, 0);
        let rows = self.rows(spot.line);
        let row = &rows[spot.row];
        let text = self.buffer.line_text(spot.line);
        let index = char_at_column(&text, row.column + column, self.tab_width);
        let last = if spot.row + 1 == rows.len() {
            row.chars.end
        } else {
            row.chars.end - 1
        };
        self.buffer.line_start(spot.line) + index.clamp(row.chars.start, last)
    }

    pub fn step(&self, from: ScreenLine, delta: isize) -> ScreenLine {
        let line = from.line.min(self.buffer.len_lines() - 1);
        let last = self.rows(line).len() - 1;
        let mut at = ScreenLine {
            line,
            row: if line == from.line {
                from.row.min(last)
            } else {
                last
            },
        };
        let mut remaining = delta.unsigned_abs();
        if delta >= 0 {
            loop {
                let last = self.rows(at.line).len() - 1;
                let room = last - at.row.min(last);
                if remaining <= room || at.line + 1 >= self.buffer.len_lines() {
                    at.row = (at.row + remaining).min(last);
                    return at;
                }
                remaining -= room + 1;
                at = ScreenLine {
                    line: at.line + 1,
                    row: 0,
                };
            }
        }
        loop {
            if remaining <= at.row || at.line == 0 {
                at.row = at.row.saturating_sub(remaining);
                return at;
            }
            remaining -= at.row + 1;
            let line = at.line - 1;
            at = ScreenLine {
                line,
                row: self.rows(line).len() - 1,
            };
        }
    }

    pub fn distance(&self, from: ScreenLine, to: ScreenLine, limit: usize) -> Option<isize> {
        if to < from {
            return self.distance(to, from, limit).map(|rows| -rows);
        }
        let mut rows = 0;
        let mut at = from;
        while at.line < to.line {
            rows += self.rows(at.line).len() - at.row;
            if rows > limit {
                return None;
            }
            at = ScreenLine {
                line: at.line + 1,
                row: 0,
            };
        }
        rows += to.row - at.row;
        (rows <= limit).then(|| rows.cast_signed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer() -> Buffer {
        Buffer::with_text("t", "abcdefgh\nxy\n日本語x")
    }

    fn at(line: usize, row: usize) -> ScreenLine {
        ScreenLine { line, row }
    }

    #[test]
    fn lines_split_into_rows_at_the_width() {
        let buffer = buffer();
        let layout = Layout::new(&buffer, 3, 8);

        assert_eq!(layout.rows(0).len(), 3);
        assert_eq!(layout.rows(1).len(), 1);
        assert_eq!(layout.rows(2).len(), 3);
    }

    #[test]
    fn positions_map_to_screen_lines() {
        let buffer = buffer();
        let layout = Layout::new(&buffer, 3, 8);

        assert_eq!(layout.screen_line(0), at(0, 0));
        assert_eq!(layout.screen_line(4), at(0, 1));
        assert_eq!(layout.screen_line(8), at(0, 2));
        assert_eq!(layout.screen_line(9), at(1, 0));
        assert_eq!(layout.screen_line(13), at(2, 1));
    }

    #[test]
    fn stepping_moves_by_screen_rows_within_the_buffer() {
        let buffer = buffer();
        let layout = Layout::new(&buffer, 3, 8);

        assert_eq!(layout.step(at(0, 0), 2), at(0, 2));
        assert_eq!(layout.step(at(0, 0), 3), at(1, 0));
        assert_eq!(layout.step(at(0, 2), 4), at(2, 2));
        assert_eq!(layout.step(at(1, 0), -1), at(0, 2));
        assert_eq!(layout.step(at(1, 0), -10), at(0, 0));
        assert_eq!(layout.step(at(1, 0), 10), at(2, 2));
    }

    #[test]
    fn distances_count_screen_rows_up_to_a_limit() {
        let buffer = buffer();
        let layout = Layout::new(&buffer, 3, 8);

        assert_eq!(layout.distance(at(0, 1), at(1, 0), 10), Some(2));
        assert_eq!(layout.distance(at(1, 0), at(0, 1), 10), Some(-2));
        assert_eq!(layout.distance(at(0, 0), at(2, 2), 3), None);
    }

    #[test]
    fn columns_are_counted_from_the_start_of_the_row() {
        let buffer = buffer();
        let layout = Layout::new(&buffer, 3, 8);

        assert_eq!(layout.column(4), 1);
        assert_eq!(layout.column(8), 2);
        assert_eq!(layout.column(13), 0);
    }

    #[test]
    fn positions_come_from_a_screen_line_and_a_column() {
        let buffer = buffer();
        let layout = Layout::new(&buffer, 3, 8);

        assert_eq!(layout.position_at(at(0, 1), 1), 4);
        assert_eq!(layout.position_at(at(0, 1), 9), 5);
        assert_eq!(layout.position_at(at(0, 2), 9), 8);
        assert_eq!(layout.position_at(at(9, 0), 0), 14);
    }

    #[test]
    fn stale_screen_lines_are_clamped_into_the_buffer() {
        let buffer = buffer();
        let layout = Layout::new(&buffer, 3, 8);

        assert_eq!(layout.step(at(9, 7), 0), at(2, 2));
        assert_eq!(layout.step(at(1, 5), 0), at(1, 0));
    }

    #[test]
    fn a_zero_width_keeps_one_row_per_line() {
        let buffer = buffer();
        let layout = Layout::new(&buffer, 0, 8);

        assert_eq!(layout.rows(0).len(), 1);
        assert_eq!(layout.step(at(0, 0), 1), at(1, 0));
    }
}
