use crate::editor::Editor;
use crate::text::BufferId;

const KEPT: usize = 100;

impl Editor {
    pub fn remember_location(&mut self, location: (BufferId, usize)) {
        self.xref.push(location);
        let excess = self.xref.len().saturating_sub(KEPT);
        self.xref.drain(..excess);
    }

    pub fn current_location(&self) -> (BufferId, usize) {
        let buffer = self.buffers.current();
        (buffer.id(), buffer.point())
    }

    pub(in crate::editor) fn xref_go_back(&mut self) {
        while let Some((id, point)) = self.xref.pop() {
            if let Some(index) = self.buffers.position(|buffer| buffer.id() == id) {
                self.select_buffer(index);
                let buffer = self.buffers.current_mut();
                buffer.set_point(point.min(buffer.len_chars()));
                return;
            }
        }
        self.show_message("No earlier location");
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::{editor_with, press};

    #[test]
    fn going_back_returns_to_remembered_places_in_order() {
        let mut editor = editor_with("one\ntwo\nthree\n", 0);
        let first = editor.current_location();
        press(&mut editor, "C-n");
        editor.remember_location(first);
        let second = editor.current_location();
        press(&mut editor, "C-n");
        editor.remember_location(second);

        press(&mut editor, "M-,");
        assert_eq!(editor.buffer().point(), 4);
        press(&mut editor, "M-,");
        assert_eq!(editor.buffer().point(), 0);
        press(&mut editor, "M-,");
        assert_eq!(editor.message(), Some("No earlier location"));
    }
}
