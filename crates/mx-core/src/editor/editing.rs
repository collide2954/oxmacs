use crate::editor::Editor;
use crate::input::{Key, KeyCode};

impl Editor {
    pub(super) fn self_insert(&mut self) {
        if let Some(KeyCode::Char(c)) = self.last_key.map(Key::code) {
            self.current_mut().insert(c.encode_utf8(&mut [0; 4]));
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
}
