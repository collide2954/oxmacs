use crate::editor::Editor;
use crate::syntax::Language;

impl Editor {
    fn auto_saved(&self) -> Vec<usize> {
        if !self.settings.auto_save {
            return Vec::new();
        }
        (0..self.buffers.len())
            .filter(|index| self.buffers.language(*index) == Some(Language::Rust))
            .filter(|index| {
                self.buffers.get(*index).is_some_and(|buffer| {
                    buffer.is_modified()
                        && buffer
                            .file()
                            .is_some_and(|file| !Self::differs_on_disk(file))
                })
            })
            .collect()
    }

    pub fn wants_auto_save(&self) -> bool {
        !self.auto_saved().is_empty()
    }

    pub fn auto_save(&mut self) {
        let message = self.message.clone();
        for index in self.auto_saved() {
            if self.write_buffer_at(index, false) {
                self.message.clone_from(&message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::config::Config;
    use crate::editor::Editor;
    use crate::editor::lint::fixtures::{project, spawned};
    use crate::editor::test_support::type_text;

    fn edited(editor: &mut Editor) {
        type_text(editor, "// ");
    }

    fn on_disk(dir: &tempfile::TempDir) -> String {
        fs::read_to_string(dir.path().join("src").join("main.rs")).unwrap()
    }

    #[test]
    fn idle_rust_buffers_are_saved_quietly_and_linted() {
        let (dir, mut editor) = project();
        edited(&mut editor);
        assert!(editor.wants_auto_save());

        editor.auto_save();

        assert_eq!(on_disk(&dir), "// fn main() {}\n");
        assert!(!editor.buffer().is_modified());
        assert_eq!(editor.message(), None);
        assert_eq!(spawned(&mut editor).len(), 1);
        assert!(!editor.wants_auto_save());
    }

    #[test]
    fn other_buffers_are_left_alone() {
        let (dir, mut editor) = project();
        let notes = dir.path().join("notes.txt");
        fs::write(&notes, "").unwrap();
        editor.open_file(&notes);
        edited(&mut editor);

        editor.auto_save();

        assert!(editor.buffer().is_modified());
        assert!(!editor.wants_auto_save());
    }

    #[test]
    fn auto_save_can_be_turned_off() {
        let (dir, mut editor) = project();
        editor.apply_config(&Config::parse("auto-save = false").unwrap());
        edited(&mut editor);

        editor.auto_save();

        assert_eq!(on_disk(&dir), "fn main() {}\n");
    }

    #[test]
    fn files_changed_on_disk_are_not_overwritten() {
        let (dir, mut editor) = project();
        edited(&mut editor);
        fs::write(
            dir.path().join("src").join("main.rs"),
            "changed elsewhere\n",
        )
        .unwrap();

        editor.auto_save();

        assert_eq!(on_disk(&dir), "changed elsewhere\n");
        assert!(editor.buffer().is_modified());
    }

    #[test]
    fn auto_save_does_not_format() {
        let (dir, mut editor) = project();
        editor.apply_config(&Config::parse("format-on-save = true").unwrap());
        editor.formatter = |text, _, _| Ok(text.to_uppercase());
        edited(&mut editor);

        editor.auto_save();

        assert_eq!(on_disk(&dir), "// fn main() {}\n");
    }
}
