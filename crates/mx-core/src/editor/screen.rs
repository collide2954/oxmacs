use std::ops::Range;

use crate::display::{Row, ScreenLine};
use crate::editor::Editor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenRow {
    pub offset: isize,
    pub line: usize,
    pub chars: Range<usize>,
    pub column: usize,
    pub end_column: Option<usize>,
}

impl Editor {
    pub fn screen_rows(&self, from: isize, count: usize) -> Vec<ScreenRow> {
        let layout = self.layout();
        let top = layout.step(self.viewport.top(), 0);
        let mut at = layout.step(top, from);
        let first = layout
            .distance(top, at, from.unsigned_abs())
            .unwrap_or(from);
        let mut rows = Vec::with_capacity(count);
        let mut wrapped: Option<(usize, Vec<Row>)> = None;
        for offset in (first..).take(count) {
            let line_rows = match &wrapped {
                Some((line, rows)) if *line == at.line => rows,
                _ => &wrapped.insert((at.line, layout.rows(at.line))).1,
            };
            let row = &line_rows[at.row];
            rows.push(ScreenRow {
                offset,
                line: at.line,
                chars: row.chars.clone(),
                column: row.column,
                end_column: line_rows.get(at.row + 1).map(|next| next.column),
            });
            let next = layout.step(at, 1);
            if next == at {
                break;
            }
            at = next;
        }
        rows
    }

    pub fn screen_distance(&self, from: ScreenLine, to: ScreenLine, limit: usize) -> Option<isize> {
        self.layout().distance(from, to, limit)
    }

    pub fn cursor_screen(&self) -> (usize, isize) {
        let layout = self.layout();
        let point = self.buffers.current().point();
        let spot = layout.screen_line(point);
        let row = layout
            .distance(self.viewport.top(), spot, self.viewport.rows())
            .unwrap_or(0);
        (layout.column(point), row)
    }
}

#[cfg(test)]
mod tests {
    use super::ScreenRow;
    use crate::display::ScreenLine;
    use crate::editor::test_support::{editor_with, press};

    fn wrapped() -> crate::editor::Editor {
        let mut editor = editor_with("abcdefghij\nxy", 0);
        editor.resize_window(10, 5);
        editor
    }

    type Summary = (isize, usize, std::ops::Range<usize>, usize, Option<usize>);

    fn summary(rows: &[ScreenRow]) -> Vec<Summary> {
        rows.iter()
            .map(|row| {
                (
                    row.offset,
                    row.line,
                    row.chars.clone(),
                    row.column,
                    row.end_column,
                )
            })
            .collect()
    }

    #[test]
    fn screen_rows_follow_the_wrapped_text_from_the_top() {
        let editor = wrapped();

        assert_eq!(
            summary(&editor.screen_rows(0, 5)),
            [
                (0, 0, 0..4, 0, Some(4)),
                (1, 0, 4..8, 4, Some(8)),
                (2, 0, 8..10, 8, None),
                (3, 1, 0..2, 0, None),
            ]
        );
    }

    #[test]
    fn screen_rows_above_the_top_are_offset_negatively() {
        let mut editor = wrapped();
        editor.resize_window(2, 5);
        press(&mut editor, "M->");
        assert_eq!(editor.viewport().top(), ScreenLine { line: 0, row: 2 });

        let rows = summary(&editor.screen_rows(-2, 2));

        assert_eq!(rows, [(-2, 0, 0..4, 0, Some(4)), (-1, 0, 4..8, 4, Some(8))]);
        assert_eq!(summary(&editor.screen_rows(-9, 1))[0].0, -2);
    }

    #[test]
    fn the_cursor_sits_on_a_screen_row_and_column() {
        let mut editor = wrapped();

        press(&mut editor, "C-n C-f");

        assert_eq!(editor.cursor_screen(), (1, 1));
    }

    #[test]
    fn distances_between_screen_lines_are_bounded() {
        let editor = wrapped();
        let at = |line, row| ScreenLine { line, row };

        assert_eq!(editor.screen_distance(at(0, 0), at(1, 0), 10), Some(3));
        assert_eq!(editor.screen_distance(at(1, 0), at(0, 1), 10), Some(-2));
        assert_eq!(editor.screen_distance(at(0, 0), at(1, 0), 2), None);
    }
}
