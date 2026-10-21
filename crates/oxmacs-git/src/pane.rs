use std::ops::Range;
use std::path::{Path, PathBuf};

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::Editor;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::diff::{Hunk, LineKind};
use crate::repo::FileDiff;
use crate::status::{Line, Rendered, Row, Section, stats};
use crate::view::{self, GitState, StatusView};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Layout {
    #[default]
    Unified,
    Split,
}

#[derive(Debug)]
pub(crate) struct DiffPane {
    scope: ScopeId,
    pub(crate) root: PathBuf,
    pub(crate) file: Option<(Section, String)>,
    pub(crate) staged: bool,
    pub(crate) layout: Layout,
    pub(crate) width: usize,
    pub(crate) rendered: Rendered,
}

impl AppletBuffer for DiffPane {
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
}

const SEPARATOR: &str = " │ ";
const NARROWEST: usize = 12;
const WIDTH: usize = 100;

fn digits(diff: &FileDiff) -> usize {
    diff.hunks
        .iter()
        .map(|hunk| (hunk.old_start + hunk.old_len).max(hunk.new_start + hunk.new_len))
        .max()
        .unwrap_or(1)
        .to_string()
        .len()
}

fn numbered(value: Option<usize>, width: usize) -> String {
    value.map_or_else(|| " ".repeat(width), |value| format!("{value:>width$}"))
}

fn label(section: Section) -> &'static str {
    match section {
        Section::Untracked => "Untracked",
        Section::Unstaged => "Unstaged",
        Section::Staged => "Staged",
        Section::Stashes | Section::Recent | Section::Revision => "",
    }
}

pub(crate) fn render(
    section: Section,
    index: usize,
    path: &str,
    diff: &FileDiff,
    layout: Layout,
    width: usize,
) -> Rendered {
    let mut out = Rendered::default();
    let mut line = Line::default();
    line.pill(label(section), Highlight::Link);
    line.push("  ", None);
    line.push(path, Some(Highlight::Heading));
    line.push("  ", None);
    stats(&mut line, diff);
    out.add(Row::File(section, index), line);
    hunks(&mut out, section, index, diff, layout, width);
    out
}

pub(crate) fn hunks(
    out: &mut Rendered,
    section: Section,
    index: usize,
    diff: &FileDiff,
    layout: Layout,
    width: usize,
) {
    let digits = digits(diff);
    for (number, hunk) in diff.hunks.iter().enumerate() {
        out.add(Row::Blank, Line::default());
        let mut line = Line::default();
        line.push(&hunk.header(), Some(Highlight::Keyword));
        out.add(Row::Hunk(section, index, number), line);
        match layout {
            Layout::Unified => unified(out, (section, index, number), hunk, digits),
            Layout::Split => split(out, (section, index, number), hunk, digits, width),
        }
    }
}

fn nothing(path: &str) -> Rendered {
    let mut out = Rendered::default();
    let mut line = Line::default();
    line.push(&format!("No changes to {path}"), Some(Highlight::Comment));
    out.add(Row::Blank, line);
    out
}

type Place = (Section, usize, usize);

fn unified(out: &mut Rendered, place: Place, hunk: &Hunk, digits: usize) {
    let (section, index, number) = place;
    let (mut old, mut new) = (hunk.old_start, hunk.new_start);
    for (at, diff_line) in hunk.lines.iter().enumerate() {
        let (left, right) = match diff_line.kind {
            LineKind::Context => (Some(old), Some(new)),
            LineKind::Removed => (Some(old), None),
            LineKind::Added => (None, Some(new)),
        };
        old += usize::from(left.is_some());
        new += usize::from(right.is_some());
        let mut line = Line::default();
        line.push(&numbered(left, digits), Some(Highlight::Comment));
        line.push(" ", None);
        line.push(&numbered(right, digits), Some(Highlight::Comment));
        line.push(" ", None);
        match diff_line.kind {
            LineKind::Context => {
                line.push(" ", None);
            }
            LineKind::Removed => {
                line.push("-", Some(Highlight::Error));
                line.background(Highlight::Removed);
            }
            LineKind::Added => {
                line.push("+", Some(Highlight::Success));
                line.background(Highlight::Added);
            }
        }
        line.push(&diff_line.text, None);
        out.add(Row::Line(section, index, number, at), line);
    }
}

