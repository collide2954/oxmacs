use crate::display::Layout;
use crate::editor::buffers::Mode;
use crate::editor::{Editor, Effect};
use crate::text::{Buffer, motion};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clicks {
    Single,
    Double,
    Triple,
}

impl Editor {
    pub fn click(&mut self, cell: Cell, clicks: Clicks, extend: bool) {
        self.end_command_chain();
        let position = self.position_at(cell);
        self.line_drag = None;
        if self.hear_click(position, clicks, extend) {
            return;
        }
        if clicks == Clicks::Single && !extend && self.buffers.splash(0).is_some() {
            let buffer = self.buffers.current_mut();
            buffer.deactivate_mark();
            buffer.set_point(position);
            self.drag_anchor = None;
            if self
                .buffers
                .splash(0)
                .is_some_and(|splash| splash.has_button(self.point_line()))
            {
                self.push_button();
            }
            return;
        }
        let line = self.buffers.current().line_of(position);
        if clicks == Clicks::Single && !extend && self.diagnostic_on_line(line).is_some() {
            let buffer = self.buffers.current_mut();
            buffer.deactivate_mark();
            buffer.set_point(position);
            self.drag_anchor = None;
            self.goto_diagnostic();
            return;
        }
        if clicks == Clicks::Single
            && !extend
            && let Some(error) = self.error_on_line(line)
        {
            let buffer = self.buffers.current_mut();
            buffer.deactivate_mark();
            buffer.set_point(position);
            self.drag_anchor = None;
            self.visit_error(error);
            return;
        }
        if clicks == Clicks::Double
            && matches!(self.buffers.mode(0), Some(Mode::Plugin(_)))
            && let Some(command) = self.bound_command("RET")
        {
            let buffer = self.buffers.current_mut();
            buffer.deactivate_mark();
            buffer.set_point(position);
            self.drag_anchor = None;
            self.run_id(command, false);
            return;
        }
        let buffer = self.buffers.current_mut();
        match clicks {
            Clicks::Single if extend => {
                if !buffer.mark_active() {
                    buffer.set_mark(buffer.point());
                }
                buffer.set_point(position);
            }
            Clicks::Single => {
                buffer.deactivate_mark();
                buffer.set_point(position);
            }
            Clicks::Double => {
                let word = motion::word_at(buffer, position);
                buffer.set_mark(word.start);
                buffer.set_point(word.end);
            }
            Clicks::Triple => {
                let line = buffer.line_of(position);
                let end = (buffer.line_end(line) + 1).min(buffer.len_chars());
                buffer.set_mark(buffer.line_start(line));
                buffer.set_point(end);
            }
        }
        self.drag_anchor = Some(
            buffer
                .mark()
                .filter(|_| buffer.mark_active())
                .unwrap_or(position),
        );
        self.publish_primary();
    }

    pub fn drag(&mut self, cell: Cell) {
        let position = self.position_at(cell);
        if self.hear_drag(position) {
            return;
        }
        let Some(anchor) = self.drag_anchor else {
            return;
        };
        let buffer = self.buffers.current_mut();
        if !buffer.mark_active() {
            buffer.set_mark(anchor);
        }
        buffer.set_point(position);
        self.publish_primary();
    }

    pub fn click_line_number(&mut self, row: usize, extend: bool) {
        self.end_command_chain();
        let line = self.line_at_row(row);
        let buffer = self.buffers.current();
        let anchor = if extend {
            buffer.line_of(
                buffer
                    .mark()
                    .filter(|_| buffer.mark_active())
                    .unwrap_or(buffer.point()),
            )
        } else {
            line
        };
        self.drag_anchor = None;
        self.line_anchor = Some(anchor);
        self.select_lines(anchor, line);
    }

    pub fn drag_line_numbers(&mut self, row: usize) {
        if let Some(anchor) = self.line_anchor {
            let line = self.line_at_row(row);
            self.select_lines(anchor, line);
        }
    }

    fn line_at_row(&self, row: usize) -> usize {
        let position = self.position_at(Cell { row, column: 0 });
        self.buffers.current().line_of(position)
    }

    fn select_lines(&mut self, from: usize, to: usize) {
        let buffer = self.buffers.current_mut();
        let after = |buffer: &Buffer, line| (buffer.line_end(line) + 1).min(buffer.len_chars());
        let (mark, point) = if to >= from {
            (buffer.line_start(from), after(buffer, to))
        } else {
            (after(buffer, from), buffer.line_start(to))
        };
        buffer.set_mark(mark);
        buffer.set_point(point);
        self.publish_primary();
    }

