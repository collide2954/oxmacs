use std::fs;
use std::path::{Path, PathBuf};

use oxmacs_core::editor::Editor;
use oxmacs_core::files::paths::SEPARATOR as SEP;

use crate::Dired;
use crate::testing::lines;

pub(crate) fn workspace() -> (tempfile::TempDir, Editor) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello").unwrap();
    fs::write(dir.path().join("b.txt"), "bee").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub").join("c.txt"), "sea").unwrap();
    let mut editor = Editor::new();
    editor.install(&Dired);
    editor.set_home_directory(None);
    editor.set_working_directory(dir.path().to_owned());
    (dir, editor)
}

pub(crate) fn dir_name(path: &Path) -> String {
    format!("{}{SEP}", path.file_name().unwrap().to_string_lossy())
}

pub(crate) fn listed(editor: &mut Editor, dir: &tempfile::TempDir) {
    crate::browse::dired(editor, dir.path());
}

pub(crate) fn mark_column(editor: &Editor) -> String {
    lines(editor)[2..]
        .iter()
        .map(|line| line.chars().next().unwrap())
        .collect()
}

pub(crate) fn trashed() -> Vec<PathBuf> {
    crate::ops::TRASHED.with_borrow(Clone::clone)
}

pub(crate) fn permanent(editor: &mut Editor, dir: &tempfile::TempDir) {
    let config = oxmacs_core::config::Config::parse("[dired]\ndelete-to-trash = false").unwrap();
    editor.apply_config(&config);
    listed(editor, dir);
}
