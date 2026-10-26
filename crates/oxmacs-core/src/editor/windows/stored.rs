use crate::editor::Editor;
use crate::editor::windows::Parked;
use crate::editor::windows::tiling::{Tiling, WindowId};

#[derive(Debug)]
pub(super) struct Stored {
    tiling: Tiling,
    selected: WindowId,
    windows: Vec<(WindowId, Parked)>,
}

impl Editor {
    pub fn store_windows(&mut self, key: &str) {
        self.forget_windows(key);
        let tiled = self.windows.tiling.windows();
        let mut windows = vec![(self.windows.selected, self.park())];
        let parked: Vec<(WindowId, Parked)> = self
            .windows
            .parked
            .iter()
            .filter(|(id, _)| tiled.contains(id))
            .copied()
            .collect();
        for (id, parked) in parked {
            if let Some(copy) = self.copy_parked(parked) {
                windows.push((id, copy));
            }
        }
        self.windows.stored.push((
            key.to_owned(),
            Stored {
                tiling: self.windows.tiling.clone(),
                selected: self.windows.selected,
                windows,
            },
        ));
    }

    pub fn recall_windows(&mut self, key: &str) -> bool {
        let Some(at) = self
            .windows
            .stored
            .iter()
            .position(|(known, _)| known == key)
        else {
            return false;
        };
        let (_, stored) = self.windows.stored.remove(at);
        let tiled = self.windows.tiling.windows();
        let live = self.windows.selected;
        if !tiled.contains(&live) {
            let parked = self.park();
            self.windows.parked.push((live, parked));
        }
        let (released, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.windows.parked)
            .into_iter()
            .partition(|(id, _)| tiled.contains(id));
        self.windows.parked = kept;
        for (_, parked) in released {
            self.release_parked(parked);
        }
        let mut selected = None;
        for (id, parked) in stored.windows {
            if id == stored.selected {
                selected = Some(parked);
            } else {
                self.windows.parked.push((id, parked));
            }
        }
        self.windows.tiling = stored.tiling;
        self.windows.selected = stored.selected;
        self.rehome_windows();
        if let Some(parked) = selected.filter(|parked| self.buffer_by_id(parked.buffer).is_some()) {
            self.show(parked);
        } else {
            self.reveal_point();
        }
        true
    }

    pub fn forget_windows(&mut self, key: &str) {
        let Some(at) = self
            .windows
            .stored
            .iter()
            .position(|(known, _)| known == key)
        else {
            return;
        };
        let (_, stored) = self.windows.stored.remove(at);
        for (_, parked) in stored.windows {
            self.release_parked(parked);
        }
    }

    fn copy_parked(&mut self, parked: Parked) -> Option<Parked> {
        let index = self
            .buffers
            .position(|buffer| buffer.id() == parked.buffer)?;
        let buffer = self.buffers.get_mut(index)?;
        let point = buffer.marker(parked.point)?;
        Some(Parked {
            point: buffer.add_marker(point),
            ..parked
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::testing::{editor_with, press, type_text};

    #[test]
    fn stored_windows_come_back_with_their_buffers_points_and_selection() {
        let mut editor = editor_with("abc\ndef", 0);
        press(&mut editor, "C-x 3 C-n C-x o");
        let right = editor.selected_window();
        editor.store_windows("one");
        press(&mut editor, "C-x 1 C-x b");
        type_text(&mut editor, "other");
        press(&mut editor, "RET");

        assert!(editor.recall_windows("one"));

        assert_eq!(editor.tiling().windows().len(), 2);
        assert_eq!(editor.selected_window(), right);
        assert_eq!(editor.buffer().name(), "t");
        assert_eq!(editor.buffer().point(), 0);
        let left = editor.tiling().windows()[0];
        assert_eq!(editor.pane_of(left).unwrap().point(), 4);
        assert!(!editor.recall_windows("one"));
    }

    #[test]
    fn stored_points_follow_edits_made_meanwhile() {
        let mut editor = editor_with("abc", 3);
        editor.store_windows("one");
        press(&mut editor, "M-<");
        type_text(&mut editor, "xx");

        assert!(editor.recall_windows("one"));

        assert_eq!(editor.buffer().point(), 5);
    }

    #[test]
    fn windows_of_killed_buffers_come_back_showing_another_buffer() {
        let mut editor = Editor::new();
        press(&mut editor, "C-x b");
        type_text(&mut editor, "doomed");
        press(&mut editor, "RET C-x 2");
        editor.store_windows("one");
        press(&mut editor, "C-x 1 C-x k RET");

        assert!(editor.recall_windows("one"));

        for window in editor.tiling().windows() {
            assert_eq!(editor.pane_of(window).unwrap().buffer().name(), "*scratch*");
        }
    }

    #[test]
    fn storing_again_replaces_and_forgetting_drops() {
        let mut editor = editor_with("abc", 1);
        editor.store_windows("one");
        press(&mut editor, "C-e");
        editor.store_windows("one");
        press(&mut editor, "C-a");

        assert!(editor.recall_windows("one"));
        assert_eq!(editor.buffer().point(), 3);

        editor.store_windows("two");
        editor.forget_windows("two");
        assert!(!editor.recall_windows("two"));
    }
}
