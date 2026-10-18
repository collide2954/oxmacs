use std::path::{self, Path, PathBuf};

use crate::display::Viewport;
use crate::editor::minibuffer::{Listing, Purpose};
use crate::editor::{Choice, Editor};
use crate::files::{self, paths};
use crate::text::{Buffer, FileInfo};

impl Editor {
    pub fn set_working_directory(&mut self, directory: PathBuf) {
        self.working_directory = directory;
    }

    pub fn set_home_directory(&mut self, home: Option<PathBuf>) {
        self.home = home;
    }

    pub(super) fn find_file(&mut self) {
        if !self.open_minibuffer("Find file: ", Purpose::FindFile) {
            return;
        }
        let mut start = paths::abbreviate(&self.default_directory(), self.home.as_deref());
        if !start.ends_with(paths::is_separator) {
            start.push(paths::SEPARATOR);
        }
        self.set_minibuffer_input(&start);
        self.refresh_minibuffer();
    }

    pub(super) fn refresh_minibuffer(&mut self) {
        let Some(minibuffer) = &self.minibuffer else {
            return;
        };
        if minibuffer.purpose != Purpose::FindFile {
            return;
        }
        let input = minibuffer.input.contents();
        let directory = self.resolve_path(paths::split(&input).0);
        if minibuffer
            .listing
            .as_ref()
            .is_some_and(|listing| listing.directory == directory)
        {
            return;
        }
        let entries = paths::list_dir(&directory).unwrap_or_default();
        if let Some(minibuffer) = &mut self.minibuffer {
            minibuffer.listing = Some(Listing { directory, entries });
        }
    }

    pub(super) fn exit_find_file(&mut self, exact: bool) {
        let input = self.minibuffer_input();
        let (directory, name) = paths::split(&input);
        if name.is_empty() {
            if let Some(minibuffer) = &mut self.minibuffer {
                minibuffer.note = Some("[Directory]".to_owned());
            }
            return;
        }
        if !exact && let Some((label, entry_path, is_dir)) = self.selected_entry() {
            if is_dir {
                self.set_minibuffer_input(&format!("{directory}{label}"));
            } else {
                self.minibuffer = None;
                self.open_file(&entry_path);
            }
            return;
        }
        let path = self.resolve_path(&input);
        self.minibuffer = None;
        self.open_file(&path);
    }

    pub(super) fn complete_file_name(&mut self) {
        if let Some((label, _, _)) = self.selected_entry() {
            let input = self.minibuffer_input();
            let (directory, _) = paths::split(&input);
            self.set_minibuffer_input(&format!("{directory}{label}"));
        }
    }

    pub(super) fn minibuffer_delete_backward(&mut self) {
        match self.parent_input() {
            Some(parent) => self.set_minibuffer_input(&parent),
            None => self.delete_char(false),
        }
    }

    fn parent_input(&self) -> Option<String> {
        let minibuffer = self
            .minibuffer
            .as_ref()
            .filter(|minibuffer| minibuffer.purpose == Purpose::FindFile)?;
        if minibuffer.input.point() != minibuffer.input.len_chars() {
            return None;
        }
        let input = minibuffer.input.contents();
        let trimmed = input.strip_suffix(paths::is_separator)?;
        let index = trimmed.rfind(paths::is_separator)?;
        Some(trimmed[..=index].to_owned())
    }

    fn selected_entry(&self) -> Option<(String, PathBuf, bool)> {
        let completion = self.selected_completion()?;
        let Choice::Entry(index) = completion.choice else {
            return None;
        };
        let listing = self.minibuffer.as_ref()?.listing.as_ref()?;
        let entry = listing.entries.get(index)?;
        Some((
            completion.label,
            listing.directory.join(&entry.name),
            entry.is_dir,
        ))
    }

    fn minibuffer_input(&self) -> String {
        self.minibuffer
            .as_ref()
            .map(|minibuffer| minibuffer.input.contents())
            .unwrap_or_default()
    }

