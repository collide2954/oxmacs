use crate::display;
use crate::editor::Editor;
use crate::text::motion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub line: usize,
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
        let buffer = &mut self.buffer;
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
    }

    pub fn drag(&mut self, cell: Cell) {
        let Some(anchor) = self.drag_anchor else {
            return;
        };
        let position = self.position_at(cell);
        let buffer = &mut self.buffer;
        if !buffer.mark_active() {
            buffer.set_mark(anchor);
        }
        buffer.set_point(position);
    }

    fn position_at(&self, cell: Cell) -> usize {
        let line = cell.line.min(self.buffer.len_lines() - 1);
        let text = self.buffer.line_text(line);
        self.buffer.line_start(line) + display::char_at_column(&text, cell.column, self.tab_width)
    }

    fn end_command_chain(&mut self) {
        self.last_command = None;
        self.goal_column = None;
        self.shift_selection = false;
        self.buffer.close_undo_step();
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::{editor_with, press, type_text};
    use crate::editor::{Cell, Clicks, Editor};

    fn cell(line: usize, column: usize) -> Cell {
        Cell { line, column }
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
