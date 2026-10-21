use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::Editor;
use oxmacs_core::text::{Buffer, BufferId};
use oxmacs_core::theme::Highlight;

use crate::diff::LineKind;
use crate::mode_line;
use crate::pane::{self, DiffPane};
use crate::repo::{self, Summary};
use crate::status::{self, Folds, Rendered, Row, Section, Status};

const DIFFED: usize = 200;

#[derive(Debug)]
pub(crate) struct GitState {
    pub(crate) scope: ScopeId,
    pub(crate) diff_scope: ScopeId,
    pub(crate) patches: usize,
    pub(crate) pending: Vec<crate::commit::Pending>,
    pub(crate) mode_line: mode_line::Cache,
}

#[derive(Debug)]
pub(crate) struct StatusView {
    scope: ScopeId,
    pub(crate) root: PathBuf,
    pub(crate) metadata: PathBuf,
    pub(crate) status: Status,
    pub(crate) folds: Folds,
    pub(crate) rendered: Rendered,
}

impl AppletBuffer for StatusView {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn line_style(&self, line: usize) -> LineStyle {
        self.rendered.styles.get(line).cloned().unwrap_or_default()
    }

    fn pills(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
        self.rendered.pills.get(line).cloned().unwrap_or_default()
    }

    fn directory(&self) -> Option<&Path> {
        Some(&self.root)
    }

    fn fixed_name(&self) -> bool {
        true
    }

