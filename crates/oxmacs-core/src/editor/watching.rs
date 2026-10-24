use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::text::{BufferId, Splice};

impl Editor {
    pub fn on_buffer_opened(&mut self, handler: fn(&mut Editor, BufferId)) {
        self.opened_handlers.push(handler);
    }

    pub(super) fn tell_opened(&mut self, id: BufferId) {
        for handler in self.opened_handlers.clone() {
            handler(self, id);
        }
    }

    pub fn track_edits(&mut self) {
        self.tracking_edits = true;
        for index in 0..self.buffers.len() {
            if let Some((buffer, Mode::Text(_))) = self.buffers.entry_mut(index) {
                buffer.track_splices();
            }
        }
    }

    pub(super) fn tracking_edits(&self) -> bool {
        self.tracking_edits
    }

    pub fn take_edits(&mut self) -> Vec<(BufferId, Vec<Splice>)> {
        let mut edits = Vec::new();
        for index in 0..self.buffers.len() {
            if let Some((buffer, Mode::Text(_))) = self.buffers.entry_mut(index) {
                let splices = buffer.take_splices();
                if !splices.is_empty() {
                    edits.push((buffer.id(), splices));
                }
            }
        }
        edits
    }

    pub fn hold_saves(&mut self) {
        self.holding_saves = true;
    }

    pub(super) fn held_save(&mut self, id: BufferId, format: bool) -> bool {
        if !self.holding_saves || self.releasing == Some(id) {
            return false;
        }
        if !self.held_saves.iter().any(|(held, _)| *held == id) {
            self.held_saves.push((id, format));
        }
        true
    }

    pub fn held_saves(&self) -> Vec<BufferId> {
        self.held_saves.iter().map(|(id, _)| *id).collect()
    }

    pub fn finish_held_save(&mut self, id: BufferId) -> bool {
        let Some(at) = self.held_saves.iter().position(|(held, _)| *held == id) else {
            return false;
        };
        let (_, format) = self.held_saves.remove(at);
        let Some(index) = self.buffers.position(|buffer| buffer.id() == id) else {
            return false;
        };
        self.releasing = Some(id);
        let written = self.write_buffer_at(index, format);
        self.releasing = None;
        if !written {
            self.quit_requested = false;
        }
        written
    }

    pub fn drop_held_save(&mut self, id: BufferId) {
        self.held_saves.retain(|(held, _)| *held != id);
        self.quit_requested = false;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::command::{Scope, ScopeId};
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, press};
    use crate::plugin::PluginBuffer;
    use crate::text::{BufferId, Splice};

    #[derive(Debug)]
    struct Listing;

    impl PluginBuffer for Listing {
        fn scope(&self) -> ScopeId {
            Scope::Global.into()
        }
    }

    fn note(editor: &mut Editor, id: BufferId) {
        if editor.plugin_state::<Vec<BufferId>>().is_none() {
            editor.set_plugin_state(Vec::<BufferId>::new());
        }
        if let Some(opened) = editor.plugin_state_mut::<Vec<BufferId>>() {
            opened.push(id);
        }
    }

    #[test]
    fn edits_to_text_buffers_wait_until_they_are_taken() {
        let mut editor = editor_with("abc", 0);
        editor.track_edits();
        let id = editor.buffer().id();

        press(&mut editor, "x C-d");

        assert_eq!(
            editor.take_edits(),
            [(
                id,
                vec![
                    Splice {
                        start: 0,
                        removed: 0,
                        inserted: 1
                    },
                    Splice {
                        start: 1,
                        removed: 1,
                        inserted: 0
                    }
                ]
            )]
        );
        assert_eq!(editor.take_edits(), []);
        let listing = editor.open_plugin_buffer("*list*", Listing);
        if let Some(buffer) = editor.buffer_by_id_mut(listing) {
            buffer.insert_at(0, "plugin text");
        }
        assert_eq!(editor.take_edits(), []);
    }

    #[test]
    fn opening_text_buffers_is_told_but_plugin_buffers_are_not() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("notes.txt");
        fs::write(&path, "hello").unwrap();
        let mut editor = Editor::new();
        editor.on_buffer_opened(note);
        editor.track_edits();

        editor.open_file(&path);
        let file = editor.buffer().id();
        press(&mut editor, "x");
        editor.open_plugin_buffer("*list*", Listing);

        assert_eq!(editor.plugin_state::<Vec<BufferId>>(), Some(&vec![file]));
        assert_eq!(editor.take_edits().len(), 1);
    }

    #[test]
    fn held_saves_are_written_when_the_host_lets_them_go() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("notes.txt");
        fs::write(&path, "old").unwrap();
        let mut editor = Editor::new();
        editor.open_file(&path);
        editor.hold_saves();
        let id = editor.buffer().id();

        press(&mut editor, "x C-x C-s");

        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(editor.held_saves(), [id]);
        assert!(editor.finish_held_save(id));
        assert_eq!(fs::read_to_string(&path).unwrap(), "xold");
        assert_eq!(editor.held_saves(), []);
    }

    #[test]
    fn quitting_waits_for_held_saves_and_a_dropped_save_stops_it() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("notes.txt");
        fs::write(&path, "old").unwrap();
        let mut editor = Editor::new();
        editor.open_file(&path);
        editor.hold_saves();
        let id = editor.buffer().id();

        press(&mut editor, "x C-x C-c y");
        assert!(!editor.should_quit());
        editor.drop_held_save(id);
        assert!(!editor.should_quit());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");

        press(&mut editor, "C-x C-c y");
        assert!(!editor.should_quit());
        editor.finish_held_save(id);
        assert!(editor.should_quit());
        assert_eq!(fs::read_to_string(&path).unwrap(), "xold");
    }
}
