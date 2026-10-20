use crate::command::Command;
use crate::editor::buffers::Mode;
use crate::editor::{Editor, Effect};
use crate::text::{Buffer, BufferId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewScroll {
    Lines(isize),
    Pages(isize),
    Top,
    Bottom,
}

impl Editor {
    pub(in crate::editor) fn scroll_preview(&mut self, command: Command) -> bool {
        if !matches!(self.buffers.mode(0), Some(Mode::Preview(_))) {
            return false;
        }
        let scroll = match command {
            Command::NextLine => PreviewScroll::Lines(1),
            Command::PreviousLine => PreviewScroll::Lines(-1),
            Command::ScrollUpCommand => PreviewScroll::Pages(1),
            Command::ScrollDownCommand => PreviewScroll::Pages(-1),
            Command::BeginningOfBuffer => PreviewScroll::Top,
            Command::EndOfBuffer => PreviewScroll::Bottom,
            _ => return false,
        };
        self.effects
            .push(Effect::ScrollPreview(self.selected_window(), scroll));
        true
    }

    fn preview_index(&self, source: BufferId) -> Option<usize> {
        (0..self.buffers.len()).find(
            |index| matches!(self.buffers.mode(*index), Some(Mode::Preview(id)) if *id == source),
        )
    }

    pub(in crate::editor) fn markdown_preview(&mut self) {
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
        self.beside(|editor| match editor.preview_index(source) {
            Some(index) => editor.select_buffer(index),
            None => editor.add_preview(source),
        });
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
    use crate::editor::test_support::{editor_with, press};
    use crate::editor::{Editor, Effect, PreviewScroll};
    use crate::syntax::Language;

    fn markdown() -> Editor {
        let mut editor = editor_with("# Title\n", 0);
        editor.set_major_mode(Some(Language::MARKDOWN));
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

    fn previewed(text: &str) -> Editor {
        let mut editor = editor_with(text, 0);
        editor.set_major_mode(Some(Language::MARKDOWN));
        press(&mut editor, "C-c C-c p");
        editor.take_effects();
        editor
    }

    #[test]
    fn keys_in_the_preview_scroll_it() {
        let mut editor = previewed("# Title\n\ntext\n");
        press(&mut editor, "C-x o");
        let preview = editor.selected_window();
        editor.take_effects();

        for (keys, scroll) in [
            ("C-n", PreviewScroll::Lines(1)),
            ("<up>", PreviewScroll::Lines(-1)),
            ("C-v", PreviewScroll::Pages(1)),
            ("M-v", PreviewScroll::Pages(-1)),
            ("SPC", PreviewScroll::Pages(1)),
            ("DEL", PreviewScroll::Pages(-1)),
            ("M->", PreviewScroll::Bottom),
            ("M-<", PreviewScroll::Top),
        ] {
            press(&mut editor, keys);
            assert_eq!(
                editor.take_effects(),
                [Effect::ScrollPreview(preview, scroll)],
                "{keys}"
            );
        }
    }

    #[test]
    fn q_closes_the_preview_window_view() {
        let mut editor = previewed("# A\n");
        press(&mut editor, "C-x o");

        press(&mut editor, "q");

        assert_ne!(editor.buffer().name(), "*Preview: t*");
    }

    #[test]
    fn an_existing_split_hosts_the_preview() {
        let mut editor = markdown();

        press(&mut editor, "C-x 3 C-c C-c p");

        assert_eq!(editor.tiling().windows().len(), 2);
    }
}
