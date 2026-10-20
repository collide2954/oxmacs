use std::path::PathBuf;

use crate::config::OnDiskChange;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::files::{self, DiskState, paths};
use crate::text::{Buffer, FileInfo};

impl Editor {
    pub fn visited_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self
            .buffers
            .iter()
            .filter_map(|buffer| buffer.file().map(|file| file.path.clone()))
            .collect();
        files.sort();
        files.dedup();
        files
    }

    pub fn watched_directories(&self) -> Vec<PathBuf> {
        let mut directories: Vec<PathBuf> = (0..self.buffers.len())
            .filter_map(|index| self.buffers.location(index))
            .filter(|(_, directory)| *directory)
            .map(|(path, _)| path.to_owned())
            .collect();
        directories.sort();
        directories.dedup();
        directories
    }

    pub fn check_disk_changes(&mut self) {
        self.refresh_changed_listings();
        self.tell_disk_change();
        if self.on_disk_change == OnDiskChange::Ignore {
            return;
        }
        for index in 0..self.buffers.len() {
            let Some(buffer) = self.buffers.get(index) else {
                continue;
            };
            let Some(file) = buffer.file() else {
                continue;
            };
            let Some(recorded) = file.disk else {
                continue;
            };
            let Ok(now) = DiskState::of(&file.path) else {
                continue;
            };
            if now == Some(recorded) {
                continue;
            }
            let name = buffer.name().to_owned();
            let modified = buffer.is_modified();
            match now {
                None => {
                    self.note_disk_state(index, None, false);
                    self.show_message(format!("{name} was deleted on disk"));
                }
                Some(_) if modified => {
                    self.note_disk_state(index, now, true);
                    self.show_message(format!("{name} changed on disk; keeping your edits"));
                }
                Some(_) if self.on_disk_change == OnDiskChange::Revert => self.revert_at(index),
                Some(_) => {
                    if self.question.is_none() {
                        self.ask_to_reread(index, &name, now);
                    }
                }
            }
        }
    }

    pub(in crate::editor) fn differs_on_disk(file: &FileInfo) -> bool {
        file.changed_on_disk
            || DiskState::of(&file.path)
                .ok()
                .flatten()
                .is_some_and(|now| Some(now) != file.disk)
    }

    pub(in crate::editor) fn note_disk_state(
        &mut self,
        index: usize,
        disk: Option<DiskState>,
        changed: bool,
    ) {
        if let Some(file) = self.buffers.get_mut(index).and_then(Buffer::file_mut) {
            file.disk = disk;
            file.changed_on_disk = changed;
        }
    }

    pub(in crate::editor) fn revert_buffer(&mut self) {
        if self.customizing().is_some() {
            self.revert_settings();
            return;
        }
        if self.in_diagnostics_list() {
            self.recheck();
            return;
        }
        if matches!(self.buffers.mode(0), Some(Mode::Environment(_))) {
            self.refresh_environment();
            return;
        }
        if matches!(self.buffers.mode(0), Some(Mode::Table(_))) {
            self.r_table_reload();
            return;
        }
        if matches!(self.buffers.mode(0), Some(Mode::Processes(_))) {
            self.render_processes();
            return;
        }
        if matches!(self.buffers.mode(0), Some(Mode::Tools(_))) {
            self.check_tool_releases();
            return;
        }
        if matches!(self.buffers.mode(0), Some(Mode::Doctor(_))) {
            self.oxmacs_doctor();
            return;
        }
        if self.buffers.listing(0).is_some() {
            self.refresh_listing(0);
            return;
        }
        if matches!(self.buffers.mode(0), Some(Mode::Applet(_))) {
            if let Some(command) = self.bound_command("g") {
                self.run_id(command, false);
            }
            return;
        }
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

    pub(in crate::editor) fn revert_at(&mut self, index: usize) {
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
            self.reveal_point();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::command::Command;
    use crate::config::Config;
    use crate::editor::Editor;
    use crate::editor::test_support::{press, question, type_text};

    fn opened(text: &str) -> (tempfile::TempDir, PathBuf, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, text).unwrap();
        let mut editor = Editor::new();
        editor.set_home_directory(None);
        editor.open_file(&path);
        (dir, path, editor)
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

    fn with_mode(editor: &mut Editor, mode: &str) {
        let config = Config::parse(&format!("on-disk-change = \"{mode}\"")).unwrap();
        editor.apply_config(&config);
    }

    #[test]
    fn unmodified_buffers_follow_disk_changes() {
        let (_dir, path, mut editor) = opened("old");
        fs::write(&path, "new text").unwrap();

        editor.check_disk_changes();

        assert_eq!(editor.buffer().contents(), "new text");
        assert!(!editor.buffer().is_modified());
        assert_eq!(editor.message(), Some("Reverted buffer a.txt"));
    }

    #[test]
    fn modified_buffers_keep_their_edits_and_warn_once() {
        let (_dir, path, mut editor) = opened("old");
        type_text(&mut editor, "x");
        fs::write(&path, "new text").unwrap();

        editor.check_disk_changes();
        assert_eq!(editor.buffer().contents(), "xold");
        assert_eq!(
            editor.message(),
            Some("a.txt changed on disk; keeping your edits")
        );

        editor.show_message("other");
        editor.check_disk_changes();
        assert_eq!(editor.message(), Some("other"));
    }

    #[test]
    fn deleted_files_are_reported_once() {
        let (_dir, path, mut editor) = opened("old");
        fs::remove_file(&path).unwrap();

        editor.check_disk_changes();
        assert_eq!(editor.buffer().contents(), "old");
        assert_eq!(editor.message(), Some("a.txt was deleted on disk"));

        editor.show_message("other");
        editor.check_disk_changes();
        assert_eq!(editor.message(), Some("other"));
    }

    #[test]
    fn ignore_leaves_buffers_alone() {
        let (_dir, path, mut editor) = opened("old");
        with_mode(&mut editor, "ignore");
        fs::write(&path, "new text").unwrap();

        editor.check_disk_changes();

        assert_eq!(editor.buffer().contents(), "old");
        assert_eq!(editor.message(), None);
    }

    #[test]
    fn ask_mode_asks_before_rereading() {
        let (_dir, path, mut editor) = opened("old");
        with_mode(&mut editor, "ask");
        fs::write(&path, "new text").unwrap();

        editor.check_disk_changes();
        assert_eq!(
            question(&editor),
            Some("a.txt changed on disk; reread it? (y or n) ".to_owned())
        );
        press(&mut editor, "y");

        assert_eq!(editor.buffer().contents(), "new text");
    }

    #[test]
    fn a_declined_reread_is_not_asked_again() {
        let (_dir, path, mut editor) = opened("old");
        with_mode(&mut editor, "ask");
        fs::write(&path, "new text").unwrap();
        editor.check_disk_changes();
        press(&mut editor, "n");

        editor.check_disk_changes();

        assert_eq!(question(&editor), None);
        assert_eq!(editor.buffer().contents(), "old");
    }

    #[test]
    fn ask_mode_asks_about_each_changed_buffer_in_turn() {
        let (dir, a, mut editor) = opened("a");
        let b = dir.path().join("b.txt");
        fs::write(&b, "b").unwrap();
        editor.open_file(&b);
        with_mode(&mut editor, "ask");
        fs::write(&a, "new a").unwrap();
        fs::write(&b, "new b").unwrap();

        editor.check_disk_changes();
        assert_eq!(
            question(&editor),
            Some("b.txt changed on disk; reread it? (y or n) ".to_owned())
        );
        press(&mut editor, "y");
        assert_eq!(
            question(&editor),
            Some("a.txt changed on disk; reread it? (y or n) ".to_owned())
        );
    }

    #[test]
    fn saving_over_a_changed_file_asks_first() {
        let (_dir, path, mut editor) = opened("old");
        type_text(&mut editor, "x");
        fs::write(&path, "new text").unwrap();

        press(&mut editor, "C-x C-s");
        assert_eq!(
            question(&editor),
            Some("a.txt changed on disk; save anyway? (y or n) ".to_owned())
        );
        press(&mut editor, "n");
        assert_eq!(fs::read_to_string(&path).unwrap(), "new text");
        assert!(editor.buffer().is_modified());

        press(&mut editor, "C-x C-s y");
        assert_eq!(fs::read_to_string(&path).unwrap(), "xold");
        assert!(!editor.buffer().is_modified());
    }

    #[test]
    fn saving_after_a_noticed_change_still_asks() {
        let (_dir, path, mut editor) = opened("old");
        type_text(&mut editor, "x");
        fs::write(&path, "new text").unwrap();
        editor.check_disk_changes();

        press(&mut editor, "C-x C-s");

        assert!(question(&editor).is_some());
    }

    #[test]
    fn saving_an_unchanged_file_does_not_ask() {
        let (_dir, path, mut editor) = opened("old");
        type_text(&mut editor, "x");

        press(&mut editor, "C-x C-s");

        assert_eq!(question(&editor), None);
        assert_eq!(fs::read_to_string(&path).unwrap(), "xold");
    }

    #[test]
    fn visited_files_are_listed_once_in_order() {
        let (dir, a, mut editor) = opened("a");
        let b = dir.path().join("b.txt");
        fs::write(&b, "b").unwrap();
        editor.open_file(&b);
        editor.open_file(&a);

        assert_eq!(editor.visited_files(), [a, b]);
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