    fn resolve_path(&self, input: &str) -> PathBuf {
        let path = paths::expand(input, self.home.as_deref());
        if path.is_absolute() {
            path
        } else {
            self.default_directory().join(path)
        }
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

    pub(super) fn save_buffer(&mut self) {
        let buffer = self.buffers.current();
        if buffer.file().is_none() {
            self.show_message("Buffer is not visiting a file");
        } else if !buffer.is_modified() {
            self.show_message("(No changes need to be saved)");
        } else {
            self.save_buffer_at(0);
        }
    }

    pub(super) fn save_buffer_at(&mut self, index: usize) -> bool {
        let Some(buffer) = self.buffers.get(index) else {
            return false;
        };
        let Some(file) = buffer.file() else {
            return false;
        };
        let path = file.path.clone();
        let format = file.format;
        match files::save(&path, &buffer.contents(), format) {
            Ok(disk) => {
                if let Some(buffer) = self.buffers.get_mut(index) {
                    buffer.set_file(FileInfo {
                        path: path.clone(),
                        format,
                        disk: Some(disk),
                    });
                    buffer.mark_saved();
                }
                let shown = paths::abbreviate(&path, self.home.as_deref());
                self.show_message(format!("Wrote {shown}"));
                true
            }
            Err(error) => {
                self.show_message(error.to_string());
                false
            }
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

    const SEP: char = crate::files::paths::SEPARATOR;

    fn find_file_workspace() -> (tempfile::TempDir, Editor) {
        let (dir, mut editor) = workspace();
        fs::write(dir.path().join(".hidden"), "").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("c.txt"), "sea").unwrap();
        editor.set_home_directory(None);
        press(&mut editor, "C-x C-f");
        (dir, editor)
    }

    fn input(editor: &Editor) -> String {
        editor.minibuffer().unwrap().input().contents()
    }

    fn labels(editor: &Editor) -> Vec<String> {
        editor
            .completions()
            .into_iter()
            .map(|completion| completion.label)
            .collect()
    }

    fn root(dir: &tempfile::TempDir) -> String {
        format!("{}{SEP}", dir.path().display())
    }

    #[test]
    fn find_file_starts_in_the_default_directory() {
        let (dir, editor) = find_file_workspace();

        assert_eq!(editor.minibuffer().unwrap().prompt(), "Find file: ");
        assert_eq!(input(&editor), root(&dir));
        assert_eq!(
            labels(&editor),
            [
                format!("sub{SEP}"),
                "a.txt".into(),
                "b.txt".into(),
                ".hidden".into()
            ]
        );
    }

    #[test]
    fn typing_filters_the_directory_entries() {
        let (_dir, mut editor) = find_file_workspace();

        type_text(&mut editor, "txt");

        assert_eq!(labels(&editor), ["a.txt", "b.txt"]);
    }

    #[test]
    fn return_opens_the_selected_file() {
        let (_dir, mut editor) = find_file_workspace();

        type_text(&mut editor, "b.t");
        press(&mut editor, "RET");

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.buffer().name(), "b.txt");
        assert_eq!(editor.buffer().contents(), "bee");
    }

    #[test]
    fn tab_and_return_descend_into_directories() {
        for key in ["TAB", "RET"] {
            let (dir, mut editor) = find_file_workspace();

            type_text(&mut editor, "su");
            press(&mut editor, key);

            assert_eq!(input(&editor), format!("{}sub{SEP}", root(&dir)), "{key}");
            assert_eq!(labels(&editor), ["c.txt"], "{key}");
        }
    }

    #[test]
    fn backspace_after_a_separator_goes_up_a_directory() {
        let (dir, mut editor) = find_file_workspace();
        type_text(&mut editor, "su");
        press(&mut editor, "TAB");

        press(&mut editor, "DEL");

        assert_eq!(input(&editor), root(&dir));
    }

    #[test]
    fn return_without_a_match_creates_a_new_file() {
        let (_dir, mut editor) = find_file_workspace();

        type_text(&mut editor, "new.md");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer().name(), "new.md");
        assert_eq!(editor.message(), Some("(New file)"));
    }

    #[test]
    fn meta_return_uses_the_exact_input() {
        let (_dir, mut editor) = find_file_workspace();

        type_text(&mut editor, "a");
        press(&mut editor, "M-RET");

        assert_eq!(editor.buffer().name(), "a");
    }

    #[test]
    fn return_on_a_bare_directory_keeps_the_prompt_open() {
        let (_dir, mut editor) = find_file_workspace();

        press(&mut editor, "RET");

        assert_eq!(editor.minibuffer().unwrap().note(), Some("[Directory]"));
    }

    #[test]
    fn the_prompt_abbreviates_the_home_directory() {
        let (dir, mut editor) = workspace();
        editor.set_home_directory(Some(dir.path().to_owned()));

        press(&mut editor, "C-x C-f");

        assert_eq!(input(&editor), format!("~{SEP}"));
        assert!(labels(&editor).contains(&"a.txt".to_owned()));
    }

    fn opened(dir: &tempfile::TempDir, name: &str) -> Editor {
        let mut editor = Editor::new();
        editor.set_home_directory(None);
        editor.open_file(&dir.path().join(name));
        editor
    }

    #[test]
    fn save_writes_the_buffer_to_its_file() {
        let (dir, _) = workspace();
        let mut editor = opened(&dir, "a.txt");
        type_text(&mut editor, "x");

        press(&mut editor, "C-x C-s");

        let path = dir.path().join("a.txt");
        assert_eq!(fs::read_to_string(&path).unwrap(), "xhello");
        assert!(!editor.buffer().is_modified());
        assert_eq!(
            editor.message(),
            Some(format!("Wrote {}", path.display()).as_str())
        );
    }

    #[test]
    fn saving_keeps_the_original_line_endings() {
        let (dir, _) = workspace();
        fs::write(dir.path().join("w.txt"), "one\r\n").unwrap();
        let mut editor = opened(&dir, "w.txt");
        type_text(&mut editor, "x");

        press(&mut editor, "C-x C-s");

        assert_eq!(fs::read(dir.path().join("w.txt")).unwrap(), b"xone\r\n");
    }

    #[test]
    fn saving_an_unmodified_buffer_does_nothing() {
        let (dir, _) = workspace();
        let mut editor = opened(&dir, "a.txt");

        press(&mut editor, "C-x C-s");

        assert_eq!(editor.message(), Some("(No changes need to be saved)"));
    }

    #[test]
    fn save_errors_are_reported_and_keep_the_buffer_modified() {
        let (dir, _) = workspace();
        let sub = dir.path().join("gone");
        fs::create_dir(&sub).unwrap();
        let mut editor = Editor::new();
        editor.open_file(&sub.join("x.txt"));
        type_text(&mut editor, "x");
        fs::remove_dir(&sub).unwrap();

        press(&mut editor, "C-x C-s");

        assert!(editor.message().unwrap().contains("x.txt"));
        assert!(editor.buffer().is_modified());
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
