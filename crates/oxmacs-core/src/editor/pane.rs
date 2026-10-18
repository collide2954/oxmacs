use std::ops::Range;

use crate::display::{self, Layout, LineStyle, Row, ScreenLine, Viewport};
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::movement::display_column;
use crate::files::LineEnding;
use crate::text::Buffer;
use crate::theme::Highlight;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenRow {
    pub offset: isize,
    pub line: usize,
    pub chars: Range<usize>,
    pub column: usize,
    pub end_column: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct Pane<'a> {
    editor: &'a Editor,
    index: usize,
    viewport: Viewport,
    point: usize,
    selected: bool,
}

impl<'a> Pane<'a> {
    pub(super) fn new(
        editor: &'a Editor,
        index: usize,
        viewport: Viewport,
        point: usize,
        selected: bool,
    ) -> Self {
        Self {
            editor,
            index,
            viewport,
            point,
            selected,
        }
    }

    pub fn buffer(&self) -> &'a Buffer {
        self.editor
            .buffers
            .get(self.index)
            .unwrap_or_else(|| self.editor.buffers.current())
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn point(&self) -> usize {
        self.point
    }

    pub fn preview_source(&self) -> Option<&'a Buffer> {
        let Some(Mode::Preview(source)) = self.editor.buffers.mode(self.index) else {
            return None;
        };
        let index = self
            .editor
            .buffers
            .position(|buffer| buffer.id() == *source)?;
        self.editor.buffers.get(index)
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn minibuffer_active(&self) -> bool {
        self.editor.minibuffer.is_some()
    }

    pub fn tab_width(&self) -> usize {
        self.editor.tab_width
    }

    fn layout(&self) -> Layout<'a> {
        Layout::new(
            self.buffer(),
            self.viewport.wrap_width(),
            self.editor.tab_width,
        )
    }

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
        let spot = layout.screen_line(self.point);
        let row = layout
            .distance(self.viewport.top(), spot, self.viewport.rows())
            .unwrap_or(0);
        (layout.column(self.point), row)
    }

    pub fn cursor_glyph(&self) -> (String, usize) {
        let buffer = self.buffer();
        let line = buffer.line_text(buffer.line_of(self.point));
        display::cursor_glyph(&line, buffer.column(self.point), self.editor.tab_width)
    }

    pub fn line_style(&self, line: usize) -> LineStyle {
        self.editor.line_style_at(self.index, line)
    }

    pub fn line_marks(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
        if self.selected {
            self.editor.line_marks(line)
        } else {
            Vec::new()
        }
    }

    pub fn mode_line(&self) -> String {
        let buffer = self.buffer();
        let state = if buffer.is_read_only() {
            "%%"
        } else if buffer.is_modified() {
            "**"
        } else {
            "--"
        };
        let crlf = buffer
            .file()
            .is_some_and(|file| file.format.line_ending == LineEnding::CrLf);
        let mode = self
            .editor
            .buffers
            .language(self.index)
            .map(|language| format!("   ({})", language.name()))
            .unwrap_or_default();
        format!(
            "{state} {}   L{} C{}{}{mode}",
            buffer.name(),
            buffer.line_of(self.point) + 1,
            display_column(buffer, self.point, self.editor.tab_width),
            if crlf { "   CRLF" } else { "" }
        )
    }
}

impl Editor {
    pub fn pane(&self) -> Pane<'_> {
        Pane::new(self, 0, self.viewport, self.buffers.current().point(), true)
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
            summary(&editor.pane().screen_rows(0, 5)),
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

        let rows = summary(&editor.pane().screen_rows(-2, 2));

        assert_eq!(rows, [(-2, 0, 0..4, 0, Some(4)), (-1, 0, 4..8, 4, Some(8))]);
        assert_eq!(summary(&editor.pane().screen_rows(-9, 1))[0].0, -2);
    }

    #[test]
    fn the_cursor_sits_on_a_screen_row_and_column() {
        let mut editor = wrapped();

        press(&mut editor, "C-n C-f");

        assert_eq!(editor.pane().cursor_screen(), (1, 1));
    }

    #[test]
    fn distances_between_screen_lines_are_bounded() {
        let editor = wrapped();
        let pane = editor.pane();
        let at = |line, row| ScreenLine { line, row };

        assert_eq!(pane.screen_distance(at(0, 0), at(1, 0), 10), Some(3));
        assert_eq!(pane.screen_distance(at(1, 0), at(0, 1), 10), Some(-2));
        assert_eq!(pane.screen_distance(at(0, 0), at(1, 0), 2), None);
    }

    #[test]
    fn the_cursor_glyph_is_the_one_at_point() {
        let editor = editor_with("a日b", 1);

        assert_eq!(editor.pane().cursor_glyph(), ("日".to_owned(), 2));
    }
}
