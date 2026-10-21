use std::ops::Range;
use std::path::{Path, PathBuf};

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::Editor;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::history::{Details, Entry};
use crate::log::LogView;
use crate::pane::{self, Layout};
use crate::status::{self, Line, Rendered, Row, Section};
use crate::view::{self, GitState};

const WIDTH: usize = 100;

#[derive(Debug)]
pub(crate) struct RevisionPane {
    scope: ScopeId,
    pub(crate) root: PathBuf,
    pub(crate) layout: Layout,
    pub(crate) width: usize,
    pub(crate) rendered: Rendered,
}

impl AppletBuffer for RevisionPane {
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

pub(crate) fn render(entry: &Entry, details: &Details, layout: Layout, width: usize) -> Rendered {
    let mut out = Rendered::default();
    let mut line = Line::default();
    line.push(&entry.short, Some(Highlight::Constant));
    line.push("  ", None);
    line.push(&entry.summary, Some(Highlight::Heading));
    out.add(Row::Info, line);
    if details.files.is_empty() {
        out.add(Row::Blank, Line::default());
        let mut line = Line::default();
        line.push("No changes", Some(Highlight::Comment));
        out.add(Row::Blank, line);
    }
    for (index, (change, diff)) in details.files.iter().enumerate() {
        out.add(Row::Blank, Line::default());
        let mut line = Line::default();
        let (letter, face) = status::letter(Section::Revision, change.kind);
        line.pill(letter, face);
        line.push("  ", None);
        line.push(&change.path, Some(Highlight::Heading));
        if let Some(from) = &change.from {
            line.push(&format!(" ← {from}"), Some(Highlight::Comment));
        }
        line.push("  ", None);
        status::stats(&mut line, diff);
        out.add(Row::File(Section::Revision, index), line);
        pane::hunks(&mut out, Section::Revision, index, diff, layout, width);
    }
    out
}

fn pane_of(editor: &Editor, root: &Path) -> Option<BufferId> {
    editor.find_applet_buffer::<RevisionPane>(|pane| pane.root == root)
}

fn pane_for(editor: &mut Editor, root: &Path) -> Option<BufferId> {
    if let Some(id) = pane_of(editor, root) {
        return Some(id);
    }
    let scope = editor
        .applet_state::<GitState>()
        .map(|state| state.revision_scope)?;
    let name = format!("git revision: {}", view::folder_name(root));
    let root = root.to_owned();
    Some(editor.add_applet_buffer_with(&name, |buffer| {
        buffer.set_read_only(true);
        RevisionPane {
            scope,
            root,
            layout: Layout::Unified,
            width: WIDTH,
            rendered: Rendered::default(),
        }
    }))
}

pub(crate) fn redraw(editor: &mut Editor, pane_id: BufferId) {
    let width = editor.buffer_width(pane_id).unwrap_or(WIDTH);
    let Some((_, pane)) = editor.applet_buffer::<RevisionPane>(pane_id) else {
        return;
    };
    let root = pane.root.clone();
    let layout = pane.layout;
    let Some(log_id) = editor.find_applet_buffer::<LogView>(|view| view.root == root) else {
        return;
    };
    let Some((_, view)) = editor.applet_buffer::<LogView>(log_id) else {
        return;
    };
    let rendered = match (view.selected_entry(), &view.details) {
        (Some(entry), Some(details)) => render(entry, details, layout, width),
        _ => Rendered::default(),
    };
    let Some((buffer, pane)) = editor.applet_buffer_mut::<RevisionPane>(pane_id) else {
        return;
    };
    let line = buffer.line_of(buffer.point());
    pane.width = width;
    pane.rendered = rendered;
    buffer.set_text(&pane.rendered.text);
    let last = pane.rendered.rows.len().saturating_sub(1);
    let start = buffer.line_start(line.min(last));
    buffer.set_point(start);
}

pub(crate) fn show(editor: &mut Editor) {
    let id = editor.buffer().id();
    let Some(root) = editor
        .applet_buffer::<LogView>(id)
        .map(|(_, view)| view.root.clone())
    else {
        return;
    };
    let Some(pane_id) = pane_for(editor, &root) else {
        return;
    };
    if let Some((buffer, _)) = editor.applet_buffer_mut::<RevisionPane>(pane_id) {
        buffer.set_point(0);
    }
    if !editor.buffer_shown(pane_id) {
        let origin = editor.selected_window();
        editor.show_buffer_beside(pane_id);
        editor.select_window(origin);
    }
    redraw(editor, pane_id);
}

pub(crate) fn follow(editor: &mut Editor, root: &Path) {
    if let Some(pane_id) = pane_of(editor, root)
        && editor.buffer_shown(pane_id)
    {
        if let Some((buffer, _)) = editor.applet_buffer_mut::<RevisionPane>(pane_id) {
            buffer.set_point(0);
        }
        redraw(editor, pane_id);
    }
}

pub(crate) fn fit(editor: &mut Editor) {
    for id in editor.applet_buffers::<RevisionPane>() {
        let Some(width) = editor.buffer_width(id) else {
            continue;
        };
        let stale = editor
            .applet_buffer::<RevisionPane>(id)
            .is_some_and(|(_, pane)| pane.layout == Layout::Split && pane.width != width);
        if stale {
            redraw(editor, id);
        }
    }
}

pub(crate) fn toggle_layout(editor: &mut Editor) {
    let id = editor.buffer().id();
    let Some((_, pane)) = editor.applet_buffer_mut::<RevisionPane>(id) else {
        return;
    };
    pane.layout = match pane.layout {
        Layout::Unified => Layout::Split,
        Layout::Split => Layout::Unified,
    };
    redraw(editor, id);
}

pub(crate) fn step(editor: &mut Editor, forward: bool) {
    let id = editor.buffer().id();
    let Some((buffer, pane)) = editor.applet_buffer_mut::<RevisionPane>(id) else {
        return;
    };
    let line = buffer.line_of(buffer.point());
    let rows = &pane.rendered.rows;
    let item = |at: &usize| matches!(rows[*at], Row::File(..) | Row::Hunk(..));
    let found = if forward {
        (line + 1..rows.len()).find(item)
    } else {
        (0..line).rev().find(item)
    };
    if let Some(found) = found {
        let start = buffer.line_start(found);
        buffer.set_point(start);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::Editor;

    use super::*;
    use crate::repo::fixtures::{git, repository};
    use crate::testing::{at_point, installed, press};

    fn logged() -> (tempfile::TempDir, Editor) {
        let dir = repository();
        fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();
        fs::write(dir.path().join("c.txt"), "new\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(
            dir.path(),
            &["commit", "-q", "-m", "Shout\n\nAnd add a file"],
        );
        let mut editor = installed(dir.path());
        let window = editor.selected_window();
        editor.resize_pane(window, 30, 160);
        press(&mut editor, "C-x g l l");
        (dir, editor)
    }

    fn pane_text(editor: &Editor) -> String {
        let id = editor.find_applet_buffer::<RevisionPane>(|_| true).unwrap();
        editor.buffer_by_id(id).unwrap().contents()
    }

    #[test]
    fn return_shows_the_commit_beside_the_log() {
        let (dir, mut editor) = logged();

        press(&mut editor, "RET");

        assert!(editor.buffer().name().starts_with("git log: "));
        let id = editor.find_applet_buffer::<RevisionPane>(|_| true).unwrap();
        assert!(editor.buffer_shown(id));
        let name = dir.path().file_name().unwrap().to_string_lossy();
        assert_eq!(
            editor.buffer_by_id(id).unwrap().name(),
            format!("git revision: {name}")
        );
        let shown = pane_text(&editor);
        let lines: Vec<&str> = shown.lines().collect();
        assert!(lines[0].ends_with("  Shout"), "{lines:?}");
        assert_eq!(lines[2], "M  a.txt  +1 -1");
        assert!(lines.contains(&"@@ -1,3 +1,3 @@"), "{lines:?}");
        assert!(lines.contains(&"A  c.txt  +1"), "{lines:?}");
    }

    #[test]
    fn the_revision_follows_the_commit_at_point() {
        let (_dir, mut editor) = logged();
        press(&mut editor, "RET");

        press(&mut editor, "n");

        let shown = pane_text(&editor);
        assert!(shown.lines().next().unwrap().ends_with("  First commit"));
        assert!(shown.contains("A  b.txt  +1"), "{shown}");
    }

    #[test]
    fn t_shows_the_revision_side_by_side_and_n_moves_between_hunks() {
        let (_dir, mut editor) = logged();
        press(&mut editor, "RET C-x o");

        press(&mut editor, "t");
        assert!(pane_text(&editor).contains(" │ "));

        press(&mut editor, "n");
        assert_eq!(at_point(&editor), "M  a.txt  +1 -1");
        press(&mut editor, "n");
        assert_eq!(at_point(&editor), "@@ -1,3 +1,3 @@");
    }

    #[test]
    fn q_leaves_the_revision() {
        let (_dir, mut editor) = logged();
        press(&mut editor, "RET C-x o q");

        assert!(!editor.buffer().name().starts_with("git revision: "));
    }
}
