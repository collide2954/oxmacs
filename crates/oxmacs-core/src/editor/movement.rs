use crate::display::{self, Layout};
use crate::editor::Editor;
use crate::text::Buffer;

pub(super) fn display_column(buffer: &Buffer, point: usize, tab_width: usize) -> usize {
    let line = buffer.line_text(buffer.line_of(point));
    display::display_column(&line, buffer.column(point), tab_width)
}

impl Editor {
    pub(super) fn move_point(&mut self, motion: impl FnOnce(&Buffer, usize) -> usize) {
        let buffer = self.current_mut();
        let target = motion(buffer, buffer.point());
        buffer.set_point(target);
    }

    pub(super) fn move_vertically(&mut self, lines: isize, goal_column: Option<usize>) {
        let width = if self.minibuffer.is_some() {
            0
        } else {
            self.viewport.wrap_width()
        };
        let tab_width = self.tab_width;
        let buffer = self.current_mut();
        let layout = Layout::new(buffer, width, tab_width);
        let point = buffer.point();
        let from = layout.screen_line(point);
        let goal = goal_column.unwrap_or_else(|| layout.column(point));
        let to = layout.step(from, lines);
        let target = match to == from {
            true if lines < 0 => 0,
            true => buffer.len_chars(),
            false => layout.position_at(to, goal),
        };
        buffer.set_point(target);
        self.goal_column = Some(goal);
    }

    pub(super) fn go_to_line(&mut self, line: usize, column: usize) {
        let buffer = self.buffers.current_mut();
        let line = line.saturating_sub(1);
        let target = buffer.line_start(line) + column.saturating_sub(1);
        buffer.set_point(target.min(buffer.line_end(line)));
        self.reveal_point();
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::point_after;

    #[test]
    fn line_motion_moves_by_screen_rows_on_wrapped_lines() {
        let mut editor = crate::editor::test_support::editor_with("abcdefghij\nxy", 1);
        editor.resize_window(10, 5);

        for (keys, expected) in [("C-n", 5), ("C-n", 9), ("C-n", 12), ("C-p", 9), ("C-p", 5)] {
            crate::editor::test_support::press(&mut editor, keys);
            assert_eq!(editor.buffer().point(), expected, "{keys}");
        }
    }

    #[test]
    fn char_motions() {
        assert_eq!(point_after("hello", 2, "C-f"), 3);
        assert_eq!(point_after("hello", 2, "<right>"), 3);
        assert_eq!(point_after("hello", 2, "C-b"), 1);
        assert_eq!(point_after("hello", 2, "<left>"), 1);
    }

    #[test]
    fn line_motions() {
        let text = "one\ntwo\nthree";
        assert_eq!(point_after(text, 5, "C-a"), 4);
        assert_eq!(point_after(text, 5, "<home>"), 4);
        assert_eq!(point_after(text, 5, "C-e"), 7);
        assert_eq!(point_after(text, 5, "<end>"), 7);
        assert_eq!(point_after(text, 5, "C-n"), 9);
        assert_eq!(point_after(text, 5, "<down>"), 9);
        assert_eq!(point_after(text, 5, "C-p"), 1);
        assert_eq!(point_after(text, 5, "<up>"), 1);
    }

    #[test]
    fn consecutive_vertical_motions_keep_goal_column() {
        let text = "abcdef\nab\nabcdef";
        assert_eq!(point_after(text, 5, "C-n"), 9);
        assert_eq!(point_after(text, 5, "C-n C-n"), 15);
        assert_eq!(point_after(text, 15, "C-p C-p"), 5);
    }

    #[test]
    fn other_commands_reset_goal_column() {
        let text = "abcdef\nab\nabcdef";
        assert_eq!(point_after(text, 5, "C-n C-b C-n"), 11);
    }

    #[test]
    fn word_motions() {
        let text = "alpha beta gamma";
        assert_eq!(point_after(text, 0, "M-f"), 5);
        assert_eq!(point_after(text, 0, "C-<right>"), 5);
        assert_eq!(point_after(text, 16, "M-b"), 11);
        assert_eq!(point_after(text, 16, "C-<left>"), 11);
    }

    #[test]
    fn buffer_motions() {
        let text = "one\ntwo";
        assert_eq!(point_after(text, 5, "M-<"), 0);
        assert_eq!(point_after(text, 5, "C-<home>"), 0);
        assert_eq!(point_after(text, 1, "M->"), 7);
        assert_eq!(point_after(text, 1, "C-<end>"), 7);
    }
}
