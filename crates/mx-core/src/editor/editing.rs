use crate::editor::Editor;
use crate::input::{Key, KeyCode};
use crate::text::{Buffer, motion};

impl Editor {
    pub(super) fn self_insert(&mut self) {
        if let Some(KeyCode::Char(c)) = self.last_key.map(Key::code) {
            self.insert_over_selection(c.encode_utf8(&mut [0; 4]));
            self.reindent_after(c);
        }
    }

    pub(super) fn insert_over_selection(&mut self, text: &str) {
        self.delete_selection();
        self.current_mut().insert(text);
    }

    pub(super) fn delete_char(&mut self, forward: bool) {
        if self.delete_selection() {
            return;
        }
        let buffer = self.current_mut();
        if forward {
            buffer.delete_forward(1);
        } else {
            buffer.delete_backward(1);
        }
    }

    pub(super) fn delete_to(&mut self, motion: impl FnOnce(&Buffer, usize) -> usize) {
        self.shift_selection = false;
        let buffer = self.current_mut();
        buffer.deactivate_mark();
        let point = buffer.point();
        let target = motion(buffer, point);
        buffer.delete_range(point.min(target)..point.max(target));
    }

    pub(super) fn delete_line(&mut self) {
        self.delete_to(|buffer, point| {
            let end = motion::end_of_line(buffer, point);
            if end == point {
                motion::forward_char(buffer, point, 1)
            } else {
                end
            }
        });
    }

    pub(super) fn delete_selection(&mut self) -> bool {
        self.shift_selection = false;
        let buffer = self.current_mut();
        let selection = buffer.selection();
        buffer.deactivate_mark();
        match selection {
            Some(range) => {
                buffer.delete_range(range);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{contents_after, press};

    #[test]
    fn printable_keys_insert_themselves() {
        let mut editor = Editor::new();

        press(&mut editor, "h i SPC A");

        assert_eq!(editor.buffer().contents(), "hi A");
        assert_eq!(editor.buffer().point(), 4);
    }

    #[test]
    fn return_inserts_a_newline() {
        assert_eq!(contents_after("ab", 1, "RET"), ("a\nb".to_owned(), 2));
        assert_eq!(contents_after("ab", 1, "C-m"), ("a\nb".to_owned(), 2));
        assert_eq!(contents_after("ab", 1, "C-j"), ("a\nb".to_owned(), 2));
    }

    #[test]
    fn delete_char_removes_char_after_point() {
        assert_eq!(contents_after("abc", 1, "C-d"), ("ac".to_owned(), 1));
        assert_eq!(contents_after("abc", 1, "<delete>"), ("ac".to_owned(), 1));
    }

    #[test]
    fn delete_backward_char_removes_char_before_point() {
        assert_eq!(contents_after("abc", 1, "DEL"), ("bc".to_owned(), 0));
    }

    #[test]
    fn delete_line_deletes_to_the_end_of_the_line() {
        assert_eq!(
            contents_after("one two\nthree", 3, "C-k"),
            ("one\nthree".to_owned(), 3)
        );
    }

    #[test]
    fn delete_line_at_the_end_of_a_line_joins_it_with_the_next() {
        assert_eq!(
            contents_after("one\ntwo", 3, "C-k"),
            ("onetwo".to_owned(), 3)
        );
        assert_eq!(contents_after("one", 3, "C-k"), ("one".to_owned(), 3));
    }

    #[test]
    fn delete_word_deletes_forward_to_the_end_of_the_word() {
        for keys in ["M-d", "C-<delete>"] {
            assert_eq!(
                contents_after("alpha beta", 0, keys),
                (" beta".to_owned(), 0),
                "{keys}"
            );
        }
    }

    #[test]
    fn backward_delete_word_deletes_back_to_the_start_of_the_word() {
        for keys in ["M-DEL", "C-DEL"] {
            assert_eq!(
                contents_after("alpha beta", 10, keys),
                ("alpha ".to_owned(), 6),
                "{keys}"
            );
        }
    }

    #[test]
    fn typing_replaces_the_selection() {
        assert_eq!(
            contents_after("hello world", 0, "C-SPC M-f x"),
            ("x world".to_owned(), 1)
        );
        assert_eq!(
            contents_after("hello world", 0, "C-SPC M-f RET"),
            ("\n world".to_owned(), 1)
        );
    }

    #[test]
    fn delete_keys_delete_the_selection() {
        for keys in ["C-SPC M-f DEL", "C-SPC M-f C-d", "C-SPC M-f <delete>"] {
            assert_eq!(
                contents_after("hello world", 0, keys),
                (" world".to_owned(), 0),
                "{keys}"
            );
        }
    }

    #[test]
    fn line_and_word_deletions_clear_the_selection_and_skip_the_clipboard() {
        let mut editor = crate::editor::test_support::editor_with("hello world", 0);

        press(&mut editor, "C-SPC C-f M-d");

        assert_eq!(editor.buffer().contents(), "h world");
        assert_eq!(editor.buffer().selection(), None);
        let effects = editor.take_effects();
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, crate::editor::Effect::WriteClipboard(_))),
            "{effects:?}"
        );
    }
}
