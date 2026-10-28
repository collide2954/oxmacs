use std::fs;
use std::path::{Path, PathBuf};

use crate::editor::Editor;

const HEADER: &str = "oxmacs recovery copy of ";

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

impl Editor {
    pub fn set_recovery_folder(&mut self, folder: Option<PathBuf>) {
        self.recovery = folder;
    }

    fn recovery_copy(&self, path: &Path) -> Option<PathBuf> {
        let folder = self.recovery.as_ref()?;
        let name = path.file_name()?.to_string_lossy();
        let key = fnv(&path.display().to_string());
        Some(folder.join(format!("{name}.{key:016x}.recover")))
    }

    fn modified_files(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.buffers.len()).filter(|index| {
            self.buffers
                .get(*index)
                .is_some_and(|buffer| buffer.is_modified() && buffer.file().is_some())
        })
    }

    fn unrecovered(&self) -> impl Iterator<Item = usize> + '_ {
        self.modified_files()
            .filter(|_| self.config.auto_save && self.recovery.is_some())
            .filter(|index| {
                self.buffers.get(*index).is_some_and(|buffer| {
                    !self.recovered.contains(&(buffer.id(), buffer.revision()))
                })
            })
    }

    pub fn set_folders(&mut self, folders: crate::folders::Folders) {
        self.tools_root = Some(folders.tools());
        self.recovery = Some(folders.recovery());
        self.folders = Some(folders);
    }

    pub fn folders(&self) -> Option<&crate::folders::Folders> {
        self.folders.as_ref()
    }

    pub fn deletes_to_trash(&self) -> bool {
        self.config.delete_to_trash
    }

    pub fn wants_auto_save(&self) -> bool {
        (self.config.auto_save_visited && self.modified_files().next().is_some())
            || self.unrecovered().next().is_some()
    }

    pub fn auto_save(&mut self) {
        if self.config.auto_save_visited {
            self.save_visited();
        }
        let due: Vec<usize> = self.unrecovered().collect();
        for index in due {
            self.write_recovery(index);
        }
    }

    fn save_visited(&mut self) {
        let message = self.message.clone();
        let due: Vec<usize> = self
            .modified_files()
            .filter(|index| {
                self.buffers
                    .get(*index)
                    .and_then(|buffer| buffer.file())
                    .is_some_and(|file| !Self::differs_on_disk(file))
            })
            .collect();
        for index in due {
            if self.write_buffer_at(index, false) {
                self.message.clone_from(&message);
            }
        }
    }

    fn write_recovery(&mut self, index: usize) {
        let Some(buffer) = self.buffers.get(index) else {
            return;
        };
        let Some(path) = buffer.file().map(|file| file.path.clone()) else {
            return;
        };
        let Some(copy) = self.recovery_copy(&path) else {
            return;
        };
        let key = (buffer.id(), buffer.revision());
        let text = format!("{HEADER}{}\n{}", path.display(), buffer.contents());
        let written = copy
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| {
                oxmacs_files::write(&copy, text.as_bytes(), oxmacs_files::NewFile::Private)
            });
        if written.is_ok() {
            self.recovered.retain(|(id, _)| *id != key.0);
            self.recovered.push(key);
        }
    }

    pub(in crate::editor) fn forget_recovery(&mut self, path: &Path) {
        if let Some(copy) = self.recovery_copy(path) {
            let _ = fs::remove_file(copy);
        }
    }

    pub(in crate::editor) fn offer_recovery(&mut self, path: &Path) {
        let Some(copy) = self.recovery_copy(path) else {
            return;
        };
        let newer = match (
            fs::metadata(&copy).and_then(|found| found.modified()),
            fs::metadata(path).and_then(|found| found.modified()),
        ) {
            (Ok(saved), Ok(file)) => saved > file,
            (Ok(_), Err(_)) => true,
            _ => false,
        };
        let Some(text) = fs::read_to_string(&copy)
            .ok()
            .filter(|_| newer)
            .and_then(|saved| saved.split_once('\n').map(|(_, text)| text.to_owned()))
        else {
            return;
        };
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let path = path.to_path_buf();
        self.ask(
            &format!("Recover unsaved changes to {name}?"),
            move |editor, yes| {
                let found = editor.index_visiting(&path);
                if let (true, Some(index)) = (yes, found)
                    && let Some(buffer) = editor.buffers.get_mut(index)
                {
                    buffer.replace_text(&text);
                }
            },
        );
    }
}
#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::json;

    use crate::config::Config;
    use crate::editor::Editor;
    use crate::editor::build::lint::fixtures::{project, spawned};
    use crate::testing::{press, question, type_text};

    fn edited(editor: &mut Editor) {
        type_text(editor, "// ");
    }

    fn on_disk(dir: &tempfile::TempDir) -> String {
        fs::read_to_string(dir.path().join("src").join("main.rs")).unwrap()
    }

    fn copies(cache: &Path) -> Vec<String> {
        fs::read_dir(cache)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| fs::read_to_string(entry.path()).unwrap())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn recovering() -> (tempfile::TempDir, tempfile::TempDir, Editor) {
        let (dir, mut editor) = project();
        let cache = tempfile::tempdir().unwrap();
        editor.set_recovery_folder(Some(cache.path().join("recovery")));
        (dir, cache, editor)
    }

    fn visiting(editor: &mut Editor) {
        editor.apply_config(
            &Config::default()
                .with("auto-save-visited", json!(true))
                .unwrap(),
        );
    }

    #[test]
    fn idle_buffers_get_recovery_copies_in_the_cache_until_saved() {
        let (dir, cache, mut editor) = recovering();
        let recovery = cache.path().join("recovery");
        edited(&mut editor);
        assert!(editor.wants_auto_save());

        editor.auto_save();

        assert_eq!(on_disk(&dir), "fn main() {}\n");
        assert!(editor.buffer().is_modified());
        let kept = copies(&recovery);
        assert_eq!(kept.len(), 1);
        assert!(kept[0].ends_with("\n// fn main() {}\n"), "{kept:?}");
        assert!(!editor.wants_auto_save());
        assert!(
            !fs::read_dir(dir.path().join("src"))
                .unwrap()
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".recover"))
        );

        press(&mut editor, "C-x C-s");

        assert_eq!(copies(&recovery), Vec::<String>::new());
    }

    #[cfg(unix)]
    #[test]
    fn recovery_copies_are_only_for_their_owner() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, cache, mut editor) = recovering();
        edited(&mut editor);

        editor.auto_save();

        let copy = fs::read_dir(cache.path().join("recovery"))
            .unwrap()
            .flatten()
            .next()
            .unwrap();
        assert_eq!(copy.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn a_newer_copy_is_offered_when_the_file_is_visited_again() {
        let (dir, cache, mut editor) = recovering();
        let earlier = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        fs::File::options()
            .write(true)
            .open(dir.path().join("src").join("main.rs"))
            .unwrap()
            .set_modified(earlier)
            .unwrap();
        edited(&mut editor);
        editor.auto_save();
        let mut later = Editor::new();
        later.set_recovery_folder(Some(cache.path().join("recovery")));

        later.open_file(&dir.path().join("src").join("main.rs"));

        assert_eq!(
            question(&later),
            Some("Recover unsaved changes to main.rs? (y or n) ".to_owned())
        );
        press(&mut later, "y");
        assert_eq!(later.buffer().contents(), "// fn main() {}\n");
        assert!(later.buffer().is_modified());
    }

    #[test]
    fn killing_a_buffer_drops_its_copy() {
        let (_dir, cache, mut editor) = recovering();
        edited(&mut editor);
        editor.auto_save();

        press(&mut editor, "C-x k RET y");

        assert_eq!(copies(&cache.path().join("recovery")), Vec::<String>::new());
    }

    #[test]
    fn auto_save_can_be_turned_off() {
        let (_dir, cache, mut editor) = recovering();
        editor.apply_config(&Config::default().with("auto-save", json!(false)).unwrap());
        edited(&mut editor);

        assert!(!editor.wants_auto_save());
        editor.auto_save();

        assert_eq!(copies(&cache.path().join("recovery")), Vec::<String>::new());
    }

    #[test]
    fn visited_files_can_be_saved_in_place_and_linted() {
        let (dir, mut editor) = project();
        visiting(&mut editor);
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
    fn files_changed_on_disk_are_not_overwritten() {
        let (dir, mut editor) = project();
        visiting(&mut editor);
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
        editor.apply_config(
            &Config::default()
                .with("format-on-save", json!(true))
                .unwrap()
                .with("auto-save-visited", json!(true))
                .unwrap(),
        );
        editor.add_formatter(
            crate::syntax::Language::RUST,
            crate::editor::Formatter {
                name: "upper",
                format: |_, request| Ok(request.text.to_uppercase()),
                regions: false,
                on_save: false,
            },
        );
        edited(&mut editor);

        editor.auto_save();

        assert_eq!(on_disk(&dir), "// fn main() {}\n");
    }
}
