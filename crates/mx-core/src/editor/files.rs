use std::path::{self, Path, PathBuf};

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
        self.open_file_prompt("Find file: ", Purpose::FindFile);
    }

    pub(super) fn write_file(&mut self) {
        self.open_file_prompt("Write file: ", Purpose::WriteFile);
    }

    fn open_file_prompt(&mut self, prompt: &str, purpose: Purpose) {
        if !self.open_minibuffer(prompt, purpose) {
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
        if !minibuffer.purpose.takes_path() {
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

    pub(super) fn exit_file_prompt(&mut self, exact: bool) {
        let Some(purpose) = self
            .minibuffer
            .as_ref()
            .map(|minibuffer| minibuffer.purpose)
        else {
            return;
        };
        let input = self.minibuffer_input();
        let (directory, name) = paths::split(&input);
        if name.is_empty() {
            if let Some(minibuffer) = &mut self.minibuffer {
                minibuffer.note = Some("[Directory]".to_owned());
            }
            return;
        }
        let path = match self.selected_entry().filter(|_| !exact) {
            Some((label, _, true)) => {
                self.set_minibuffer_input(&format!("{directory}{label}"));
                return;
            }
            Some((_, entry_path, false)) => entry_path,
            None => self.resolve_path(&input),
        };
        self.minibuffer = None;
        if purpose == Purpose::WriteFile {
            self.write_file_to(path);
        } else {
            self.open_file(&path);
        }
    }

    fn write_file_to(&mut self, path: PathBuf) {
        let visiting = self
            .buffers
            .current()
            .file()
            .is_some_and(|file| file.path == path);
        if path.exists() && !visiting {
            self.ask_to_overwrite(path);
        } else {
            self.finish_write(path);
        }
    }

    pub(super) fn finish_write(&mut self, path: PathBuf) {
        let buffer = self.buffers.current_mut();
        let format = buffer.file().map(|file| file.format).unwrap_or_default();
        buffer.set_file(FileInfo {
            path,
            format,
            disk: None,
        });
        self.name_file_buffers();
        self.save_buffer_at(0);
    }

    pub(super) fn name_file_buffers(&mut self) {
        let paths: Vec<PathBuf> = self
            .buffers
            .iter()
            .filter_map(|buffer| buffer.file().map(|file| file.path.clone()))
            .collect();
        let mut names = paths::unique_names(&paths).into_iter();
        for buffer in self.buffers.iter_mut() {
            if buffer.file().is_some()
                && let Some(name) = names.next()
            {
                buffer.set_name(name);
            }
        }
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
            .filter(|minibuffer| minibuffer.purpose.takes_path())?;
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
                let mut buffer = Buffer::with_text(path.display().to_string(), &loaded.text);
                buffer.set_file(FileInfo {
                    path,
                    format: loaded.format,
                    disk: loaded.disk,
                });
                self.add_buffer(buffer);
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
            self.write_file();
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
    fn write_file_saves_under_a_new_name() {
        let (dir, _) = workspace();
        let mut editor = opened(&dir, "a.txt");

        press(&mut editor, "C-x C-w");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Write file: ");
        type_text(&mut editor, "c.txt");
        press(&mut editor, "RET");

        assert_eq!(
            fs::read_to_string(dir.path().join("c.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "hello"
        );
        assert_eq!(editor.buffer().name(), "c.txt");
        assert_eq!(visited(&editor), Some(dir.path().join("c.txt")));
    }

    #[test]
    fn saving_a_buffer_without_a_file_asks_where_to_write_it() {
        let (dir, mut editor) = workspace();
        editor.set_home_directory(None);
        type_text(&mut editor, "hi");

        press(&mut editor, "C-x C-s");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Write file: ");
        type_text(&mut editor, "new.txt");
        press(&mut editor, "RET");

        assert_eq!(
            fs::read_to_string(dir.path().join("new.txt")).unwrap(),
            "hi"
        );
        assert_eq!(editor.buffer().name(), "new.txt");
        assert!(!editor.buffer().is_modified());
    }

    #[test]
    fn write_file_asks_before_overwriting_another_file() {
        let (dir, _) = workspace();
        let mut editor = opened(&dir, "a.txt");
        let target = dir.path().join("b.txt");

        press(&mut editor, "C-x C-w");
        type_text(&mut editor, "b.txt");
        press(&mut editor, "RET");
        assert_eq!(
            editor
                .question()
                .map(|question| question.prompt().to_owned()),
            Some(format!(
                "File {} exists; overwrite? (y or n) ",
                target.display()
            ))
        );

        press(&mut editor, "n");
        assert_eq!(fs::read_to_string(&target).unwrap(), "bee");

        press(&mut editor, "C-x C-w");
        type_text(&mut editor, "b.txt");
        press(&mut editor, "RET y");
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn writing_to_the_visited_file_does_not_ask() {
        let (dir, _) = workspace();
        let mut editor = opened(&dir, "a.txt");
        type_text(&mut editor, "x");

        press(&mut editor, "C-x C-w");
        type_text(&mut editor, "a.txt");
        press(&mut editor, "RET");

        assert_eq!(editor.question(), None);
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "xhello"
        );
    }

    fn twin_mains(dir: &tempfile::TempDir) -> [PathBuf; 2] {
        ["x", "y"].map(|sub| {
            let path = dir.path().join(sub).join("main.rs");
            fs::create_dir(dir.path().join(sub)).unwrap();
            fs::write(&path, "").unwrap();
            path
        })
    }

    #[test]
    fn files_sharing_a_name_show_their_directory() {
        let (dir, mut editor) = workspace();
        let [x, y] = twin_mains(&dir);

        editor.open_file(&x);
        assert_eq!(editor.buffer().name(), "main.rs");
        editor.open_file(&y);

        assert_eq!(
            editor.buffer_names(),
            ["main.rs<y>", "main.rs<x>", "*scratch*"]
        );
    }

    #[test]
    fn writing_to_a_shared_name_shows_the_directory() {
        let (dir, _) = workspace();
        let [x, _] = twin_mains(&dir);
        let mut editor = opened(&dir, "a.txt");
        editor.open_file(&x);
        editor.open_file(&dir.path().join("a.txt"));

        press(&mut editor, "C-x C-w");
        type_text(&mut editor, &format!("y{SEP}main.rs"));
        press(&mut editor, "M-RET y");

        assert_eq!(
            editor.buffer_names(),
            ["main.rs<y>", "main.rs<x>", "*scratch*"]
        );
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
