use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::tiling::Axis;
use crate::text::{Buffer, BufferId};

impl Editor {
    fn preview_index(&self, source: BufferId) -> Option<usize> {
        (0..self.buffers.len()).find(
            |index| matches!(self.buffers.mode(*index), Some(Mode::Preview(id)) if *id == source),
        )
    }

    pub(super) fn markdown_preview(&mut self) {
        let source = self.buffers.current().id();
        let origin = self.selected_window();
        if let Some(preview) = self
            .preview_index(source)
            .map(|index| self.buffers.get(index).map(Buffer::id))
            && let Some(window) = preview.and_then(|id| self.window_showing(id))
        {
            self.select_window(window);
            self.delete_window();
            self.select_window(origin);
            return;
        }
        if self.tiling().windows().len() == 1 {
            self.split_window(Axis::Columns);
        }
        self.other_window();
        if let Some(index) = self.preview_index(source) {
            self.select_buffer(index);
        } else {
            self.add_preview(source);
        }
        self.select_window(origin);
    }

    fn add_preview(&mut self, source: BufferId) {
        let name = self
            .buffers
            .position(|buffer| buffer.id() == source)
            .and_then(|index| self.buffers.get(index))
            .map_or_else(String::new, |buffer| buffer.name().to_owned());
        let mut buffer = Buffer::new(format!("*Preview: {name}*"));
        buffer.set_read_only(true);
        self.add_buffer_with(buffer, Mode::Preview(source));
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, press};
    use crate::syntax::Language;

    fn markdown() -> Editor {
        let mut editor = editor_with("# Title\n", 0);
        editor.set_major_mode(Some(Language::Markdown));
        editor
    }

    #[test]
    fn the_preview_opens_beside_its_source() {
        let mut editor = markdown();
        let origin = editor.selected_window();

        press(&mut editor, "C-c C-c p");

        assert_eq!(editor.tiling().windows().len(), 2);
        assert_eq!(editor.selected_window(), origin);
        let other = editor.tiling().windows()[1];
        let pane = editor.pane_of(other).unwrap();
        assert_eq!(pane.buffer().name(), "*Preview: t*");
        assert_eq!(pane.preview_source().unwrap().contents(), "# Title\n");
    }

    #[test]
    fn the_preview_follows_edits_to_its_source() {
        let mut editor = markdown();
        press(&mut editor, "C-c C-c p M-> x");

        let other = editor.tiling().windows()[1];
        let pane = editor.pane_of(other).unwrap();
        assert_eq!(pane.preview_source().unwrap().contents(), "# Title\nx");
    }

    #[test]
    fn previewing_again_closes_it() {
        let mut editor = markdown();
        let origin = editor.selected_window();

        press(&mut editor, "C-c C-c p C-c C-c p");

        assert_eq!(editor.tiling().windows(), [origin]);
    }

    #[test]
    fn an_existing_split_hosts_the_preview() {
        let mut editor = markdown();

        press(&mut editor, "C-x 3 C-c C-c p");

        assert_eq!(editor.tiling().windows().len(), 2);
    }
}