    pub fn middle_click(&mut self, cell: Cell) {
        self.end_command_chain();
        let position = self.position_at(cell);
        self.buffers.current_mut().deactivate_mark();
        self.buffers.current_mut().set_point(position);
        self.effects.push(Effect::ReadPrimary);
    }

    pub fn scroll_lines(&mut self, delta: isize) {
        self.exit_search();
        let layout = Layout::new(
            self.buffers.current(),
            self.viewport.wrap_width(),
            crate::editor::tab_width_in(self.buffers.current(), self.tab_width),
        );
        self.viewport.scroll_by(delta, &layout);
        let point = self.buffers.current().point();
        let spot = layout.screen_line(point);
        let target = if spot < self.viewport.top() {
            self.viewport.top()
        } else if self.viewport.shows(spot, &layout) {
            return;
        } else {
            self.viewport.bottom(&layout)
        };
        let goal = self.goal_column.unwrap_or_else(|| layout.column(point));
        let position = layout.position_at(target, goal);
        self.buffers.current_mut().set_point(position);
        self.goal_column = Some(goal);
    }

    pub(in crate::editor) fn position_at(&self, cell: Cell) -> usize {
        let layout = self.layout();
        let rows = isize::try_from(cell.row).unwrap_or(isize::MAX);
        layout.position_at(layout.step(self.viewport.top(), rows), cell.column)
    }

    fn end_command_chain(&mut self) {
        self.exit_search();
        self.last_command = None;
        self.last_id = None;
        self.goal_column = None;
        self.shift_selection = false;
        self.buffers.current_mut().close_undo_step();
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::{Cell, Clicks, Editor};
    use crate::testing::{editor_with, press, type_text};

    fn cell(line: usize, column: usize) -> Cell {
        Cell { row: line, column }
    }

    fn clicked(text: &str, at: Cell, clicks: Clicks) -> Editor {
        let mut editor = editor_with(text, 0);
        editor.click(at, clicks, false);
        editor
    }

    #[test]
    fn click_moves_point_to_the_cell() {
        let editor = clicked("hello\nworld", cell(1, 2), Clicks::Single);

        assert_eq!(editor.buffer().point(), 8);
        assert_eq!(editor.buffer().selection(), None);
    }

    #[test]
    fn clicks_outside_the_text_clamp_to_it() {
        assert_eq!(
            clicked("hello\nworld", cell(0, 99), Clicks::Single)
                .buffer()
                .point(),
            5
        );
        assert_eq!(
            clicked("hello\nworld", cell(9, 0), Clicks::Single)
                .buffer()
                .point(),
            6
        );
    }

    #[test]
    fn clicks_land_on_display_columns() {
        assert_eq!(
            clicked("\tx", cell(0, 5), Clicks::Single).buffer().point(),
            1
        );
    }

    #[test]
    fn click_clears_the_selection() {
        let mut editor = editor_with("hello", 0);
        press(&mut editor, "C-SPC C-f");

        editor.click(cell(0, 3), Clicks::Single, false);

        assert_eq!(editor.buffer().selection(), None);
    }

    #[test]
    fn dragging_selects_from_where_the_button_went_down() {
        let mut editor = editor_with("hello\nworld", 0);

        editor.click(cell(0, 1), Clicks::Single, false);
        editor.drag(cell(1, 2));

        assert_eq!(editor.buffer().selection(), Some(1..8));
    }

    #[test]
    fn double_click_selects_a_word() {
        let editor = clicked("alpha beta", cell(0, 7), Clicks::Double);

        assert_eq!(editor.buffer().selection(), Some(6..10));
    }

    #[test]
    fn double_click_on_spaces_selects_the_gap() {
        let editor = clicked("a   b", cell(0, 2), Clicks::Double);

        assert_eq!(editor.buffer().selection(), Some(1..4));
    }

    #[test]
    fn triple_click_selects_the_line_and_its_newline() {
        let editor = clicked("one\ntwo\nthree", cell(1, 1), Clicks::Triple);

        assert_eq!(editor.buffer().selection(), Some(4..8));
    }

    #[test]
    fn shift_click_extends_the_selection_from_point() {
        let mut editor = editor_with("hello world", 2);

        editor.click(cell(0, 8), Clicks::Single, true);
        assert_eq!(editor.buffer().selection(), Some(2..8));

        editor.click(cell(0, 4), Clicks::Single, true);
        assert_eq!(editor.buffer().selection(), Some(2..4));
    }

    fn lines(count: usize) -> String {
        vec!["abc"; count].join("\n")
    }

    #[test]
    fn scrolling_moves_the_window_and_pulls_point_into_view() {
        let mut editor = editor_with(&lines(50), 2);
        editor.resize_window(10, 0);

        editor.scroll_lines(3);

        assert_eq!(editor.viewport().top().line, 3);
        assert_eq!(editor.buffer().point(), 3 * 4 + 2);
    }

    #[test]
    fn scrolling_up_pulls_point_to_the_last_visible_line() {
        let mut editor = editor_with(&lines(50), 20 * 4);
        editor.resize_window(10, 0);
        editor.scroll_lines(15 - editor.viewport().top().line.cast_signed());

        editor.scroll_lines(-10);

        assert_eq!(editor.viewport().top().line, 5);
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 14);
    }

