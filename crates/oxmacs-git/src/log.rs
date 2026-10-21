use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use gix::ObjectId;
use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::{Action, Editor, Group, Offer, Picked, Transient};
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::graph::{GraphRow, Lanes};
use crate::history::{self, Details, Entry, Label, LabelKind, Tips};
use crate::revision;
use crate::status::{self, Line, Rendered, Row, Section};
use crate::view::{self, GitState};
use crate::widget::{LogClick, LogScroll, LogViewport};

pub(crate) const PAGE: usize = 1000;
const SUBJECT: usize = 72;
const FILE_COLUMN: usize = 60;
const MARGIN: usize = 100;

pub(crate) static LOG: Transient = Transient {
    title: "Log",
    groups: &[Group {
        title: "Log",
        switches: &[],
        actions: &[
            Action {
                key: 'l',
                summary: "Current branch",
                run: |editor, _| open(editor, Tips::Head),
            },
            Action {
                key: 'a',
                summary: "All branches",
                run: |editor, _| open(editor, Tips::All),
            },
            Action {
                key: 'o',
                summary: "Other branch",
                run: |editor, _| other(editor),
            },
        ],
    }],
};

#[derive(Debug)]
pub(crate) struct LogView {
    scope: ScopeId,
    pub(crate) root: PathBuf,
    pub(crate) tips: Tips,
    pub(crate) entries: Vec<Entry>,
    pub(crate) graph: Vec<GraphRow>,
    lanes: Lanes<ObjectId>,
    pub(crate) labels: HashMap<ObjectId, Vec<Label>>,
    pub(crate) complete: bool,
    pub(crate) page: usize,
    pub(crate) rendered: Rendered,
    pub(crate) selected: Option<ObjectId>,
    pub(crate) details: Option<Details>,
    pub(crate) about: Rendered,
    pub(crate) top: usize,
    pub(crate) shown: usize,
}

impl LogView {
    pub(crate) fn selected_entry(&self) -> Option<&Entry> {
        let selected = self.selected?;
        self.entries.iter().find(|entry| entry.id == selected)
    }
}

