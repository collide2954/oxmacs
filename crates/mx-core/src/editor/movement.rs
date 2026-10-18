use crate::editor::Editor;
use crate::text::{Buffer, motion};

impl Editor {
    pub(super) fn move_point(&mut self, motion: impl FnOnce(&Buffer, usize) -> usize) {
        let buffer = self.current_mut();
        let target = motion(buffer, buffer.point());
        buffer.set_point(target);
    }

    pub(super) fn move_vertically(&mut self, lines: isize, goal_column: Option<usize>) {
        let buffer = self.current_mut();
        let point = buffer.point();
        let goal = goal_column.unwrap_or_else(|| buffer.column(point));
        let target = motion::vertical(buffer, point, lines, goal);
        buffer.set_point(target);
        self.goal_column = Some(goal);
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::point_after;

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