    fn ragged(count: usize) -> String {
        (0..count)
            .map(|line| if line % 2 == 0 { "abcdefghij" } else { "ab" })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn scrolling_past_short_lines_keeps_the_column() {
        let mut editor = editor_with(&ragged(50), 8);
        editor.resize_window(10, 0);

        editor.scroll_lines(3);
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 3);
        assert_eq!(editor.point_display_column(), 2);

        editor.scroll_lines(1);
        assert_eq!(editor.point_display_column(), 8);
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 4);
    }

    #[test]
    fn line_motion_after_scrolling_keeps_the_column() {
        let mut editor = editor_with(&ragged(50), 8);
        editor.resize_window(10, 0);
        editor.scroll_lines(3);

        press(&mut editor, "C-n");

        assert_eq!(editor.point_display_column(), 8);
    }

    #[test]
    fn scrolling_lines_with_tabs_uses_display_columns() {
        let text = format!("abcdefghij\n{}\n\tx", "\n".repeat(30));
        let mut editor = editor_with(&text, 8);
        editor.resize_window(5, 0);

        editor.scroll_lines(32);

        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 32);
        assert_eq!(editor.point_display_column(), 8);
    }

    #[test]
    fn clicks_land_on_wrapped_rows() {
        let mut editor = editor_with("abcdefghij", 0);
        editor.resize_window(10, 5);

        editor.click(cell(1, 2), Clicks::Single, false);

        assert_eq!(editor.buffer().point(), 6);
    }

    #[test]
    fn wheel_scrolling_counts_wrapped_rows() {
        let line = "x".repeat(12);
        let mut editor = editor_with(&format!("{line}\n{line}\n{line}"), 0);
        editor.resize_window(4, 5);

        editor.scroll_lines(2);
        assert_eq!(
            editor.viewport().top(),
            crate::display::ScreenLine { line: 0, row: 2 }
        );

        editor.scroll_lines(2);
        assert_eq!(
            editor.viewport().top(),
            crate::display::ScreenLine { line: 1, row: 1 }
        );
    }

    #[test]
    fn scrolling_stops_at_the_ends() {
        let mut editor = editor_with(&lines(50), 0);
        editor.resize_window(10, 0);

        editor.scroll_lines(-5);
        assert_eq!(editor.viewport().top().line, 0);

        editor.scroll_lines(1_000);
        assert_eq!(editor.viewport().top().line, 49);
    }

    #[test]
    fn clicking_a_line_number_selects_the_line() {
        let mut editor = editor_with("one\ntwo\nthree", 0);

        editor.click_line_number(1, false);

        assert_eq!(editor.buffer().selection(), Some(4..8));
        assert_eq!(editor.buffer().point(), 8);
    }

    #[test]
    fn dragging_over_line_numbers_selects_whole_lines() {
        let mut editor = editor_with("one\ntwo\nthree", 0);

        editor.click_line_number(0, false);
        editor.drag_line_numbers(2);
        assert_eq!(editor.buffer().selection(), Some(0..13));

        editor.click_line_number(2, false);
        editor.drag_line_numbers(1);
        assert_eq!(editor.buffer().selection(), Some(4..13));
        assert_eq!(editor.buffer().point(), 4);
    }

    #[test]
    fn shift_clicking_a_line_number_extends_by_lines() {
        let mut editor = editor_with("one\ntwo\nthree", 5);

        editor.click_line_number(2, true);

        assert_eq!(editor.buffer().selection(), Some(4..13));
    }

    #[test]
    fn clicking_ends_the_current_typing_step() {
        let mut editor = Editor::new();
        type_text(&mut editor, "ab");

        editor.click(cell(0, 0), Clicks::Single, false);
        type_text(&mut editor, "c");
        press(&mut editor, "C-/");

        assert_eq!(editor.buffer().contents(), "ab");
    }
}
