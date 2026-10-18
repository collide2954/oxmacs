use std::fs;
use std::path::{Path, PathBuf};

use crate::editor::Editor;
use crate::editor::test_support::lines;
use crate::files::paths::SEPARATOR as SEP;

pub(super) fn workspace() -> (tempfile::TempDir, Editor) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello").unwrap();
    fs::write(dir.path().join("b.txt"), "bee").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub").join("c.txt"), "sea").unwrap();
    let mut editor = Editor::new();
    editor.set_home_directory(None);
    editor.set_working_directory(dir.path().to_owned());
    (dir, editor)
}

pub(super) fn dir_name(path: &Path) -> String {
    format!("{}{SEP}", path.file_name().unwrap().to_string_lossy())
}

pub(super) fn listed(editor: &mut Editor, dir: &tempfile::TempDir) {
    editor.dired(dir.path());
}

pub(super) fn mark_column(editor: &Editor) -> String {
    lines(editor)[2..]
        .iter()
        .map(|line| line.chars().next().unwrap())
        .collect()
}

pub(super) fn trashed() -> Vec<PathBuf> {
    crate::files::ops::TRASHED.with_borrow(Clone::clone)
}

pub(super) fn permanent(editor: &mut Editor, dir: &tempfile::TempDir) {
    let config = crate::config::Config::parse("delete-to-trash = false").unwrap();
    editor.apply_config(&config);
    listed(editor, dir);
}
