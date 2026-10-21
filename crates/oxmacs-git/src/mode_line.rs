use std::collections::HashMap;
use std::path::{Path, PathBuf};

use oxmacs_core::editor::Editor;
use oxmacs_core::text::BufferId;

use crate::repo::{self, Summary};
use crate::view::{GitState, StatusView};

#[derive(Debug, Default)]
pub(crate) struct Cache {
    roots: HashMap<PathBuf, Option<PathBuf>>,
    summaries: HashMap<PathBuf, Summary>,
}

pub(crate) fn label(summary: &Summary) -> String {
    let branch = summary.branch.as_deref().unwrap_or("(detached)");
    let upstream = summary
        .upstream
        .map(|(ahead, behind)| format!(" ↑{ahead} ↓{behind}"))
        .unwrap_or_default();
    let dirty = if summary.dirty { " ●" } else { "" };
    format!("{branch}{upstream}{dirty}")
}

fn cache(editor: &Editor) -> Option<&Cache> {
    editor
        .applet_state::<GitState>()
        .map(|state| &state.mode_line)
}

fn cache_mut(editor: &mut Editor) -> Option<&mut Cache> {
    editor
        .applet_state_mut::<GitState>()
        .map(|state| &mut state.mode_line)
}

pub(crate) fn segment(editor: &Editor) -> Option<String> {
    let cache = cache(editor)?;
    let root = cache.roots.get(&editor.default_directory())?.as_ref()?;
    cache.summaries.get(root).map(label)
}

fn root_of(editor: &mut Editor, directory: PathBuf) -> Option<PathBuf> {
    if let Some(known) = cache(editor).and_then(|cache| cache.roots.get(&directory)) {
        return known.clone();
    }
    let found = repo::root(&directory);
    if let Some(cache) = cache_mut(editor) {
        cache.roots.insert(directory, found.clone());
    }
    found
}

pub(crate) fn notice(editor: &mut Editor) {
    let directory = editor.default_directory();
    let known = cache(editor).is_some_and(|cache| cache.roots.contains_key(&directory));
    if known {
        return;
    }
    let Some(root) = root_of(editor, directory) else {
        return;
    };
    let summarized = cache(editor).is_some_and(|cache| cache.summaries.contains_key(&root));
    if !summarized {
        refresh(editor, &root);
    }
}

pub(crate) fn remember(editor: &mut Editor, root: &Path, summary: Summary) {
    if let Some(cache) = cache_mut(editor) {
        cache
            .roots
            .entry(root.to_owned())
            .or_insert_with(|| Some(root.to_owned()));
        cache.summaries.insert(root.to_owned(), summary);
    }
}

pub(crate) fn refresh(editor: &mut Editor, root: &Path) {
    match repo::summary(root) {
        Ok(summary) => remember(editor, root, summary),
        Err(_) => {
            if let Some(cache) = cache_mut(editor) {
                cache.summaries.remove(root);
            }
        }
    }
}

fn has_status(editor: &Editor, root: &Path) -> bool {
    editor
        .find_applet_buffer::<StatusView>(|view| view.root == root)
        .is_some()
}

pub(crate) fn saved(editor: &mut Editor, id: BufferId) {
    let Some(directory) = editor
        .buffer_by_id(id)
        .and_then(|buffer| buffer.file())
        .and_then(|file| file.path.parent())
        .map(Path::to_path_buf)
    else {
        return;
    };
    let Some(root) = root_of(editor, directory) else {
        return;
    };
    if !has_status(editor, &root) {
        refresh(editor, &root);
    }
}

pub(crate) fn disk_changed(editor: &mut Editor) {
    let roots: Vec<PathBuf> = cache(editor)
        .map(|cache| cache.summaries.keys().cloned().collect())
        .unwrap_or_default();
    for root in roots {
        if !has_status(editor, &root) {
            refresh(editor, &root);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::Editor;

    use super::*;
    use crate::repo::fixtures::{git, repository};
    use crate::testing::{installed, press};

    fn summary(branch: Option<&str>, upstream: Option<(usize, usize)>, dirty: bool) -> Summary {
        Summary {
            branch: branch.map(str::to_owned),
            upstream,
            dirty,
        }
    }

    #[test]
    fn the_label_names_the_branch_upstream_counts_and_changes() {
        assert_eq!(label(&summary(Some("main"), None, false)), "main");
        assert_eq!(
            label(&summary(Some("main"), Some((2, 0)), true)),
            "main ↑2 ↓0 ●"
        );
        assert_eq!(label(&summary(None, None, true)), "(detached) ●");
    }

    fn visiting(dir: &Path) -> Editor {
        let mut editor = installed(dir);
        editor.open_file(&dir.join("a.txt"));
        press(&mut editor, "C-f");
        editor
    }

    #[test]
    fn the_mode_line_shows_the_branch_of_the_current_buffer() {
        let dir = repository();

        let editor = visiting(dir.path());

        assert_eq!(editor.mode_line_segments(), ["main"]);
    }

    #[test]
    fn buffers_outside_a_repository_show_nothing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "a\n").unwrap();

        let editor = visiting(dir.path());

        assert_eq!(editor.mode_line_segments(), Vec::<String>::new());
    }

    #[test]
    fn upstreams_add_commits_ahead_and_behind() {
        let dir = repository();
        git(dir.path(), &["branch", "-q", "base"]);
        git(dir.path(), &["branch", "-q", "--set-upstream-to=base"]);
        fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
        git(dir.path(), &["commit", "-q", "-am", "Ahead"]);

        let editor = visiting(dir.path());

        assert_eq!(editor.mode_line_segments(), ["main ↑1 ↓0"]);
    }

    #[test]
    fn a_detached_head_says_so() {
        let dir = repository();
        git(dir.path(), &["checkout", "-q", "--detach"]);

        let editor = visiting(dir.path());

        assert_eq!(editor.mode_line_segments(), ["(detached)"]);
    }

    #[test]
    fn the_segment_reads_a_cache_that_disk_changes_refresh() {
        let dir = repository();
        let mut editor = visiting(dir.path());
        fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();

        press(&mut editor, "C-f");
        assert_eq!(editor.mode_line_segments(), ["main"]);

        editor.check_disk_changes();
        assert_eq!(editor.mode_line_segments(), ["main ●"]);
    }

    #[test]
    fn saving_a_file_marks_the_change() {
        let dir = repository();
        let mut editor = visiting(dir.path());

        press(&mut editor, "x C-x C-s");

        assert_eq!(editor.mode_line_segments(), ["main ●"]);
    }

    #[test]
    fn reading_the_status_refreshes_the_segment() {
        let dir = repository();
        let mut editor = visiting(dir.path());
        fs::write(dir.path().join("c.txt"), "new\n").unwrap();

        press(&mut editor, "C-x g");
        let file = editor.buffer_visiting(&dir.path().join("a.txt")).unwrap();
        editor.show_buffer(file);

        assert_eq!(editor.mode_line_segments(), ["main ●"]);
    }

    #[test]
    fn staged_changes_count_as_changes() {
        let dir = repository();
        fs::write(dir.path().join("d.txt"), "staged\n").unwrap();
        git(dir.path(), &["add", "d.txt"]);

        let editor = visiting(dir.path());

        assert_eq!(editor.mode_line_segments(), ["main ●"]);
    }
}