    fn watched(&self) -> Option<&Path> {
        Some(&self.metadata)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Spot {
    Section(Section),
    File(Section, String),
    Hunk(Section, String, usize),
    Line(Section, String, usize, usize),
    Stash(usize),
    Commit(usize),
}

pub(crate) fn spot(status: &Status, row: Row) -> Option<Spot> {
    let path = |section, index| status.path(section, index).map(str::to_owned);
    Some(match row {
        Row::Info | Row::Blank => return None,
        Row::Section(section) => Spot::Section(section),
        Row::File(section, index) => Spot::File(section, path(section, index)?),
        Row::Hunk(section, index, hunk) => Spot::Hunk(section, path(section, index)?, hunk),
        Row::Line(section, index, hunk, line) => {
            Spot::Line(section, path(section, index)?, hunk, line)
        }
        Row::Pair(section, index, hunk, left, right) => {
            Spot::Line(section, path(section, index)?, hunk, left.or(right)?)
        }
        Row::Stash(index) => Spot::Stash(index),
        Row::Commit(index) => Spot::Commit(index),
    })
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

pub(crate) fn read(start: &Path) -> Result<Status, String> {
    let snapshot = repo::snapshot(start)?;
    let mut status = Status {
        snapshot,
        diffs: HashMap::new(),
    };
    let mut wanted: Vec<(Section, String)> = Vec::new();
    for section in [Section::Untracked, Section::Unstaged, Section::Staged] {
        wanted.extend(
            status
                .files(section)
                .into_iter()
                .take(DIFFED)
                .map(|change| (section, change.path)),
        );
    }
    let asked: Vec<(&str, repo::Side)> = wanted
        .iter()
        .filter_map(|(section, path)| Some((path.as_str(), section.side()?)))
        .collect();
    let found = repo::file_diffs(&status.snapshot.root, &asked)?;
    status.diffs = wanted.into_iter().zip(found).collect();
    Ok(status)
}

pub(crate) fn current(editor: &Editor) -> Option<(BufferId, &StatusView)> {
    let id = editor.buffer().id();
    editor
        .applet_buffer::<StatusView>(id)
        .map(|(_, view)| (id, view))
}

#[derive(Debug)]
pub(crate) struct Here<'a> {
    pub(crate) status_id: BufferId,
    pub(crate) view: &'a StatusView,
    pub(crate) rows: &'a [Row],
    pub(crate) line: usize,
}

pub(crate) fn here(editor: &Editor) -> Option<Here<'_>> {
    let buffer = editor.buffer();
    let id = buffer.id();
    let line = buffer.line_of(buffer.point());
    if let Some((_, view)) = editor.applet_buffer::<StatusView>(id) {
        return Some(Here {
            status_id: id,
            view,
            rows: &view.rendered.rows,
            line,
        });
    }
    let (_, diff) = editor.applet_buffer::<DiffPane>(id)?;
    let status_id = editor.find_applet_buffer::<StatusView>(|view| view.root == diff.root)?;
    let (_, view) = editor.applet_buffer::<StatusView>(status_id)?;
    Some(Here {
        status_id,
        view,
        rows: &diff.rendered.rows,
        line,
    })
}

pub(crate) fn row_at_point(editor: &Editor) -> Option<(BufferId, Row)> {
    let here = here(editor)?;
    let row = *here.rows.get(here.line)?;
    Some((here.status_id, row))
}

pub(crate) fn open(editor: &mut Editor) {
    let start = editor.default_directory();
    let Ok(status) = read(&start) else {
        editor.show_message("Not inside a Git repository");
        return;
    };
    let root = status.snapshot.root.clone();
    if let Some(id) = editor.find_applet_buffer::<StatusView>(|view| view.root == root) {
        editor.show_buffer(id);
        update(editor, id, Some(status), None);
        pane::show(editor);
        return;
    }
    let Some(scope) = editor.applet_state::<GitState>().map(|state| state.scope) else {
        return;
    };
    let name = format!(
        "git: {}",
        root.file_name().map_or_else(
            || root.display().to_string(),
            |name| name.to_string_lossy().into_owned()
        )
    );
    let metadata = status.snapshot.metadata.clone();
    let id = editor.add_applet_buffer_with(&name, |buffer| {
        buffer.set_read_only(true);
        StatusView {
            scope,
            root,
            metadata,
            status: Status::default(),
            folds: Folds::default(),
            rendered: Rendered::default(),
        }
    });
    editor.show_buffer(id);
    update(editor, id, Some(status), None);
    if let Some((buffer, view)) = editor.applet_buffer_mut::<StatusView>(id) {
        let rows = &view.rendered.rows;
        let first = rows
            .iter()
            .position(|row| matches!(row, Row::File(..)))
            .or_else(|| rows.iter().position(|row| matches!(row, Row::Section(_))))
            .unwrap_or(0);
        place(buffer, first);
    }
    pane::show(editor);
}

pub(crate) fn update(
    editor: &mut Editor,
    id: BufferId,
    fresh: Option<Status>,
    focus: Option<Spot>,
) {
    let now = now();
    let Some((buffer, view)) = editor.applet_buffer_mut::<StatusView>(id) else {
        return;
    };
    let line = buffer.line_of(buffer.point());
    let focus = focus.or_else(|| {
        view.rendered
            .rows
            .get(line)
            .and_then(|row| spot(&view.status, *row))
    });
    let summary = fresh.as_ref().map(|fresh| Summary::of(&fresh.snapshot));
    if let Some(fresh) = fresh {
        view.status = fresh;
    }
    view.rendered = status::render(&view.status, &view.folds, now);
    buffer.set_text(&view.rendered.text);
    let found = focus.and_then(|focus| {
        view.rendered
            .rows
            .iter()
            .position(|row| spot(&view.status, *row).as_ref() == Some(&focus))
    });
    let last = view.rendered.rows.len().saturating_sub(1);
    place(buffer, found.unwrap_or(line.min(last)));
    let root = view.root.clone();
    if let Some(summary) = summary {
        mode_line::remember(editor, &root, summary);
    }
    pane::redraw_root(editor, &root);
}

fn place(buffer: &mut Buffer, line: usize) {
    let start = buffer.line_start(line);
    buffer.set_point(start);
}

pub(crate) fn refresh(editor: &mut Editor) {
    let Some((id, view)) = current(editor) else {
        return;
    };
    let root = view.root.clone();
    reread(editor, id, &root);
}

pub(crate) fn reread(editor: &mut Editor, id: BufferId, root: &Path) {
    match read(root) {
        Ok(status) => update(editor, id, Some(status), None),
        Err(error) => editor.show_message(error),
    }
}

pub(crate) fn refresh_root(editor: &mut Editor, root: &Path) {
    for id in editor.applet_buffers::<StatusView>() {
        let same = editor
            .applet_buffer::<StatusView>(id)
            .is_some_and(|(_, view)| view.root == root);
        if same {
            reread(editor, id, root);
        }
    }
}

pub(crate) fn refresh_all(editor: &mut Editor) {
    for id in editor.applet_buffers::<StatusView>() {
        let Some(root) = editor
            .applet_buffer::<StatusView>(id)
            .map(|(_, view)| view.root.clone())
        else {
            continue;
        };
        if let Ok(status) = read(&root) {
            let changed = editor
                .applet_buffer::<StatusView>(id)
                .is_some_and(|(_, view)| view.status != status);
            if changed {
                update(editor, id, Some(status), None);
            }
        }
    }
}

pub(crate) fn refresh_saved(editor: &mut Editor, _id: BufferId) {
    refresh_all(editor);
}

fn item(row: Row) -> bool {
    matches!(
        row,
        Row::Section(_) | Row::File(..) | Row::Hunk(..) | Row::Stash(_) | Row::Commit(_)
    )
}

pub(crate) fn step(editor: &mut Editor, forward: bool) {
    let Some(here) = here(editor) else {
        return;
    };
    let (line, rows) = (here.line, here.rows);
    let found = if forward {
        (line + 1..rows.len()).find(|at| item(rows[*at]))
    } else {
        (0..line).rev().find(|at| item(rows[*at]))
    };
    if let Some(found) = found {
        place(editor.buffer_mut(), found);
    }
}

pub(crate) fn toggle(editor: &mut Editor) {
    let Some((id, row)) = row_at_point(editor) else {
        return;
    };
    let Some((_, view)) = editor.applet_buffer_mut::<StatusView>(id) else {
        return;
    };
    let path = |section, index| view.status.path(section, index).map(str::to_owned);
    let focus = match row {
        Row::Section(section) => {
            if !view.folds.closed.remove(&section) {
                view.folds.closed.insert(section);
            }
            None
        }
        Row::File(section, index) => {
            let Some(path) = path(section, index) else {
                return;
            };
            let key = (section, path);
            if !view.folds.open.remove(&key) {
                view.folds.open.insert(key);
            }
            None
        }
        Row::Hunk(section, index, hunk)
        | Row::Line(section, index, hunk, _)
        | Row::Pair(section, index, hunk, ..) => {
            let Some(path) = path(section, index) else {
                return;
            };
            let key = (section, path.clone(), hunk);
            if matches!(row, Row::Line(..) | Row::Pair(..)) || !view.folds.hidden.remove(&key) {
                view.folds.hidden.insert(key);
            }
            Some(Spot::Hunk(section, path, hunk))
        }
        Row::Info | Row::Blank | Row::Stash(_) | Row::Commit(_) => return,
    };
    update(editor, id, None, focus);
}

pub(crate) fn visit(editor: &mut Editor) {
    let Some((id, row)) = row_at_point(editor) else {
        return;
    };
    let Some((_, view)) = editor.applet_buffer::<StatusView>(id) else {
        return;
    };
    let (section, index, line) = match row {
        Row::File(section, index) => (section, index, None),
        Row::Hunk(section, index, hunk) => (section, index, Some((hunk, None))),
        Row::Line(section, index, hunk, line) => (section, index, Some((hunk, Some(line)))),
        Row::Pair(section, index, hunk, left, right) => {
            (section, index, Some((hunk, right.or(left))))
        }
        _ => {
            editor.show_message("Nothing to visit here");
            return;
        }
    };
    let Some(path) = view.status.path(section, index) else {
        return;
    };
    let target = view.root.join(path);
    let number = line.and_then(|(hunk, line)| {
        let hunk = view.status.diff(section, path)?.hunks.get(hunk)?;
        let before = line.map_or(0, |line| {
            hunk.lines[..line]
                .iter()
                .filter(|line| line.kind != LineKind::Removed)
                .count()
        });
        Some(hunk.new_start.max(1) + before)
    });
    if !target.is_file() {
        editor.show_message(format!("{path} is not in the working tree"));
        return;
    }
    if editor.open_file(&target)
        && let Some(number) = number
    {
        editor.go_to_line(number, 1);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::Editor;

    use crate::repo::fixtures::{git, repository};
    use crate::testing::{at_point, installed, lines, press};

    fn changed() -> (tempfile::TempDir, Editor) {
        let dir = repository();
        fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();
        fs::write(dir.path().join("c.txt"), "new\n").unwrap();
        fs::write(dir.path().join("d.txt"), "staged\n").unwrap();
        git(dir.path(), &["add", "d.txt"]);
        let editor = installed(dir.path());
        (dir, editor)
    }

    #[test]
    fn c_x_g_shows_the_status_with_point_on_the_first_file() {
        let (dir, mut editor) = changed();

        press(&mut editor, "C-x g");

        let name = dir.path().file_name().unwrap().to_string_lossy();
        assert_eq!(editor.buffer().name(), format!("git: {name}"));
        assert!(editor.buffer().is_read_only());
        assert_eq!(at_point(&editor), "  ?  c.txt  +1");
        let shown = lines(&editor);
        assert!(shown[0].starts_with("Head:     main  "), "{shown:?}");
        assert!(shown.contains(&"  M  a.txt  +1 -1".to_owned()), "{shown:?}");
        assert!(shown.contains(&"  A  d.txt  +1".to_owned()), "{shown:?}");
        assert_eq!(editor.default_directory(), dir.path());
    }

    #[test]
    fn outside_a_repository_c_x_g_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = installed(dir.path());

        press(&mut editor, "C-x g");

        assert_eq!(editor.message(), Some("Not inside a Git repository"));
        assert_eq!(editor.buffer().name(), "*scratch*");
    }

    #[test]
    fn n_and_p_move_between_sections_and_files() {
        let (_dir, mut editor) = changed();
        press(&mut editor, "C-x g");

        for (keys, expected) in [
            ("n", "Unstaged changes (1)"),
            ("n", "  M  a.txt  +1 -1"),
            ("n", "Staged changes (1)"),
            ("p", "  M  a.txt  +1 -1"),
            ("p p p", "Untracked files (1)"),
        ] {
            press(&mut editor, keys);
            assert_eq!(at_point(&editor), expected, "{keys}");
        }
    }

    #[test]
    fn tab_opens_a_file_diff_and_folds_hunks_and_sections() {
        let (_dir, mut editor) = changed();
        press(&mut editor, "C-x g n n");

        press(&mut editor, "TAB");
        let shown = lines(&editor);
        let file = shown
            .iter()
            .position(|line| line.contains("a.txt"))
            .unwrap();
        assert_eq!(
            shown[file + 1..file + 6],
            ["@@ -1,3 +1,3 @@", " one", "-two", "+TWO", " three"]
        );
        assert_eq!(at_point(&editor), "  M  a.txt  +1 -1");

        press(&mut editor, "n TAB");
        assert_eq!(at_point(&editor), "@@ -1,3 +1,3 @@");
        assert_eq!(lines(&editor)[file + 2], "");

        press(&mut editor, "TAB C-n C-n TAB");
        assert_eq!(at_point(&editor), "@@ -1,3 +1,3 @@");
        assert_eq!(lines(&editor)[file + 2], "");

        press(&mut editor, "p p TAB");
        assert_eq!(at_point(&editor), "Unstaged changes (1) …");
        assert!(!lines(&editor).iter().any(|line| line.contains("a.txt")));
        assert!(!editor.buffer().is_modified());
    }

    #[test]
    fn return_visits_the_file_at_the_line_shown() {
        let (dir, mut editor) = changed();
        press(&mut editor, "C-x g n n TAB n C-n C-n C-n");
        assert_eq!(at_point(&editor), "+TWO");

        press(&mut editor, "RET");

        assert_eq!(
            editor.buffer().file().unwrap().path,
            dir.path().join("a.txt")
        );
        let buffer = editor.buffer();
        assert_eq!(buffer.line_of(buffer.point()), 1);
    }

    #[test]
    fn g_reads_the_repository_again_keeping_point_on_its_file() {
        let (dir, mut editor) = changed();
        press(&mut editor, "C-x g n n");
        fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();

        press(&mut editor, "g");

        assert!(lines(&editor).contains(&"  M  b.txt  +1 -1".to_owned()));
        assert_eq!(at_point(&editor), "  M  a.txt  +1 -1");
    }

    #[test]
    fn c_x_g_again_reuses_and_refreshes_the_buffer() {
        let (dir, mut editor) = changed();
        press(&mut editor, "C-x g");
        editor.open_file(&dir.path().join("b.txt"));
        fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();

        press(&mut editor, "C-x g");

        assert!(editor.buffer().name().starts_with("git: "));
        assert!(lines(&editor).contains(&"  M  b.txt  +1 -1".to_owned()));
        assert_eq!(
            editor
                .buffer_names()
                .iter()
                .filter(|name| name.starts_with("git: "))
                .count(),
            1
        );
    }

    #[test]
    fn saving_a_file_refreshes_the_status() {
        let (dir, mut editor) = changed();
        press(&mut editor, "C-x g");
        editor.open_file(&dir.path().join("b.txt"));

        press(&mut editor, "x C-x C-s");
        press(&mut editor, "C-x b RET");

        assert!(editor.buffer().name().starts_with("git: "));
        assert!(lines(&editor).contains(&"  M  b.txt  +1 -1".to_owned()));
    }

    #[test]
    fn the_metadata_folder_is_watched() {
        let (dir, mut editor) = changed();
        press(&mut editor, "C-x g");

        assert!(
            editor
                .watched_directories()
                .contains(&dir.path().join(".git"))
        );
    }
}
