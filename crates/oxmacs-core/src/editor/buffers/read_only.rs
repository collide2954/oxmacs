use crate::editor::Editor;

impl Editor {
    pub(in crate::editor) fn toggle_read_only(&mut self) {
        if self.buffers.is_special(0) {
            self.show_message("This buffer stays read-only");
            return;
        }
        let buffer = self.buffers.current_mut();
        let read_only = !buffer.is_read_only();
        buffer.set_read_only(read_only);
        let state = if read_only { "enabled" } else { "disabled" };
        self.show_message(format!("Read-Only mode {state} in current buffer"));
    }

    pub(in crate::editor) fn refuse_read_only(&mut self) -> bool {
        let buffer = self.buffers.current();
        if self.minibuffer.is_some() || !buffer.is_read_only() {
            return false;
        }
        let message = format!("Buffer is read-only: {}", buffer.name());
        self.show_message(message);
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::{editor_with, press, type_text};

    fn read_only_editor() -> crate::editor::Editor {
        let mut editor = editor_with("hello", 1);
        press(&mut editor, "C-x C-q");
        editor
    }

    #[test]
    fn c_x_c_q_toggles_read_only_mode() {
        let mut editor = editor_with("hello", 1);

        press(&mut editor, "C-x C-q");
        assert!(editor.buffer().is_read_only());
        assert_eq!(
            editor.message(),
            Some("Read-Only mode enabled in current buffer")
        );
        assert!(editor.mode_line().starts_with("%% t"));

        press(&mut editor, "C-x C-q");
        assert!(!editor.buffer().is_read_only());
        assert_eq!(
            editor.message(),
            Some("Read-Only mode disabled in current buffer")
        );
    }

    #[test]
    fn editing_a_read_only_buffer_is_refused() {
        for keys in [
            "a", "RET", "C-d", "DEL", "C-k", "M-d", "M-DEL", "C-w", "C-y", "C-/", "C-?",
        ] {
            let mut editor = read_only_editor();

            press(&mut editor, keys);

            assert_eq!(editor.buffer().contents(), "hello", "{keys}");
            assert_eq!(editor.message(), Some("Buffer is read-only: t"), "{keys}");
            assert_eq!(editor.take_effects(), [], "{keys}");
        }
    }

    #[test]
    fn moving_and_copying_still_work() {
        let mut editor = read_only_editor();

        press(&mut editor, "C-f C-SPC C-f M-w");

        assert_eq!(editor.buffer().point(), 3);
        assert_eq!(editor.clipboard_history(), ["l"]);
    }

    #[test]
    fn pasted_text_is_refused() {
        let mut editor = read_only_editor();

        editor.paste(Some("x".to_owned()));
        editor.paste_primary(Some("y".to_owned()));

        assert_eq!(editor.buffer().contents(), "hello");
        assert_eq!(editor.message(), Some("Buffer is read-only: t"));
    }

    #[test]
    fn the_minibuffer_stays_editable() {
        let mut editor = read_only_editor();

        press(&mut editor, "M-x");
        type_text(&mut editor, "abc");

        assert_eq!(editor.minibuffer().unwrap().input().contents(), "abc");
    }
}
