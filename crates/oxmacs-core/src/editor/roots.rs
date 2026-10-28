use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::editor::Editor;
use crate::files::roots::{PROJECT, located, upward};
use crate::syntax::{Language, Roots};

type Key = (PathBuf, String);

#[derive(Debug, Default)]
pub(in crate::editor) struct Found(Mutex<HashMap<Key, Option<PathBuf>>>);

impl Editor {
    fn remembered(
        &self,
        folder: &Path,
        question: &str,
        find: impl FnOnce() -> Option<PathBuf>,
    ) -> Option<PathBuf> {
        let key = (folder.to_path_buf(), question.to_owned());
        if let Some(known) = self
            .roots
            .0
            .lock()
            .ok()
            .and_then(|found| found.get(&key).cloned())
        {
            return known;
        }
        let root = find();
        if let Ok(mut found) = self.roots.0.lock() {
            found.insert(key, root.clone());
        }
        root
    }

    pub fn project_root(&self, folder: &Path) -> Option<PathBuf> {
        let home = self.home.as_deref();
        self.remembered(folder, "", || upward(folder, PROJECT, home))
    }

    pub fn language_root(&self, folder: &Path, language: Option<Language>) -> Option<PathBuf> {
        let home = self.home.as_deref();
        let declared = language.and_then(|language| {
            self.remembered(folder, language.name(), || {
                let Roots { markers, locate } = language.roots();
                locate
                    .as_ref()
                    .and_then(|command| located(folder, command))
                    .or_else(|| upward(folder, markers, home))
            })
        });
        declared.or_else(|| self.project_root(folder))
    }
}
#[cfg(test)]
mod tests {
    use std::fs;

    use crate::editor::Editor;
    use crate::syntax::Language;

    #[test]
    fn language_roots_fall_back_to_the_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let code = dir.path().join("repo/pkg/R");
        fs::create_dir_all(dir.path().join("repo/.git")).unwrap();
        fs::create_dir_all(&code).unwrap();
        fs::write(dir.path().join("repo/pkg/DESCRIPTION"), "").unwrap();
        let mut editor = Editor::new();
        editor.set_home_directory(Some(dir.path().to_owned()));

        assert_eq!(
            editor.language_root(&code, Some(Language::r())),
            Some(dir.path().join("repo/pkg"))
        );
        assert_eq!(
            editor.language_root(&code, Some(Language::toml())),
            Some(dir.path().join("repo"))
        );
        assert_eq!(editor.project_root(&code), Some(dir.path().join("repo")));
        assert_eq!(
            editor.language_root(&dir.path().join("repo"), None),
            Some(dir.path().join("repo"))
        );
        assert_eq!(editor.project_root(dir.path()), None);
    }

    #[test]
    fn rust_roots_are_the_cargo_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let member = dir.path().join("crates").join("a");
        fs::create_dir_all(member.join("src")).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a\"]\n",
        )
        .unwrap();
        fs::write(member.join("Cargo.toml"), "[package]\nname = \"a\"\n").unwrap();
        let editor = Editor::new();
        assert_eq!(
            editor.language_root(&member.join("src"), Some(Language::rust())),
            Some(member.clone())
        );

        fs::write(member.join("src/lib.rs"), "").unwrap();
        let editor = Editor::new();
        assert_eq!(
            editor.language_root(&member.join("src"), Some(Language::rust())),
            Some(dir.path().to_owned())
        );
    }
}
