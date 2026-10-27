use crate::editor::Editor;
use crate::text::BufferId;

impl Editor {
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
    use crate::editor::{Editor, Event, EventKind};
    use crate::special_buffer::SpecialBuffer;
    use crate::testing::{editor_with, press};
    use crate::text::Splice;

    #[derive(Debug)]
    struct Listing;

    impl SpecialBuffer for Listing {
        fn scope(&self) -> ScopeId {
            Scope::Global.into()
        }
    }

    #[test]
    fn edits_to_text_buffers_wait_until_they_are_taken() {
        let mut editor = editor_with("abc", 0);
        editor.subscribe(EventKind::Edited);
        let id = editor.buffer().id();

        press(&mut editor, "x C-d");

        assert_eq!(
            editor.take_events(),
            [Event::Edited(
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
        assert_eq!(editor.take_events(), []);
        let listing = editor.open_special_buffer("*list*", Listing);
        if let Some(buffer) = editor.buffer_by_id_mut(listing) {
            buffer.insert_at(0, "plugin text");
        }
        assert_eq!(editor.take_events(), []);
    }

    #[test]
    fn opening_text_buffers_is_told_but_plugin_buffers_are_not() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("notes.txt");
        fs::write(&path, "hello").unwrap();
        let mut editor = Editor::new();
        editor.subscribe(EventKind::Opened);
        editor.subscribe(EventKind::Edited);

        editor.open_file(&path);
        let file = editor.buffer().id();
        press(&mut editor, "x");
        editor.open_special_buffer("*list*", Listing);

        let events = editor.take_events();
        assert_eq!(events[0], Event::Opened(file));
        assert!(matches!(events[1], Event::Edited(id, _) if id == file));
        assert_eq!(events.len(), 2);
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
