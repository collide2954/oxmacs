use crate::display::Viewport;
use crate::editor::minibuffer::Purpose;
use crate::editor::{Choice, Completion, Editor};
use crate::files::paths;
use crate::text::Buffer;

#[derive(Debug)]
struct Entry {
    buffer: Buffer,
    top: usize,
}

#[derive(Debug)]
pub(super) struct BufferList {
    entries: Vec<Entry>,
}

impl BufferList {
    pub(super) fn new(buffer: Buffer) -> Self {
        Self {
            entries: vec![Entry { buffer, top: 0 }],
        }
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn current(&self) -> &Buffer {
        &self.entries[0].buffer
    }

    pub(super) fn current_mut(&mut self) -> &mut Buffer {
        &mut self.entries[0].buffer
    }

    pub(super) fn top(&self) -> usize {
        self.entries[0].top
    }

    pub(super) fn set_top(&mut self, top: usize) {
        self.entries[0].top = top;
    }

    pub(super) fn get(&self, index: usize) -> Option<&Buffer> {
        self.entries.get(index).map(|entry| &entry.buffer)
    }

    pub(super) fn get_mut(&mut self, index: usize) -> Option<&mut Buffer> {
        self.entries.get_mut(index).map(|entry| &mut entry.buffer)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &Buffer> {
        self.entries.iter().map(|entry| &entry.buffer)
    }

    pub(super) fn iter_mut(&mut self) -> impl Iterator<Item = &mut Buffer> {
        self.entries.iter_mut().map(|entry| &mut entry.buffer)
    }

    pub(super) fn position(&self, predicate: impl Fn(&Buffer) -> bool) -> Option<usize> {
        self.iter().position(predicate)
    }

    pub(super) fn push_front(&mut self, buffer: Buffer) {
        self.entries.insert(0, Entry { buffer, top: 0 });
    }

    pub(super) fn select(&mut self, index: usize) {
        if index < self.entries.len() {
            let entry = self.entries.remove(index);
            self.entries.insert(0, entry);
        }
    }
}

impl Editor {
    pub(super) fn switch_to_buffer(&mut self) {
        self.open_minibuffer("Switch to buffer: ", Purpose::SwitchBuffer);
    }

    pub(super) fn buffer_completions(&self, matches: impl Fn(&str) -> bool) -> Vec<Completion> {
        let count = self.buffers.len();
        (1..=count)
            .map(|offset| offset % count)
            .filter_map(|index| {
                let buffer = self.buffers.get(index)?;
                matches(buffer.name()).then(|| Completion {
                    label: buffer.name().to_owned(),
                    annotation: buffer
                        .file()
                        .map(|file| paths::abbreviate(&file.path, self.home.as_deref())),
                    choice: Choice::Buffer(index),
                })
            })
            .collect()
    }

    pub(super) fn switch_to_buffer_named(&mut self, name: &str) {
        match self.buffers.position(|buffer| buffer.name() == name) {
            Some(index) => self.select_buffer(index),
            None => self.add_buffer(Buffer::new(name)),
        }
    }

    pub(super) fn select_buffer(&mut self, index: usize) {
        self.buffers.set_top(self.viewport.top());
        self.buffers.select(index);
        self.after_buffer_switch();
    }

    pub(super) fn add_buffer(&mut self, buffer: Buffer) {
        self.buffers.set_top(self.viewport.top());
        self.buffers.push_front(buffer);
        self.name_file_buffers();
        self.after_buffer_switch();
    }

    fn after_buffer_switch(&mut self) {
        self.viewport = Viewport::new(self.viewport.rows());
        self.viewport.set_top(self.buffers.top());
        self.viewport.reveal(self.point_line());
        self.goal_column = None;
        self.shift_selection = false;
        self.last_command = None;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::editor::Editor;
    use crate::editor::test_support::{press, type_text};
    use crate::text::Buffer;

    fn editor_with_buffers(names: &[&str]) -> Editor {
        let (last, rest) = names.split_last().unwrap();
        let mut editor = Editor::with_buffer(Buffer::new(*last));
        for name in rest.iter().rev() {
            editor.buffers.push_front(Buffer::new(*name));
        }
        editor
    }

    fn labels(editor: &Editor) -> Vec<String> {
        editor
            .completions()
            .into_iter()
            .map(|completion| completion.label)
            .collect()
    }

    #[test]
    fn switching_lists_other_buffers_by_recency_then_the_current_one() {
        let mut editor = editor_with_buffers(&["c", "b", "a"]);

        press(&mut editor, "C-x b");

        assert_eq!(editor.minibuffer().unwrap().prompt(), "Switch to buffer: ");
        assert_eq!(labels(&editor), ["b", "a", "c"]);
    }

    #[test]
    fn return_switches_to_the_most_recent_other_buffer() {
        let mut editor = editor_with_buffers(&["c", "b", "a"]);

        press(&mut editor, "C-x b RET");
        assert_eq!(editor.buffer_names(), ["b", "c", "a"]);

        press(&mut editor, "C-x b RET");
        assert_eq!(editor.buffer_names(), ["c", "b", "a"]);
    }

    #[test]
    fn typing_filters_buffer_names() {
        let mut editor = editor_with_buffers(&["c", "b", "a"]);

        press(&mut editor, "C-x b");
        type_text(&mut editor, "a");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer().name(), "a");
    }

    #[test]
    fn an_unknown_name_creates_an_empty_buffer() {
        let mut editor = editor_with_buffers(&["c", "b"]);

        press(&mut editor, "C-x b");
        type_text(&mut editor, "notes");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer_names(), ["notes", "c", "b"]);
        assert_eq!(editor.buffer().contents(), "");
        assert_eq!(editor.buffer().file(), None);
    }

    #[test]
    fn meta_return_uses_the_exact_name() {
        let mut editor = editor_with_buffers(&["c", "ab"]);

        press(&mut editor, "C-x b");
        type_text(&mut editor, "a");
        press(&mut editor, "M-RET");
        assert_eq!(editor.buffer_names(), ["a", "c", "ab"]);

        press(&mut editor, "C-x b");
        type_text(&mut editor, "c");
        press(&mut editor, "M-RET");
        assert_eq!(editor.buffer_names(), ["c", "a", "ab"]);
    }

    #[test]
    fn file_buffers_are_annotated_with_their_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, "").unwrap();
        let mut editor = Editor::new();
        editor.set_home_directory(None);
        editor.open_file(&path);

        press(&mut editor, "C-x b");

        let annotations: Vec<Option<String>> = editor
            .completions()
            .into_iter()
            .map(|completion| completion.annotation)
            .collect();
        assert_eq!(annotations, [None, Some(path.display().to_string())]);
    }

    #[test]
    fn switching_back_restores_the_view() {
        let mut editor = editor_with_buffers(&["a", "b"]);
        editor.resize_window(10);
        editor.paste(Some("x\n".repeat(100)));
        press(&mut editor, "C-p C-p C-p");
        let top = editor.viewport().top();

        press(&mut editor, "C-x b RET");
        assert_eq!(editor.viewport().top(), 0);
        press(&mut editor, "C-x b RET");

        assert_eq!(editor.viewport().top(), top);
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 97);
    }
}
