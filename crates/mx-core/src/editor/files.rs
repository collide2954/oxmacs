use std::path::{self, Path, PathBuf};

use crate::display::Viewport;
use crate::editor::Editor;
use crate::files;
use crate::text::{Buffer, FileInfo};

impl Editor {
    pub fn set_working_directory(&mut self, directory: PathBuf) {
        self.working_directory = directory;
    }

    pub fn default_directory(&self) -> PathBuf {
        self.buffers
            .current()
            .file()
            .and_then(|file| file.path.parent())
            .map_or_else(|| self.working_directory.clone(), Path::to_path_buf)
    }

    pub fn buffer_names(&self) -> Vec<&str> {
        self.buffers.iter().map(Buffer::name).collect()
    }

    pub fn open_file(&mut self, path: &Path) {
        let joined = self.default_directory().join(path);
        let path = path::absolute(&joined).unwrap_or(joined);
        if let Some(index) = self
            .buffers
            .position(|buffer| buffer.file().is_some_and(|file| file.path == path))
        {
            self.select_buffer(index);
            return;
        }
        match files::load(&path) {
            Ok(loaded) => {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                let mut buffer = Buffer::with_text(name, &loaded.text);
                buffer.set_file(FileInfo {
                    path,
                    format: loaded.format,
                    disk: loaded.disk,
                });
                self.buffers.push_front(buffer);
                self.after_buffer_switch();
                if loaded.disk.is_none() {
                    self.show_message("(New file)");
                }
            }
            Err(error) => self.show_message(error.to_string()),
        }
    }

    fn select_buffer(&mut self, index: usize) {
        self.buffers.select(index);
        self.after_buffer_switch();
    }

    fn after_buffer_switch(&mut self) {
        self.viewport = Viewport::new(self.viewport.rows());
        self.viewport.reveal(self.point_line());
        self.goal_column = None;
        self.shift_selection = false;
        self.last_command = None;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use crate::editor::Editor;
    use crate::editor::test_support::{press, type_text};

    fn workspace() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::write(dir.path().join("b.txt"), "bee").unwrap();
        let mut editor = Editor::new();
        editor.set_working_directory(dir.path().to_owned());
        (dir, editor)
    }

    fn visited(editor: &Editor) -> Option<PathBuf> {
        editor.buffer().file().map(|file| file.path.clone())
    }

    #[test]
    fn opening_a_file_shows_it_in_a_new_buffer() {
        let (dir, mut editor) = workspace();

        editor.open_file(Path::new("a.txt"));

        assert_eq!(editor.buffer().name(), "a.txt");
        assert_eq!(editor.buffer().contents(), "hello");
        assert_eq!(visited(&editor), Some(dir.path().join("a.txt")));
        assert!(!editor.buffer().is_modified());
        assert_eq!(editor.buffer_names(), ["a.txt", "*scratch*"]);
    }

    #[test]
    fn opening_a_missing_file_starts_a_new_buffer() {
        let (dir, mut editor) = workspace();

        editor.open_file(&dir.path().join("new.txt"));

        assert_eq!(editor.buffer().contents(), "");
        assert_eq!(editor.message(), Some("(New file)"));
        assert_eq!(visited(&editor), Some(dir.path().join("new.txt")));
    }

    #[test]
    fn reopening_a_visited_file_switches_to_its_buffer() {
        let (_dir, mut editor) = workspace();
        editor.open_file(Path::new("a.txt"));
        type_text(&mut editor, "x");
        editor.open_file(Path::new("b.txt"));

        editor.open_file(Path::new("a.txt"));

        assert_eq!(editor.buffer().contents(), "xhello");
        assert_eq!(editor.buffer_names(), ["a.txt", "b.txt", "*scratch*"]);
    }

    #[test]
    fn opening_errors_are_reported_without_a_new_buffer() {
        let (dir, mut editor) = workspace();

        editor.open_file(dir.path());

        assert!(editor.message().unwrap().ends_with("is a directory"));
        assert_eq!(editor.buffer_names(), ["*scratch*"]);
    }

    #[test]
    fn the_default_directory_follows_the_visited_file() {
        let (dir, mut editor) = workspace();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        assert_eq!(editor.default_directory(), dir.path());

        editor.open_file(&sub.join("c.txt"));

        assert_eq!(editor.default_directory(), sub);
    }

    #[test]
    fn opening_resets_the_view_to_the_top() {
        let (dir, mut editor) = workspace();
        fs::write(dir.path().join("long.txt"), "x\n".repeat(200)).unwrap();
        editor.resize_window(10);
        editor.paste(Some("\n".repeat(50)));
        press(&mut editor, "M->");
        assert!(editor.viewport().top() > 0);

        editor.open_file(Path::new("long.txt"));

        assert_eq!(editor.viewport().top(), 0);
    }
}
