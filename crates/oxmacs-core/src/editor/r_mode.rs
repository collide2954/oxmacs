use crate::editor::Editor;

impl Editor {
    pub(super) fn insert_operator(&mut self, operator: &str) {
        let buffer = self.current_mut();
        let point = buffer.point();
        let spaced_before = buffer
            .chars_before(point)
            .next()
            .is_none_or(char::is_whitespace);
        let spaced_after = buffer.chars_after(point).next().is_some_and(|c| c == ' ');
        let before = if spaced_before { "" } else { " " };
        let after = if spaced_after { "" } else { " " };
        buffer.insert(&format!("{before}{operator}{after}"));
        if spaced_after {
            buffer.set_point(buffer.point() + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, press};
    use crate::syntax::Language;

    fn r(text: &str, point: usize) -> Editor {
        let mut editor = editor_with(text, point);
        editor.set_major_mode(Some(Language::R));
        editor
    }

    #[test]
    fn meta_minus_inserts_an_assignment_arrow() {
        let mut editor = r("x", 1);

        press(&mut editor, "M--");

        assert_eq!(editor.buffer().contents(), "x <- ");
        assert_eq!(editor.buffer().point(), 5);
    }

    #[test]
    fn existing_spaces_are_reused() {
        let mut editor = r("x  1", 2);

        press(&mut editor, "M--");

        assert_eq!(editor.buffer().contents(), "x <- 1");
        assert_eq!(editor.buffer().point(), 5);
    }

    #[test]
    fn control_shift_m_inserts_the_native_pipe() {
        let mut editor = r("df", 2);

        press(&mut editor, "C-S-m");

        assert_eq!(editor.buffer().contents(), "df |> ");
    }

    #[test]
    fn the_shortcuts_belong_to_r_buffers() {
        let mut editor = editor_with("x", 1);
        editor.set_major_mode(Some(Language::Rust));

        press(&mut editor, "M--");

        assert_eq!(editor.buffer().contents(), "x");
    }

    #[test]
    fn read_only_r_buffers_refuse_the_shortcuts() {
        let mut editor = r("x", 1);
        press(&mut editor, "C-x C-q");

        press(&mut editor, "M--");

        assert_eq!(editor.buffer().contents(), "x");
    }
}