impl AppletBuffer for LogView {
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

pub(crate) fn face(kind: LabelKind) -> Highlight {
    match kind {
        LabelKind::Head => Highlight::Link,
        LabelKind::Local => Highlight::Success,
        LabelKind::Remote => Highlight::Function,
        LabelKind::Tag => Highlight::Constant,
    }
}

fn labels_width(labels: &[Label]) -> usize {
    labels
        .iter()
        .map(|label| label.name.chars().count() + 1)
        .sum()
}

pub(crate) fn render(
    entries: &[Entry],
    labels: &HashMap<ObjectId, Vec<Label>>,
    now: i64,
) -> Rendered {
    let none = Vec::new();
    let of = |entry: &Entry| labels.get(&entry.id).unwrap_or(&none);
    let subject = entries
        .iter()
        .map(|entry| labels_width(of(entry)) + entry.summary.chars().count())
        .max()
        .unwrap_or(0)
        .min(SUBJECT);
    let author_width = entries
        .iter()
        .map(|entry| entry.author.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = Rendered::default();
    for (index, entry) in entries.iter().enumerate() {
        let mut line = Line::default();
        line.push(&entry.short, Some(Highlight::Constant));
        line.push("  ", None);
        let start = line.width();
        for label in of(entry) {
            line.pill(&label.name, face(label.kind));
            line.push(" ", None);
        }
        let room = subject.saturating_sub(line.width() - start);
        line.push(&status::shorten(&entry.summary, room), None);
        line.pad(start + subject + 2);
        let column = line.width();
        line.push(&entry.author, Some(status::author_color(&entry.author)));
        line.pad(column + author_width + 2);
        line.push(&status::ago(entry.time, now), Some(Highlight::Comment));
        out.add(Row::Commit(index), line);
    }
    out
}

pub(crate) fn current_log(editor: &Editor) -> Option<(BufferId, &LogView)> {
    let id = editor.buffer().id();
    editor
        .applet_buffer::<LogView>(id)
        .map(|(_, view)| (id, view))
}

fn root_here(editor: &Editor) -> Option<PathBuf> {
    if let Some((_, view)) = current_log(editor) {
        return Some(view.root.clone());
    }
    view::current(editor).map(|(_, view)| view.root.clone())
}

pub(crate) fn dispatch(editor: &mut Editor) {
    if view::current(editor).is_some() {
        editor.open_transient(&LOG);
    }
}

fn buffer_for(editor: &mut Editor, root: &Path, tips: &Tips) -> Option<BufferId> {
    if let Some(id) = editor.find_applet_buffer::<LogView>(|view| view.root == root) {
        return Some(id);
    }
    let scope = editor
        .applet_state::<GitState>()
        .map(|state| state.log_scope)?;
    let name = format!("git log: {}", view::folder_name(root));
    let root = root.to_owned();
    let tips = tips.clone();
    Some(editor.add_applet_buffer_with(&name, |buffer| {
        buffer.set_read_only(true);
        LogView {
            scope,
            root,
            tips,
            entries: Vec::new(),
            graph: Vec::new(),
            lanes: Lanes::default(),
            labels: HashMap::new(),
            complete: true,
            page: PAGE,
            rendered: Rendered::default(),
            selected: None,
            details: None,
            about: Rendered::default(),
            top: 0,
            shown: 0,
        }
    }))
}

pub(crate) fn open(editor: &mut Editor, tips: Tips) {
    let Some(root) = root_here(editor) else {
        return;
    };
    let Some(id) = buffer_for(editor, &root, &tips) else {
        return;
    };
    let Some(page) = editor.applet_buffer_mut::<LogView>(id).map(|(_, view)| {
        view.tips = tips;
        view.page
    }) else {
        return;
    };
    if load(editor, id, page, None) {
        editor.show_buffer(id);
    }
}

fn load(editor: &mut Editor, id: BufferId, count: usize, keep: Option<ObjectId>) -> bool {
    let Some((_, view)) = editor.applet_buffer::<LogView>(id) else {
        return false;
    };
    let root = view.root.clone();
    let entries = match history::log(&root, &view.tips, 0, count) {
        Ok(entries) => entries,
        Err(error) => {
            editor.show_message(error);
            return false;
        }
    };
    let labels = history::labels(&root).unwrap_or_default();
    let now = view::now();
    let Some((buffer, view)) = editor.applet_buffer_mut::<LogView>(id) else {
        return false;
    };
    view.complete = entries.len() < count;
    view.lanes = Lanes::default();
    view.graph = entries
        .iter()
        .map(|entry| view.lanes.place(&entry.id, &entry.parents))
        .collect();
    view.rendered = render(&entries, &labels, now);
    view.labels = labels;
    view.entries = entries;
    buffer.set_text(&view.rendered.text);
    let line = keep
        .and_then(|kept| view.entries.iter().position(|entry| entry.id == kept))
        .unwrap_or(0);
    let start = buffer.line_start(line);
    buffer.set_point(start);
    view.top = visible_top(view.top, view.shown, line);
    select(editor, id, line);
    true
}

pub(crate) fn visible_top(top: usize, shown: usize, line: usize) -> usize {
    if shown == 0 || line < top {
        return line.min(top);
    }
    if line >= top + shown {
        return line + 1 - shown;
    }
    top
}

fn reveal(editor: &mut Editor, id: BufferId, line: usize) {
    if let Some((_, view)) = editor.applet_buffer_mut::<LogView>(id) {
        view.top = visible_top(view.top, view.shown, line);
    }
}

pub(crate) fn viewport(editor: &mut Editor, event: &LogViewport) {
    let Some((buffer, view)) = editor.applet_buffer_mut::<LogView>(event.buffer) else {
        return;
    };
    view.shown = event.rows;
    view.top = visible_top(view.top, view.shown, buffer.line_of(buffer.point()));
}

fn move_to(editor: &mut Editor, id: BufferId, line: usize) {
    let Some(start) = editor
        .buffer_by_id(id)
        .map(|buffer| buffer.line_start(line))
    else {
        return;
    };
    if let Some(buffer) = editor.buffer_by_id_mut(id) {
        buffer.set_point(start);
    }
    editor.move_points(id, |_, _| Some(start));
    settle(editor, id, line);
}

pub(crate) fn scrolled(editor: &mut Editor, event: &LogScroll) {
    let Some((buffer, view)) = editor.applet_buffer_mut::<LogView>(event.buffer) else {
        return;
    };
    let last = view.entries.len().saturating_sub(1);
    let moved = i64::try_from(view.top)
        .unwrap_or(i64::MAX)
        .saturating_add(event.rows)
        .max(0);
    view.top = usize::try_from(moved).unwrap_or(0).min(last);
    let line = buffer.line_of(buffer.point());
    let bottom = (view.top + view.shown.max(1) - 1).min(last).max(view.top);
    let target = line.clamp(view.top, bottom);
    let near = near_end(view, view.top + view.shown);
    if target != line {
        move_to(editor, event.buffer, target);
    } else if near {
        more(editor, event.buffer);
    }
}

pub(crate) fn clicked(editor: &mut Editor, event: &LogClick) {
    let Some((_, view)) = editor.applet_buffer::<LogView>(event.buffer) else {
        return;
    };
    let line = view.top + event.row;
    if line < view.entries.len() {
        move_to(editor, event.buffer, line);
    }
}

pub(crate) fn about(entry: &Entry, details: &Details, now: i64) -> Rendered {
    let mut out = Rendered::default();
    let mut line = Line::default();
    line.push(&entry.short, Some(Highlight::Constant));
    line.push("  ", None);
    line.push(&details.author, Some(status::author_color(&details.author)));
    line.push(&format!(" <{}>", details.email), Some(Highlight::Comment));
    line.push("  ", None);
    line.push(&status::ago(details.time, now), Some(Highlight::Comment));
    out.add(Row::Info, line);
    out.add(Row::Blank, Line::default());
    for (number, text) in details.message.lines().enumerate() {
        let mut line = Line::default();
        line.push(text, (number == 0).then_some(Highlight::Heading));
        out.add(Row::Info, line);
    }
    out.add(Row::Blank, Line::default());
    let mut line = Line::default();
    line.push(
        &format!("Changed files ({})", details.files.len()),
        Some(Highlight::Heading),
    );
    out.add(Row::Info, line);
    let width = details
        .files
        .iter()
        .map(|(change, _)| change.path.chars().count())
        .max()
        .unwrap_or(0)
        .min(FILE_COLUMN);
    for (change, diff) in &details.files {
        let mut line = Line::default();
        let (letter, face) = status::letter(Section::Revision, change.kind);
        line.push("  ", None);
        line.pill(letter, face);
        line.push("  ", None);
        line.push(&change.path, None);
        if let Some(from) = &change.from {
            line.push(&format!(" ← {from}"), Some(Highlight::Comment));
        }
        line.pad(5 + width + 2);
        status::stats(&mut line, diff);
        out.add(Row::Info, line);
    }
    out
}

fn select(editor: &mut Editor, id: BufferId, line: usize) {
    let Some((_, view)) = editor.applet_buffer::<LogView>(id) else {
        return;
    };
    let Some(entry) = view.entries.get(line) else {
        return;
    };
    if view.selected == Some(entry.id) {
        return;
    }
    let root = view.root.clone();
    let commit = entry.id;
    let details = history::details(&root, commit).ok();
    let shown = details
        .as_ref()
        .map(|details| about(entry, details, view::now()))
        .unwrap_or_default();
    let Some((_, view)) = editor.applet_buffer_mut::<LogView>(id) else {
        return;
    };
    view.selected = Some(commit);
    view.details = details;
    view.about = shown;
    revision::follow(editor, &root);
}

pub(crate) fn follow(editor: &mut Editor) {
    let Some((id, _)) = current_log(editor) else {
        return;
    };
    let buffer = editor.buffer();
    let line = buffer.line_of(buffer.point());
    settle(editor, id, line);
}

fn settle(editor: &mut Editor, id: BufferId, line: usize) {
    let near = editor
        .applet_buffer::<LogView>(id)
        .is_some_and(|(_, view)| near_end(view, line));
    if near {
        more(editor, id);
    }
    reveal(editor, id, line);
    select(editor, id, line);
}

fn near_end(view: &LogView, line: usize) -> bool {
    !view.complete && line + MARGIN >= view.entries.len()
}

fn more(editor: &mut Editor, id: BufferId) {
    let Some((_, view)) = editor.applet_buffer::<LogView>(id) else {
        return;
    };
    let page = view.page;
    let fresh = history::log(&view.root, &view.tips, view.entries.len(), page).unwrap_or_default();
    let now = view::now();
    let Some((buffer, view)) = editor.applet_buffer_mut::<LogView>(id) else {
        return;
    };
    let point = buffer.point();
    view.complete = fresh.len() < page;
    for entry in &fresh {
        let row = view.lanes.place(&entry.id, &entry.parents);
        view.graph.push(row);
    }
    view.entries.extend(fresh);
    view.rendered = render(&view.entries, &view.labels, now);
    buffer.set_text(&view.rendered.text);
    buffer.set_point(point);
}

fn other(editor: &mut Editor) {
    let Some(root) = root_here(editor) else {
        return;
    };
    let (mut names, remote) = history::branch_names(&root);
    names.extend(remote);
    let offered = names.clone();
    editor.prompt_pick(
        "Log branch: ",
        move |_| {
            offered
                .iter()
                .map(|name| Offer::from(name.as_str()))
                .collect()
        },
        move |editor, picked| {
            let name = match picked {
                Picked::Offered(index) => names.get(index).cloned().unwrap_or_default(),
                Picked::Typed(text) => text.trim().to_owned(),
            };
            if !name.is_empty() {
                open(editor, Tips::Branch(name));
            }
        },
    );
}

pub(crate) fn step(editor: &mut Editor, forward: bool) {
    if current_log(editor).is_none() {
        return;
    }
    let buffer = editor.buffer_mut();
    let line = buffer.line_of(buffer.point());
    let last = buffer.line_of(buffer.len_chars());
    let target = if forward {
        (line + 1).min(last)
    } else {
        line.saturating_sub(1)
    };
    let start = buffer.line_start(target);
    buffer.set_point(start);
}

pub(crate) fn refresh(editor: &mut Editor) {
    let Some((id, view)) = current_log(editor) else {
        return;
    };
    let buffer = editor.buffer();
    let keep = view
        .entries
        .get(buffer.line_of(buffer.point()))
        .map(|entry| entry.id);
    let count = view.entries.len().max(view.page);
    load(editor, id, count, keep);
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::Editor;

    use oxmacs_core::applet::Payload;

    use super::*;
    use crate::diff::hunks;
    use crate::repo::fixtures::{git, repository};
    use crate::repo::{Change, FileDiff, Kind};
    use crate::testing::{at_point, installed, lines, press, type_text};

    fn view(editor: &Editor) -> &LogView {
        let id = editor.buffer().id();
        editor.applet_buffer::<LogView>(id).unwrap().1
    }

    fn entry(short: &str, summary: &str, author: &str) -> Entry {
        Entry {
            id: ObjectId::from_hex(format!("{short:0>40}").as_bytes()).unwrap(),
            parents: Vec::new(),
            short: short.to_owned(),
            summary: summary.to_owned(),
            author: author.to_owned(),
            time: 0,
        }
    }

    #[test]
    fn rows_put_labels_before_the_summary_and_line_up_authors() {
        let entries = vec![
            entry("aaaaaaa", "Short", "Ada"),
            entry("bbbbbbb", "A longer summary", "Grace"),
        ];
        let mut labels = HashMap::new();
        labels.insert(
            entries[0].id,
            vec![Label {
                name: "main".to_owned(),
                kind: LabelKind::Head,
            }],
        );

        let rendered = render(&entries, &labels, 30);

        assert_eq!(
            rendered.text.split('\n').collect::<Vec<_>>(),
            [
                "aaaaaaa  main Short        Ada    just now",
                "bbbbbbb  A longer summary  Grace  just now",
            ]
        );
        assert_eq!(rendered.pills[0], [(9..13, Highlight::Link)]);
        assert_eq!(rendered.rows, [Row::Commit(0), Row::Commit(1)]);
    }

    #[test]
    fn details_show_the_author_message_and_changed_files() {
        let details = Details {
            message: "Shout\n\nAnd add a file".to_owned(),
            author: "Ada".to_owned(),
            email: "ada@example.com".to_owned(),
            time: 0,
            files: vec![
                (
                    Change {
                        path: "a.txt".to_owned(),
                        kind: Kind::Modified,
                        from: None,
                    },
                    FileDiff {
                        hunks: hunks("one\n", "ONE\n", 3),
                        ..FileDiff::default()
                    },
                ),
                (
                    Change {
                        path: "c.txt".to_owned(),
                        kind: Kind::Added,
                        from: None,
                    },
                    FileDiff {
                        hunks: hunks("", "new\n", 3),
                        ..FileDiff::default()
                    },
                ),
            ],
        };

        let rendered = about(&entry("abc1234", "Shout", "Ada"), &details, 30);

        assert_eq!(
            rendered.text.split('\n').collect::<Vec<_>>(),
            [
                "abc1234  Ada <ada@example.com>  just now",
                "",
                "Shout",
                "",
                "And add a file",
                "",
                "Changed files (2)",
                "  M  a.txt  +1 -1",
                "  A  c.txt  +1",
            ]
        );
        assert_eq!(rendered.pills[7], [(2..3, Highlight::Warning)]);
        assert_eq!(rendered.styles[2].spans, [(0..5, Highlight::Heading)]);
    }

    #[test]
    fn the_commit_at_point_shows_its_details() {
        let (_dir, mut editor) = branched();

        press(&mut editor, "l l");
        let shown = view(&editor).about.text.clone();
        assert!(shown.contains("Ada Lovelace <ada@example.com>"), "{shown}");
        assert!(shown.contains("\nMain work\n"), "{shown}");
        assert!(shown.ends_with("\n  A  m.txt  +1"), "{shown}");

        press(&mut editor, "n");
        assert!(view(&editor).about.text.contains("\nFirst commit\n"));
    }

    #[test]
    fn each_kind_of_label_has_its_own_color() {
        assert_eq!(face(LabelKind::Head), Highlight::Link);
        assert_eq!(face(LabelKind::Local), Highlight::Success);
        assert_eq!(face(LabelKind::Remote), Highlight::Function);
        assert_eq!(face(LabelKind::Tag), Highlight::Constant);
    }

    fn commit(dir: &Path, file: &str, text: &str, message: &str) {
        fs::write(dir.join(file), text).unwrap();
        git(dir, &["add", file]);
        git(dir, &["commit", "-q", "-m", message]);
    }

    fn branched() -> (tempfile::TempDir, Editor) {
        let dir = repository();
        git(dir.path(), &["checkout", "-q", "-b", "feature"]);
        commit(dir.path(), "f.txt", "f\n", "Feature work");
        git(dir.path(), &["checkout", "-q", "main"]);
        commit(dir.path(), "m.txt", "m\n", "Main work");
        let mut editor = installed(dir.path());
        press(&mut editor, "C-x g");
        (dir, editor)
    }

    #[test]
    fn l_opens_the_log_menu() {
        let (_dir, mut editor) = branched();

        press(&mut editor, "l");

        let menu = editor.transient().unwrap();
        assert_eq!(menu.title, "Log");
        let keys: Vec<String> = menu
            .groups
            .iter()
            .flat_map(|group| group.entries.iter().map(|entry| entry.key.clone()))
            .collect();
        assert_eq!(keys, ["l", "a", "o"]);
    }

    #[test]
    fn l_l_logs_the_current_branch_with_point_on_the_newest_commit() {
        let (dir, mut editor) = branched();

        press(&mut editor, "l l");

        let name = dir.path().file_name().unwrap().to_string_lossy();
        assert_eq!(editor.buffer().name(), format!("git log: {name}"));
        assert!(editor.buffer().is_read_only());
        let shown = lines(&editor);
        assert_eq!(shown.len(), 2);
        assert!(shown[0].contains("  main Main work  "), "{shown:?}");
        assert!(shown[0].ends_with("Ada Lovelace  just now"), "{shown:?}");
        assert!(shown[1].contains("  First commit  "), "{shown:?}");
        assert!(at_point(&editor).contains("Main work"));
        assert_eq!(editor.default_directory(), dir.path());
    }

    #[test]
    fn l_a_logs_every_branch() {
        let (_dir, mut editor) = branched();

        press(&mut editor, "l a");

        let shown = lines(&editor);
        assert_eq!(shown.len(), 3);
        assert!(
            shown
                .iter()
                .any(|line| line.contains("feature Feature work")),
            "{shown:?}"
        );
    }

    #[test]
    fn l_o_asks_for_the_branch_to_log() {
        let (_dir, mut editor) = branched();

        press(&mut editor, "l o");
        type_text(&mut editor, "feature");
        press(&mut editor, "RET");

        let shown = lines(&editor);
        assert!(shown[0].contains("Feature work"), "{shown:?}");
        assert!(
            !shown.iter().any(|line| line.contains("Main work")),
            "{shown:?}"
        );
    }

    #[test]
    fn n_and_p_move_between_commits() {
        let (_dir, mut editor) = branched();
        press(&mut editor, "l a");

        press(&mut editor, "n n");
        assert!(at_point(&editor).contains("First commit"));

        press(&mut editor, "n");
        assert!(at_point(&editor).contains("First commit"));

        press(&mut editor, "p");
        assert!(!at_point(&editor).contains("First commit"));
    }

    #[test]
    fn g_reads_the_log_again_keeping_point_on_its_commit() {
        let (dir, mut editor) = branched();
        press(&mut editor, "l l n");
        commit(dir.path(), "n.txt", "n\n", "Newer work");

        press(&mut editor, "g");

        let shown = lines(&editor);
        assert_eq!(shown.len(), 3);
        assert!(shown[0].contains("Newer work"), "{shown:?}");
        assert!(at_point(&editor).contains("First commit"));
    }

    #[test]
    fn q_leaves_the_log() {
        let (_dir, mut editor) = branched();
        press(&mut editor, "l l");

        press(&mut editor, "q");

        assert!(editor.buffer().name().starts_with("git: "));
    }

    #[test]
    fn opening_the_log_again_reuses_its_buffer() {
        let (_dir, mut editor) = branched();
        press(&mut editor, "l l q l a");

        assert_eq!(lines(&editor).len(), 3);
        assert_eq!(
            editor
                .buffer_names()
                .iter()
                .filter(|name| name.starts_with("git log: "))
                .count(),
            1
        );
    }

    #[test]
    fn every_branch_takes_a_lane_of_its_own() {
        let (_dir, mut editor) = branched();

        press(&mut editor, "l a");

        let graph = &view(&editor).graph;
        assert_eq!(graph.len(), 3);
        assert_eq!(graph[0].lane, 0);
        assert_eq!(graph[1].lane, 1);
        assert_eq!(graph[2].above, [0, 1]);
    }

    #[test]
    fn the_top_row_keeps_point_in_view() {
        assert_eq!(visible_top(0, 10, 5), 0);
        assert_eq!(visible_top(0, 10, 12), 3);
        assert_eq!(visible_top(8, 10, 3), 3);
        assert_eq!(visible_top(4, 0, 9), 4);
    }

    fn long() -> (tempfile::TempDir, Editor) {
        let dir = repository();
        for number in 1..=30 {
            git(
                dir.path(),
                &[
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    &format!("Change {number}"),
                ],
            );
        }
        let mut editor = installed(dir.path());
        press(&mut editor, "C-x g l l");
        (dir, editor)
    }

    #[test]
    fn more_commits_load_near_the_end() {
        let (_dir, mut editor) = long();
        let id = editor.buffer().id();
        if let Some((_, view)) = editor.applet_buffer_mut::<LogView>(id) {
            view.page = 10;
        }
        press(&mut editor, "q l l");
        assert_eq!(view(&editor).entries.len(), 10);
        assert!(!view(&editor).complete);

        press(&mut editor, "M->");
        assert_eq!(view(&editor).entries.len(), 20);
        assert_eq!(view(&editor).graph.len(), 20);
        assert_eq!(lines(&editor).len(), 20);
        assert_eq!(line(&editor), 9);

        press(&mut editor, "M-> M->");
        assert_eq!(view(&editor).entries.len(), 31);
        assert!(view(&editor).complete);
        assert!(lines(&editor)[30].contains("First commit"));
    }

    fn line(editor: &Editor) -> usize {
        let buffer = editor.buffer();
        buffer.line_of(buffer.point())
    }

    #[test]
    fn the_window_scrolls_and_clicks_select_commits() {
        let (_dir, mut editor) = long();
        let id = editor.buffer().id();

        editor.deliver(&Payload::new(LogViewport {
            buffer: id,
            rows: 10,
        }));
        assert_eq!(view(&editor).shown, 10);

        editor.deliver(&Payload::new(LogScroll {
            buffer: id,
            rows: 6,
        }));
        assert_eq!(view(&editor).top, 6);
        assert_eq!(line(&editor), 6);
        assert!(view(&editor).about.text.contains("\nChange 24\n"));

        editor.deliver(&Payload::new(LogClick { buffer: id, row: 2 }));
        assert_eq!(line(&editor), 8);
        assert!(view(&editor).about.text.contains("\nChange 22\n"));

        press(&mut editor, "M->");
        assert_eq!(view(&editor).top, 21);
    }
}
