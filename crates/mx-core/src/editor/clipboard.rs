use std::mem;

use crate::editor::Editor;

const HISTORY_LIMIT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    WriteClipboard(String),
    ReadClipboard,
}

impl Editor {
    pub fn take_effects(&mut self) -> Vec<Effect> {
        mem::take(&mut self.effects)
    }

    pub fn clipboard_history(&self) -> &[String] {
        &self.clipboard_history
    }

    pub(super) fn copy(&mut self, cut: bool) {
        self.shift_selection = false;
        let buffer = self.current_mut();
        let range = buffer.selection().unwrap_or_else(|| {
            let line = buffer.line_of(buffer.point());
            let end = (buffer.line_end(line) + 1).min(buffer.len_chars());
            buffer.line_start(line)..end
        });
        let whole_line = !buffer.mark_active();
        let text = buffer.text_in(range.clone());
        buffer.deactivate_mark();
        if text.is_empty() {
            return;
        }
        if cut {
            buffer.delete_range(range);
        }
        self.line_copy = whole_line.then(|| text.clone());
        self.remember(text.clone());
        self.effects.push(Effect::WriteClipboard(text));
    }

    pub(super) fn request_paste(&mut self) {
        self.effects.push(Effect::ReadClipboard);
    }

    pub fn paste(&mut self, clipboard: Option<String>) {
        let Some(text) = clipboard
            .filter(|text| !text.is_empty())
            .or_else(|| self.clipboard_history.first().cloned())
        else {
            self.show_message("Clipboard is empty");
            return;
        };
        self.shift_selection = false;
        let as_line = self.line_copy.as_ref() == Some(&text);
        let buffer = self.current_mut();
        let replacing = buffer.selection();
        buffer.deactivate_mark();
        match replacing {
            Some(range) => {
                buffer.delete_range(range);
                buffer.insert(&text);
            }
            None if as_line => {
                let point = buffer.point();
                buffer.set_point(buffer.line_start(buffer.line_of(point)));
                buffer.insert(&text);
                buffer.set_point(point + text.chars().count());
            }
            None => buffer.insert(&text),
        }
        self.remember(text);
    }

    fn remember(&mut self, text: String) {
        self.clipboard_history.retain(|old| *old != text);
        self.clipboard_history.insert(0, text);
        self.clipboard_history.truncate(HISTORY_LIMIT);
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::{editor_with, press};
    use crate::editor::{Editor, Effect};

    fn clipboard_writes(editor: &mut Editor) -> Vec<String> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::WriteClipboard(text) => Some(text),
                Effect::ReadClipboard => None,
            })
            .collect()
    }

    #[test]
    fn copy_writes_the_selection_to_the_clipboard() {
        for keys in ["C-SPC M-f M-w", "C-SPC M-f s-c"] {
            let mut editor = editor_with("hello world", 0);

            press(&mut editor, keys);

            assert_eq!(clipboard_writes(&mut editor), ["hello"], "{keys}");
            assert_eq!(editor.buffer().contents(), "hello world");
            assert_eq!(editor.buffer().selection(), None);
        }
    }

    #[test]
    fn copy_without_a_selection_copies_the_whole_line() {
        let mut editor = editor_with("one\ntwo\n", 5);

        press(&mut editor, "M-w");

        assert_eq!(clipboard_writes(&mut editor), ["two\n"]);
    }

    #[test]
    fn cut_removes_the_selection() {
        for keys in ["C-SPC M-f C-w", "C-SPC M-f s-x"] {
            let mut editor = editor_with("hello world", 0);

            press(&mut editor, keys);

            assert_eq!(clipboard_writes(&mut editor), ["hello"], "{keys}");
            assert_eq!(editor.buffer().contents(), " world");
        }
    }

    #[test]
    fn cut_without_a_selection_cuts_the_whole_line() {
        let mut editor = editor_with("one\ntwo\nthree", 5);

        press(&mut editor, "C-w");

        assert_eq!(clipboard_writes(&mut editor), ["two\n"]);
        assert_eq!(editor.buffer().contents(), "one\nthree");
        assert_eq!(editor.buffer().point(), 4);
    }

    #[test]
    fn copying_nothing_leaves_the_clipboard_alone() {
        let mut editor = editor_with("", 0);

        press(&mut editor, "M-w");

        assert_eq!(clipboard_writes(&mut editor), Vec::<String>::new());
    }

    #[test]
    fn paste_requests_the_clipboard() {
        for keys in ["C-y", "s-v"] {
            let mut editor = editor_with("", 0);

            press(&mut editor, keys);

            assert_eq!(editor.take_effects(), [Effect::ReadClipboard], "{keys}");
        }
    }

    #[test]
    fn pasted_text_is_inserted_at_point() {
        let mut editor = editor_with("ac", 1);

        editor.paste(Some("b".to_owned()));

        assert_eq!(editor.buffer().contents(), "abc");
        assert_eq!(editor.buffer().point(), 2);
    }

    #[test]
    fn paste_replaces_the_selection() {
        let mut editor = editor_with("hello world", 0);
        press(&mut editor, "C-SPC M-f");

        editor.paste(Some("bye".to_owned()));

        assert_eq!(editor.buffer().contents(), "bye world");
        assert_eq!(editor.buffer().selection(), None);
    }

    #[test]
    fn whole_line_copies_paste_above_the_current_line() {
        let mut editor = editor_with("one\ntwo\n", 5);
        press(&mut editor, "M-w C-p");

        editor.paste(Some("two\n".to_owned()));

        assert_eq!(editor.buffer().contents(), "two\none\ntwo\n");
        assert_eq!(editor.buffer().point(), 5);
    }

    #[test]
    fn a_missing_clipboard_falls_back_to_history() {
        let mut editor = editor_with("abc", 0);
        press(&mut editor, "C-SPC C-f M-w C-e");

        editor.paste(None);

        assert_eq!(editor.buffer().contents(), "abca");
    }

    #[test]
    fn an_empty_clipboard_and_history_is_reported() {
        let mut editor = Editor::new();

        editor.paste(Some(String::new()));

        assert_eq!(editor.message(), Some("Clipboard is empty"));
    }

    #[test]
    fn history_keeps_recent_unique_copies_first() {
        let mut editor = editor_with("a b", 0);

        press(
            &mut editor,
            "C-SPC C-f M-w C-e C-SPC C-b M-w M-< C-SPC C-f M-w",
        );

        assert_eq!(editor.clipboard_history(), ["a", "b"]);
    }

    #[test]
    fn history_is_capped() {
        let text: String = (0..60).map(|n| n.to_string() + "\n").collect();
        let mut editor = editor_with(&text, 0);

        for _ in 0..60 {
            press(&mut editor, "M-w C-n");
        }

        assert_eq!(editor.clipboard_history().len(), 50);
        assert_eq!(editor.clipboard_history()[0], "59\n");
    }
}
