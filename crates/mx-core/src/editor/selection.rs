use crate::command::Command;
use crate::editor::Editor;

impl Editor {
    pub(super) fn set_mark_command(&mut self, previous: Option<Command>) {
        if previous == Some(Command::SetMarkCommand) && self.buffer.mark_active() {
            self.buffer.deactivate_mark();
            self.show_message("Mark deactivated");
        } else {
            self.buffer.set_mark(self.buffer.point());
            self.show_message("Mark set");
        }
    }

    pub(super) fn exchange_point_and_mark(&mut self) {
        if !self.buffer.exchange_point_and_mark() {
            self.show_message("No mark set in this buffer");
        }
    }

    pub(super) fn mark_whole_buffer(&mut self) {
        self.buffer.set_mark(self.buffer.len_chars());
        self.buffer.set_point(0);
    }

    pub(super) fn keyboard_quit(&mut self) {
        self.buffer.deactivate_mark();
        self.show_message("Quit");
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::{editor_with, press};

    #[test]
    fn set_mark_then_motion_selects() {
        let mut editor = editor_with("hello world", 0);

        press(&mut editor, "C-SPC");
        assert_eq!(editor.message(), Some("Mark set"));

        press(&mut editor, "M-f");
        assert_eq!(editor.buffer().selection(), Some(0..5));
    }

    #[test]
    fn set_mark_twice_deactivates_but_keeps_the_mark() {
        let mut editor = editor_with("hello", 2);

        press(&mut editor, "C-SPC C-SPC");

        assert_eq!(editor.message(), Some("Mark deactivated"));
        assert_eq!(editor.buffer().selection(), None);
        assert_eq!(editor.buffer().mark(), Some(2));
    }

    #[test]
    fn c_at_sets_the_mark_too() {
        let mut editor = editor_with("hello", 1);

        press(&mut editor, "C-@ C-f");

        assert_eq!(editor.buffer().selection(), Some(1..2));
    }

    #[test]
    fn keyboard_quit_deactivates_the_selection() {
        let mut editor = editor_with("hello", 0);

        press(&mut editor, "C-SPC C-f C-g");

        assert_eq!(editor.buffer().selection(), None);
    }

    #[test]
    fn exchange_point_and_mark_swaps_ends() {
        let mut editor = editor_with("hello", 2);

        press(&mut editor, "C-SPC C-f C-f C-x C-x");

        assert_eq!(editor.buffer().point(), 2);
        assert_eq!(editor.buffer().selection(), Some(2..4));
    }

    #[test]
    fn exchange_point_and_mark_without_a_mark_reports_it() {
        let mut editor = editor_with("hello", 2);

        press(&mut editor, "C-x C-x");

        assert_eq!(editor.message(), Some("No mark set in this buffer"));
    }

    #[test]
    fn select_all_selects_the_whole_buffer() {
        for keys in ["C-x h", "s-a"] {
            let mut editor = editor_with("one\ntwo", 3);

            press(&mut editor, keys);

            assert_eq!(editor.buffer().selection(), Some(0..7), "{keys}");
            assert_eq!(editor.buffer().point(), 0, "{keys}");
        }
    }
}