pub(crate) fn pairs(hunk: &Hunk) -> Vec<(Option<usize>, Option<usize>)> {
    let mut found = Vec::new();
    let mut removed: Vec<usize> = Vec::new();
    let mut added: Vec<usize> = Vec::new();
    let flush = |found: &mut Vec<_>, removed: &mut Vec<usize>, added: &mut Vec<usize>| {
        for row in 0..removed.len().max(added.len()) {
            found.push((removed.get(row).copied(), added.get(row).copied()));
        }
        removed.clear();
        added.clear();
    };
    for (at, line) in hunk.lines.iter().enumerate() {
        match line.kind {
            LineKind::Removed => {
                if !added.is_empty() {
                    flush(&mut found, &mut removed, &mut added);
                }
                removed.push(at);
            }
            LineKind::Added => added.push(at),
            LineKind::Context => {
                flush(&mut found, &mut removed, &mut added);
                found.push((Some(at), Some(at)));
            }
        }
    }
    flush(&mut found, &mut removed, &mut added);
    found
}

fn clip(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count <= width {
        return format!("{text}{}", " ".repeat(width - count));
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

pub(crate) fn line_number(hunk: &Hunk, at: usize, old: bool) -> usize {
    let skipped = if old {
        LineKind::Added
    } else {
        LineKind::Removed
    };
    let start = if old { hunk.old_start } else { hunk.new_start };
    start
        + hunk.lines[..at]
            .iter()
            .filter(|line| line.kind != skipped)
            .count()
}

fn split(out: &mut Rendered, place: Place, hunk: &Hunk, digits: usize, width: usize) {
    let (section, index, number) = place;
    let half = width.saturating_sub(SEPARATOR.chars().count()) / 2;
    let text_width = half.saturating_sub(digits + 1).max(NARROWEST);
    for (left, right) in pairs(hunk) {
        let mut line = Line::default();
        let left_line = left.map(|at| &hunk.lines[at]);
        let right_line = right.map(|at| &hunk.lines[at]);
        line.push(
            &numbered(left.map(|at| line_number(hunk, at, true)), digits),
            Some(Highlight::Comment),
        );
        line.push(" ", None);
        line.push(
            &clip(left_line.map_or("", |line| line.text.as_str()), text_width),
            None,
        );
        if left_line.is_some_and(|line| line.kind == LineKind::Removed) {
            line.tint(0..line.width(), Highlight::Error);
        }
        line.push(SEPARATOR, Some(Highlight::Comment));
        let middle = line.width();
        line.push(
            &numbered(right.map(|at| line_number(hunk, at, false)), digits),
            Some(Highlight::Comment),
        );
        line.push(" ", None);
        line.push(
            &clip(right_line.map_or("", |line| line.text.as_str()), text_width),
            None,
        );
        if right_line.is_some_and(|line| line.kind == LineKind::Added) {
            line.tint(middle..line.width(), Highlight::Success);
        }
        out.add(Row::Pair(section, index, number, left, right), line);
    }
}

fn find(status: &crate::status::Status, file: &(Section, String)) -> Option<(Section, usize)> {
    let (section, path) = file;
    let position = |section: Section| {
        status
            .files(section)
            .iter()
            .position(|change| &change.path == path)
            .map(|index| (section, index))
    };
    position(*section).or_else(|| {
        [Section::Unstaged, Section::Staged, Section::Untracked]
            .into_iter()
            .find_map(position)
    })
}

fn staged_overview(status: &crate::status::Status, layout: Layout, width: usize) -> Rendered {
    let staged = status.files(Section::Staged);
    if staged.is_empty() {
        let mut out = Rendered::default();
        let mut line = Line::default();
        line.push("Nothing staged", Some(Highlight::Comment));
        out.add(Row::Blank, line);
        return out;
    }
    let mut out = Rendered::default();
    for (index, change) in staged.iter().enumerate() {
        if index > 0 {
            out.add(Row::Blank, Line::default());
        }
        let diff = status
            .diff(Section::Staged, &change.path)
            .cloned()
            .unwrap_or_default();
        out.append(render(
            Section::Staged,
            index,
            &change.path,
            &diff,
            layout,
            width,
        ));
    }
    out
}

pub(crate) fn redraw(editor: &mut Editor, pane_id: BufferId) {
    let width = editor.buffer_width(pane_id).unwrap_or(WIDTH);
    let Some((_, pane)) = editor.applet_buffer::<DiffPane>(pane_id) else {
        return;
    };
    let root = pane.root.clone();
    let layout = pane.layout;
    let staged = pane.staged;
    let Some(file) = pane.file.clone() else {
        return;
    };
    let Some(status_id) = editor.find_applet_buffer::<StatusView>(|view| view.root == root) else {
        return;
    };
    let Some((_, view)) = editor.applet_buffer::<StatusView>(status_id) else {
        return;
    };
    let (rendered, file) = match find(&view.status, &file) {
        _ if staged => (staged_overview(&view.status, layout, width), file),
        Some((section, index)) => {
            let path = file.1.clone();
            let diff = view
                .status
                .diff(section, &path)
                .cloned()
                .unwrap_or_default();
            (
                render(section, index, &path, &diff, layout, width),
                (section, path),
            )
        }
        None => (nothing(&file.1), file),
    };
    let Some((buffer, pane)) = editor.applet_buffer_mut::<DiffPane>(pane_id) else {
        return;
    };
    let line = buffer.line_of(buffer.point());
    pane.file = Some(file);
    pane.width = width;
    pane.rendered = rendered;
    buffer.set_text(&pane.rendered.text);
    let last = pane.rendered.rows.len().saturating_sub(1);
    let start = buffer.line_start(line.min(last));
    buffer.set_point(start);
}

pub(crate) fn file_at_point(editor: &Editor) -> Option<(PathBuf, Section, String)> {
    let (_, view) = view::current(editor)?;
    let buffer = editor.buffer();
    let (Row::File(section, index)
    | Row::Hunk(section, index, _)
    | Row::Line(section, index, ..)
    | Row::Pair(section, index, ..)) = *view.rendered.rows.get(buffer.line_of(buffer.point()))?
    else {
        return None;
    };
    let path = view.status.path(section, index)?.to_owned();
    Some((view.root.clone(), section, path))
}

fn pane_of(editor: &Editor, root: &Path) -> Option<BufferId> {
    editor.find_applet_buffer::<DiffPane>(|pane| pane.root == root)
}

pub(crate) fn follow(editor: &mut Editor) {
    let Some((root, section, path)) = file_at_point(editor) else {
        return;
    };
    let Some(pane_id) = pane_of(editor, &root) else {
        return;
    };
    if !editor.buffer_shown(pane_id) {
        return;
    }
    let wanted = Some((section, path));
    let same = editor
        .applet_buffer::<DiffPane>(pane_id)
        .is_some_and(|(_, pane)| pane.file == wanted && !pane.staged);
    if same {
        return;
    }
    if let Some((buffer, pane)) = editor.applet_buffer_mut::<DiffPane>(pane_id) {
        pane.file = wanted;
        pane.staged = false;
        buffer.set_point(0);
    }
    redraw(editor, pane_id);
}

fn pane_for(editor: &mut Editor, root: &Path) -> Option<BufferId> {
    if let Some(id) = pane_of(editor, root) {
        return Some(id);
    }
    let scope = editor
        .applet_state::<GitState>()
        .map(|state| state.diff_scope)?;
    let name = format!("git diff: {}", view::folder_name(root));
    let pane_root = root.to_owned();
    Some(editor.add_applet_buffer_with(&name, |buffer| {
        buffer.set_read_only(true);
        DiffPane {
            scope,
            root: pane_root,
            file: None,
            staged: false,
            layout: Layout::Unified,
            width: WIDTH,
            rendered: Rendered::default(),
        }
    }))
}

fn reveal(editor: &mut Editor, pane_id: BufferId) {
    if !editor.buffer_shown(pane_id) {
        let origin = editor.selected_window();
        editor.show_buffer_beside(pane_id);
        editor.select_window(origin);
    }
    redraw(editor, pane_id);
}

pub(crate) fn show(editor: &mut Editor) {
    let Some((root, section, path)) = file_at_point(editor) else {
        return;
    };
    let Some(pane_id) = pane_for(editor, &root) else {
        return;
    };
    if let Some((_, pane)) = editor.applet_buffer_mut::<DiffPane>(pane_id) {
        pane.file = Some((section, path));
        pane.staged = false;
    }
    reveal(editor, pane_id);
}

pub(crate) fn show_staged(editor: &mut Editor, root: &Path) {
    let Some(pane_id) = pane_for(editor, root) else {
        return;
    };
    if let Some((buffer, pane)) = editor.applet_buffer_mut::<DiffPane>(pane_id) {
        pane.staged = true;
        if pane.file.is_none() {
            pane.file = Some((Section::Staged, String::new()));
        }
        buffer.set_point(0);
    }
    reveal(editor, pane_id);
}
pub(crate) fn fit(editor: &mut Editor) {
    for id in editor.applet_buffers::<DiffPane>() {
        let Some(width) = editor.buffer_width(id) else {
            continue;
        };
        let stale = editor
            .applet_buffer::<DiffPane>(id)
            .is_some_and(|(_, pane)| pane.layout == Layout::Split && pane.width != width);
        if stale {
            redraw(editor, id);
        }
    }
}

pub(crate) fn redraw_root(editor: &mut Editor, root: &Path) {
    if let Some(pane_id) = pane_of(editor, root) {
        redraw(editor, pane_id);
    }
}

pub(crate) fn refresh(editor: &mut Editor) {
    let id = editor.buffer().id();
    let Some(root) = editor
        .applet_buffer::<DiffPane>(id)
        .map(|(_, pane)| pane.root.clone())
    else {
        return;
    };
    view::refresh_root(editor, &root);
}

pub(crate) fn toggle_layout(editor: &mut Editor) {
    let id = editor.buffer().id();
    let Some((_, pane)) = editor.applet_buffer_mut::<DiffPane>(id) else {
        return;
    };
    pane.layout = match pane.layout {
        Layout::Unified => Layout::Split,
        Layout::Split => Layout::Unified,
    };
    redraw(editor, id);
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::Editor;
    use oxmacs_core::theme::Highlight;

    use super::*;
    use crate::diff::hunks;
    use crate::repo::fixtures::{git, repository};
    use crate::testing::{at_point, installed, press, run_jobs};

    fn sample() -> FileDiff {
        FileDiff {
            hunks: hunks("one\ntwo\nthree\n", "one\nTWO\nthree\nfour\n", 3),
            ..FileDiff::default()
        }
    }

    fn text(rendered: &Rendered) -> Vec<&str> {
        rendered.text.split('\n').collect()
    }

    #[test]
    fn unified_diffs_number_both_sides() {
        let rendered = render(
            Section::Unstaged,
            0,
            "a.txt",
            &sample(),
            Layout::Unified,
            80,
        );

        assert_eq!(
            text(&rendered),
            [
                "Unstaged  a.txt  +2 -1",
                "",
                "@@ -1,3 +1,4 @@",
                "1 1  one",
                "2   -two",
                "  2 +TWO",
                "3 3  three",
                "  4 +four",
            ]
        );
        assert_eq!(rendered.pills[0], [(0..8, Highlight::Link)]);
        assert_eq!(rendered.styles[4].background, Some(Highlight::Removed));
        assert_eq!(rendered.styles[5].background, Some(Highlight::Added));
        assert_eq!(rendered.rows[5], Row::Line(Section::Unstaged, 0, 0, 2));
    }

    #[test]
    fn split_diffs_put_old_and_new_side_by_side() {
        let rendered = render(Section::Unstaged, 0, "a.txt", &sample(), Layout::Split, 40);

        assert_eq!(
            text(&rendered)[3..],
            [
                "1 one              │ 1 one             ",
                "2 two              │ 2 TWO             ",
                "3 three            │ 3 three           ",
                "                   │ 4 four            ",
            ]
        );
        assert_eq!(
            rendered.rows[4],
            Row::Pair(Section::Unstaged, 0, 0, Some(1), Some(2))
        );
        assert_eq!(
            rendered.pills[4],
            [(0..18, Highlight::Error), (21..39, Highlight::Success)]
        );
        assert_eq!(rendered.pills[3], []);
    }

    #[test]
    fn long_lines_are_cut_short_in_split_diffs() {
        let diff = FileDiff {
            hunks: hunks("a\n", &format!("{}\n", "x".repeat(50)), 3),
            ..FileDiff::default()
        };

        let rendered = render(Section::Unstaged, 0, "a.txt", &diff, Layout::Split, 40);

        assert!(text(&rendered)[3].ends_with(&format!("{}…", "x".repeat(15))));
    }

    #[test]
    fn removed_and_added_runs_pair_up_in_order() {
        let hunk = hunks("a\nb\nc\nd\n", "a\nB\nd\nE\nF\n", 3).remove(0);

        assert_eq!(
            pairs(&hunk),
            [
                (Some(0), Some(0)),
                (Some(1), Some(3)),
                (Some(2), None),
                (Some(4), Some(4)),
                (None, Some(5)),
                (None, Some(6)),
            ]
        );
    }

    fn changed() -> (tempfile::TempDir, Editor) {
        let dir = repository();
        fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\nfour\n").unwrap();
        fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
        let mut editor = installed(dir.path());
        let window = editor.selected_window();
        editor.resize_pane(window, 30, 160);
        press(&mut editor, "C-x g");
        (dir, editor)
    }

    fn pane_text(editor: &Editor) -> String {
        let id = editor.find_applet_buffer::<DiffPane>(|_| true).unwrap();
        editor.buffer_by_id(id).unwrap().contents()
    }

    fn into_pane(editor: &mut Editor) {
        press(editor, "C-x o");
    }

    #[test]
    fn the_status_opens_the_diff_beside_it_and_keeps_focus() {
        let (_dir, editor) = changed();

        assert!(editor.buffer().name().starts_with("git: "));
        assert_eq!(editor.tiling().windows().len(), 2);
        assert!(pane_text(&editor).starts_with("Unstaged  a.txt  +2 -1\n"));
    }

    #[test]
    fn the_diff_follows_the_file_at_point() {
        let (_dir, mut editor) = changed();

        press(&mut editor, "C-n");

        assert_eq!(at_point(&editor), "  M  b.txt  +1 -1");
        assert!(pane_text(&editor).starts_with("Unstaged  b.txt  +1 -1\n"));
    }

    #[test]
    fn t_in_the_diff_switches_to_side_by_side() {
        let (_dir, mut editor) = changed();
        into_pane(&mut editor);

        press(&mut editor, "t");

        assert!(pane_text(&editor).contains(" │ "));
        press(&mut editor, "t");
        assert!(!pane_text(&editor).contains(" │ "));
    }

    #[test]
    fn s_in_the_diff_stages_and_the_diff_moves_with_the_file() {
        let (dir, mut editor) = changed();
        into_pane(&mut editor);

        press(&mut editor, "n s");
        run_jobs(&mut editor);

        assert_eq!(
            git(dir.path(), &["show", ":a.txt"]),
            "one\nTWO\nthree\nfour\n"
        );
        assert!(pane_text(&editor).starts_with("Staged  a.txt  +2 -1\n"));
    }

    #[test]
    fn a_region_in_a_side_by_side_diff_stages_its_lines() {
        let (dir, mut editor) = changed();
        into_pane(&mut editor);
        press(&mut editor, "t n C-n C-n");
        assert!(at_point(&editor).starts_with("2 two"));

        press(&mut editor, "C-SPC C-e s");
        run_jobs(&mut editor);

        assert_eq!(git(dir.path(), &["show", ":a.txt"]), "one\nTWO\nthree\n");
    }

    #[test]
    fn side_by_side_diffs_refit_when_their_window_changes_width() {
        let (_dir, mut editor) = changed();
        into_pane(&mut editor);
        press(&mut editor, "t");
        let window = editor.selected_window();

        editor.resize_pane(window, 30, 50);
        editor.prepare_display();

        let id = editor.buffer().id();
        let width = editor.buffer_width(id).unwrap();
        let longest = pane_text(&editor)
            .lines()
            .skip(3)
            .map(|line| line.chars().count())
            .max()
            .unwrap();
        assert!(longest <= width, "{longest} > {width}");
        assert!(longest + 4 > width, "{longest} is far below {width}");
    }

    #[test]
    fn return_in_the_diff_visits_the_new_line() {
        let (dir, mut editor) = changed();
        into_pane(&mut editor);

        press(&mut editor, "n C-n C-n C-n RET");

        assert_eq!(
            editor.buffer().file().unwrap().path,
            dir.path().join("a.txt")
        );
        let buffer = editor.buffer();
        assert_eq!(buffer.line_of(buffer.point()), 1);
    }
}
