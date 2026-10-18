use crate::editor::Editor;
use crate::files::{self, paths};
use crate::text::FileInfo;

impl Editor {
    pub(super) fn revert_buffer(&mut self) {
        let buffer = self.buffers.current();
        let Some(file) = buffer.file() else {
            self.show_message("Buffer is not visiting a file");
            return;
        };
        if buffer.is_modified() {
            let path = file.path.clone();
            self.ask_to_revert(0, &path);
        } else {
            self.revert_at(0);
        }
    }

    pub(super) fn revert_at(&mut self, index: usize) {
        let Some(path) = self
            .buffers
            .get(index)
            .and_then(|buffer| buffer.file())
            .map(|file| file.path.clone())
        else {
            return;
        };
        let loaded = match files::load(&path) {
            Ok(loaded) if loaded.disk.is_some() => loaded,
            Ok(_) => {
                let shown = paths::abbreviate(&path, self.home.as_deref());
                self.show_message(format!("File {shown} no longer exists"));
                return;
            }
            Err(error) => {
                self.show_message(error.to_string());
                return;
            }
        };
        let Some(buffer) = self.buffers.get_mut(index) else {
            return;
        };
        buffer.replace_text(&loaded.text);
        buffer.set_file(FileInfo::new(path, loaded.format, loaded.disk));
        buffer.mark_saved();
        let message = format!("Reverted buffer {}", buffer.name());
        self.show_message(message);
        if index == 0 {
            self.viewport.reveal(self.point_line());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::command::Command;
    use crate::editor::Editor;
    use crate::editor::test_support::{press, type_text};

    fn opened(text: &str) -> (tempfile::TempDir, PathBuf, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, text).unwrap();
        let mut editor = Editor::new();
        editor.set_home_directory(None);
        editor.open_file(&path);
        (dir, path, editor)
    }

    fn question(editor: &Editor) -> Option<String> {
        editor
            .question()
            .map(|question| question.prompt().to_owned())
    }

    #[test]
    fn revert_buffer_rereads_the_file() {
        let (_dir, path, mut editor) = opened("old");
        fs::write(&path, "new text").unwrap();

        editor.execute(Command::RevertBuffer);

        assert_eq!(editor.buffer().contents(), "new text");
        assert!(!editor.buffer().is_modified());
        assert_eq!(editor.message(), Some("Reverted buffer a.txt"));
    }

    #[test]
    fn reverting_unsaved_edits_asks_first() {
        let (_dir, path, mut editor) = opened("old");
        type_text(&mut editor, "x");

        editor.execute(Command::RevertBuffer);
        assert_eq!(
            question(&editor),
            Some(format!(
                "Revert buffer from file {}? (y or n) ",
                path.display()
            ))
        );
        press(&mut editor, "n");
        assert_eq!(editor.buffer().contents(), "xold");

        editor.execute(Command::RevertBuffer);
        press(&mut editor, "y");
        assert_eq!(editor.buffer().contents(), "old");
        assert!(!editor.buffer().is_modified());
    }

    #[test]
    fn reverting_can_be_undone() {
        let (_dir, _path, mut editor) = opened("old");
        type_text(&mut editor, "x");
        editor.execute(Command::RevertBuffer);
        press(&mut editor, "y");

        press(&mut editor, "C-/");

        assert_eq!(editor.buffer().contents(), "xold");
        assert!(editor.buffer().is_modified());
    }

    #[test]
    fn a_buffer_without_a_file_cannot_be_reverted() {
        let mut editor = Editor::new();

        editor.execute(Command::RevertBuffer);

        assert_eq!(editor.message(), Some("Buffer is not visiting a file"));
    }

    #[test]
    fn a_deleted_file_is_not_reverted() {
        let (_dir, path, mut editor) = opened("old");
        fs::remove_file(&path).unwrap();

        editor.execute(Command::RevertBuffer);

        assert_eq!(editor.buffer().contents(), "old");
        assert_eq!(
            editor.message(),
            Some(format!("File {} no longer exists", path.display()).as_str())
        );
    }
}
