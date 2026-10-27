use std::any::Any;
use std::ops::Range;
use std::time::Instant;

use crate::display::{self, Layout, LineStyle, Row, ScreenLine, Viewport};
use crate::editor::buffers::Mode;
use crate::editor::motion::movement::display_column;
use crate::editor::special::splash::Splash;
use crate::editor::tab_width_in;
use crate::editor::windows::status::{BufferState, Status};
use crate::editor::{Editor, Gutter, PopupView};
use crate::external::diagnostics::Severity;
use crate::files::LineEnding;
use crate::special_buffer::SpecialBuffer;
use crate::syntax::Language;
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
    pub(in crate::editor) fn new(
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

    pub(in crate::editor) fn editor(&self) -> &'a Editor {
        self.editor
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

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn gutter(&self) -> Option<Gutter> {
        if self.editor.presentation.hide_chrome {
            return None;
        }
        let style = self.editor.line_numbers_at(self.index)?;
        let buffer = self.buffer();
        Some(Gutter::new(
            style,
            buffer.line_of(self.point),
            buffer.line_of(buffer.len_chars()) + 1,
            self.editor.settings.line_number_separator,
        ))
    }

    pub fn minibuffer_active(&self) -> bool {
        self.editor.minibuffer.is_some()
    }

    pub fn tab_width(&self) -> usize {
        tab_width_in(self.buffer(), self.editor.tab_width)
    }

    fn layout(&self) -> Layout<'a> {
        Layout::new(self.buffer(), self.viewport.wrap_width(), self.tab_width())
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

    pub fn is_splash(&self) -> bool {
        self.editor.buffers.special::<Splash>(self.index).is_some()
    }

    pub fn special_buffer<T: SpecialBuffer>(&self) -> Option<&'a T> {
        let Some(Mode::Special(view)) = self.editor.buffers.mode(self.index) else {
            return None;
        };
        let any: &dyn Any = view.as_ref();
        any.downcast_ref()
    }

    pub fn line_cursor(&self) -> bool {
        matches!(self.editor.buffers.mode(self.index), Some(Mode::Special(view)) if view.line_cursor())
    }

    pub fn header_lines(&self) -> Vec<(String, LineStyle)> {
        self.editor.header_lines_of(self.buffer().id())
    }

    pub fn popup(&self) -> Option<PopupView> {
        if !self.selected {
            return None;
        }
        self.editor.popup_view(
            self.buffer().id(),
            &self.layout(),
            self.viewport.top(),
            self.viewport.rows(),
        )
    }

    pub fn cursor_glyph(&self) -> (String, usize) {
        let buffer = self.buffer();
        let line = buffer.line_text(buffer.line_of(self.point));
        display::cursor_glyph(&line, buffer.column(self.point), self.tab_width())
    }

    pub fn line_style(&self, line: usize) -> LineStyle {
        self.editor.line_style_at(self.index, line)
    }

    pub fn line_diagnostics(&self, line: usize) -> Vec<(Range<usize>, Severity)> {
        self.editor.diagnostics_on_line(self.index, line)
    }

    pub fn line_note(&self, line: usize) -> Option<(String, Highlight)> {
        self.editor.note_on_line(self.index, line)
    }

    pub fn line_pills(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
        match self.editor.buffers.mode(self.index) {
            Some(Mode::Special(view)) => view.pills(line),
            _ => Vec::new(),
        }
    }

    pub fn line_flashes(&self, line: usize, now: Instant) -> Vec<(Range<usize>, Highlight, f32)> {
        self.editor.flashes_on_line(self.index, line, now)
    }

    pub fn flashing(&self, now: Instant) -> bool {
        self.editor.flashing_in(self.index, now)
    }

    pub fn line_marks(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
        if self.selected {
            self.editor.line_marks(line)
        } else {
            Vec::new()
        }
    }

    pub fn mode_line(&self) -> String {
        self.status().to_string()
    }

    pub fn status(&self) -> Status {
        let buffer = self.buffer();
        let unsaved = matches!(
            self.editor.buffers.mode(self.index),
            Some(Mode::Special(view)) if view.unsaved()
        );
        let state = if unsaved {
            BufferState::Unsaved
        } else if buffer.is_read_only() {
            BufferState::ReadOnly
        } else if buffer.is_modified() && !self.editor.buffers.is_special(self.index) {
            BufferState::Modified
        } else {
            BufferState::Unmodified
        };
        let language = self.editor.buffers.language(self.index);
        Status {
            state,
            name: buffer.name().to_owned(),
            line: buffer.line_of(self.point) + 1,
            column: display_column(buffer, self.point, self.tab_width()),
            position: match self.editor.buffers.mode(self.index) {
                Some(Mode::Special(view)) => view.position(),
                _ => None,
            },
            crlf: buffer
                .file()
                .is_some_and(|file| file.format.line_ending == LineEnding::CrLf),
            mode: language.map(Language::name),
            server: language
                .zip(buffer.file())
                .and_then(|(language, file)| self.editor.server_label(language.name(), &file.path)),
            lint: buffer
                .file()
                .and_then(|file| self.editor.lint_status(&file.path)),
        }
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
    use crate::testing::{editor_with, press};

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
