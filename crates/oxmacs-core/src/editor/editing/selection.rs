use crate::command::Command;
use crate::editor::Editor;

impl Editor {
    pub(in crate::editor) fn prepare_motion_selection(&mut self, shifted: bool) {
        if shifted && !self.current_mut().mark_active() {
            let buffer = self.current_mut();
            buffer.set_mark(buffer.point());
            self.shift_selection = true;
        } else if !shifted && self.shift_selection {
            self.current_mut().deactivate_mark();
            self.shift_selection = false;
        }
    }

    pub(in crate::editor) fn set_mark_command(&mut self, previous: Option<Command>) {
        self.shift_selection = false;
        let buffer = self.current_mut();
        if previous == Some(Command::SetMarkCommand) && buffer.mark_active() {
            buffer.deactivate_mark();
            self.show_message("Mark deactivated");
        } else {
            buffer.set_mark(buffer.point());
            self.show_message("Mark set");
        }
    }

    pub(in crate::editor) fn exchange_point_and_mark(&mut self) {
        if !self.current_mut().exchange_point_and_mark() {
            self.show_message("No mark set in this buffer");
        }
    }

    pub(in crate::editor) fn mark_whole_buffer(&mut self) {
        self.shift_selection = false;
        let buffer = self.current_mut();
        buffer.set_mark(buffer.len_chars());
        buffer.set_point(0);
    }

    pub(in crate::editor) fn keyboard_quit(&mut self) {
        self.shift_selection = false;
        self.cancel_macro();
        if self.minibuffer.take().is_none() {
            self.buffers.current_mut().deactivate_mark();
        }
        self.show_message("Quit");
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{editor_with, press};

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
    fn shifted_arrows_select() {
        let mut editor = editor_with("hello world", 0);

        press(&mut editor, "S-<right> S-<right>");

        assert_eq!(editor.buffer().selection(), Some(0..2));
    }

    #[test]
    fn any_shifted_motion_selects() {
        for (keys, expected) in [
            ("C-S-f", 0..1),
            ("M-S-f", 0..5),
            ("C-S-<right>", 0..5),
            ("S-<end>", 0..11),
        ] {
            let mut editor = editor_with("hello world", 0);

            press(&mut editor, keys);

            assert_eq!(editor.buffer().selection(), Some(expected), "{keys}");
        }
    }

    #[test]
    fn unshifted_motion_ends_a_shift_selection() {
        let mut editor = editor_with("hello", 0);

        press(&mut editor, "S-<right> <right>");

        assert_eq!(editor.buffer().selection(), None);
        assert_eq!(editor.buffer().point(), 2);
    }

    #[test]
    fn unshifted_motion_extends_a_mark_selection() {
        let mut editor = editor_with("hello", 0);

        press(&mut editor, "C-SPC S-<right> <right>");

        assert_eq!(editor.buffer().selection(), Some(0..2));
    }

    #[test]
    fn shift_selection_starts_at_point() {
        let mut editor = editor_with("hello", 2);

        press(&mut editor, "C-SPC C-SPC C-f S-<right>");

        assert_eq!(editor.buffer().selection(), Some(3..4));
    }

    #[test]
    fn shifted_keys_without_a_motion_stay_undefined() {
        let mut editor = editor_with("hello", 0);

        press(&mut editor, "S-<f5>");

        assert_eq!(editor.message(), Some("S-<f5> is undefined"));
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
