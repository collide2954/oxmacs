use std::ops::Range;
use std::path::{Path, PathBuf};

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::Editor;
use oxmacs_core::text::{Buffer, BufferId};
use oxmacs_core::theme::Highlight;

use crate::repo::{self, Upstream};
use crate::run::{self, Plan};
use crate::status::{Line, Rendered, Row};
use crate::view::{self, GitState};

const RECENT: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Local {
    pub(crate) name: String,
    pub(crate) current: bool,
    pub(crate) upstream: Option<Upstream>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Branches {
    pub(crate) local: Vec<Local>,
    pub(crate) remote: Vec<String>,
    pub(crate) recent: Vec<String>,
    pub(crate) tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
    Local,
    Remote,
    Recent,
    Tags,
}

impl Group {
    fn title(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Remote => "Remote",
            Self::Recent => "Recent",
            Self::Tags => "Tags",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Item {
    Info,
    Heading(Group),
    Branch(Group, usize),
}

impl Branches {
    fn names(&self, group: Group) -> Vec<&str> {
        match group {
            Group::Local => self.local.iter().map(|local| local.name.as_str()).collect(),
            Group::Remote => self.remote.iter().map(String::as_str).collect(),
            Group::Recent => self.recent.iter().map(String::as_str).collect(),
            Group::Tags => self.tags.iter().map(String::as_str).collect(),
        }
    }

    pub(crate) fn name(&self, group: Group, index: usize) -> Option<&str> {
        self.names(group).get(index).copied()
    }
}

fn matches(name: &str, filter: &str) -> bool {
    name.to_lowercase().contains(&filter.trim().to_lowercase())
}

pub(crate) fn render(branches: &Branches, filter: &str) -> (Rendered, Vec<Item>) {
    let mut out = Rendered::default();
    let mut items = Vec::new();
    let mut add = |out: &mut Rendered, item: Item, line: Line| {
        out.add(Row::Info, line);
        items.push(item);
    };
    if !filter.trim().is_empty() {
        let mut line = Line::default();
        line.push("Filter: ", Some(Highlight::Comment));
        line.push(filter.trim(), Some(Highlight::Keyword));
        add(&mut out, Item::Info, line);
    }
    let width = branches
        .local
        .iter()
        .map(|local| local.name.chars().count())
        .max()
        .unwrap_or(0);
    let mut any = false;
    for group in [Group::Local, Group::Remote, Group::Recent, Group::Tags] {
        let shown: Vec<(usize, &str)> = branches
            .names(group)
            .into_iter()
            .enumerate()
            .filter(|(_, name)| matches(name, filter))
            .collect();
        if shown.is_empty() {
            continue;
        }
        if !out.rows.is_empty() {
            add(&mut out, Item::Info, Line::default());
        }
        any = true;
        let mut line = Line::default();
        line.push(group.title(), Some(Highlight::Heading));
        line.push(&format!(" ({})", shown.len()), Some(Highlight::Comment));
        add(&mut out, Item::Heading(group), line);
        for (index, name) in shown {
            let mut line = Line::default();
            if let Some(local) = branches.local.get(index).filter(|_| group == Group::Local) {
                local_line(&mut line, local, width);
            } else {
                line.push("    ", None);
                line.push(name, None);
            }
            add(&mut out, Item::Branch(group, index), line);
        }
    }
    if !any {
        if !out.rows.is_empty() {
            add(&mut out, Item::Info, Line::default());
        }
        let mut line = Line::default();
        line.push("No branches match", Some(Highlight::Comment));
        add(&mut out, Item::Info, line);
    }
    (out, items)
}

fn local_line(line: &mut Line, local: &Local, width: usize) {
    if local.current {
        line.push("  ", None);
        line.push("●", Some(Highlight::Success));
        line.push(" ", None);
        line.pill(&local.name, Highlight::Link);
    } else {
        line.push("    ", None);
        line.push(&local.name, None);
    }
    let Some(upstream) = &local.upstream else {
        return;
    };
    line.pad(4 + width + 2);
    line.pill(&upstream.name, Highlight::Function);
    line.push("  ", None);
    let ahead = if upstream.ahead > 0 {
        Highlight::Success
    } else {
        Highlight::Comment
    };
    let behind = if upstream.behind > 0 {
        Highlight::Warning
    } else {
        Highlight::Comment
    };
    line.push(&format!("↑{}", upstream.ahead), Some(ahead));
    line.push(" ", None);
    line.push(&format!("↓{}", upstream.behind), Some(behind));
}

fn failed(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn names<'repo, E>(
    references: impl Iterator<Item = Result<gix::Reference<'repo>, E>>,
) -> Vec<String> {
    let mut names: Vec<String> = references
        .filter_map(Result::ok)
        .filter(|reference| !reference.name().as_bstr().ends_with(b"/HEAD"))
        .map(|reference| reference.name().shorten().to_string())
        .collect();
    names.sort();
    names
}

pub(crate) fn read(root: &Path) -> Result<Branches, String> {
    let repository = gix::open(root).map_err(failed)?;
    let current = repository
        .head_name()
        .ok()
        .flatten()
        .map(|name| name.shorten().to_string());
    let references = repository.references().map_err(failed)?;
    let mut local: Vec<Local> = references
        .local_branches()
        .map_err(failed)?
        .filter_map(Result::ok)
        .map(|reference| {
            let name = reference.name().shorten().to_string();
            Local {
                current: current.as_deref() == Some(name.as_str()),
                upstream: repo::upstream_of(&repository, reference.name()),
                name,
            }
        })
        .collect();
    local.sort_by(|a, b| a.name.cmp(&b.name));
    let recent = recent(&repository, &local);
    Ok(Branches {
        remote: names(references.remote_branches().map_err(failed)?),
        tags: names(references.tags().map_err(failed)?),
        local,
        recent,
    })
}

fn recent(repository: &gix::Repository, local: &[Local]) -> Vec<String> {
    let Ok(head) = repository.head() else {
        return Vec::new();
    };
    let mut log = head.log_iter();
    let Ok(Some(lines)) = log.rev() else {
        return Vec::new();
    };
    let mut found: Vec<String> = Vec::new();
    for line in lines.filter_map(Result::ok) {
        let message = line.message.to_string();
        let Some((from, to)) = message
            .strip_prefix("checkout: moving from ")
            .and_then(|rest| rest.split_once(" to "))
        else {
            continue;
        };
        for name in [to, from] {
            let known = local
                .iter()
                .any(|branch| branch.name == name && !branch.current);
            if known && !found.iter().any(|seen| seen == name) {
                found.push(name.to_owned());
            }
        }
        if found.len() >= RECENT {
            break;
        }
    }
    found.truncate(RECENT);
    found
}

#[derive(Debug)]
pub(crate) struct BranchView {
    scope: ScopeId,
    pub(crate) root: PathBuf,
    pub(crate) branches: Branches,
    pub(crate) filter: String,
    pub(crate) items: Vec<Item>,
    rendered: Rendered,
}

impl AppletBuffer for BranchView {
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

fn place(buffer: &mut Buffer, line: usize) {
    let start = buffer.line_start(line);
    buffer.set_point(start);
}

fn redraw(editor: &mut Editor, id: BufferId, keep: Option<(Group, String)>) {
    let Some((buffer, view)) = editor.applet_buffer_mut::<BranchView>(id) else {
        return;
    };
    let line = buffer.line_of(buffer.point());
    let (rendered, items) = render(&view.branches, &view.filter);
    buffer.set_text(&rendered.text);
    let found = keep.and_then(|(group, name)| {
        items.iter().position(|item| {
            matches!(item, Item::Branch(g, index) if *g == group && view.branches.name(*g, *index) == Some(name.as_str()))
        })
    });
    let first = items
        .iter()
        .position(|item| matches!(item, Item::Branch(..)));
    let last = items.len().saturating_sub(1);
    place(buffer, found.or(first).unwrap_or(line.min(last)));
    view.rendered = rendered;
    view.items = items;
}

fn at_point(editor: &Editor, id: BufferId) -> Option<(Group, String)> {
    let (buffer, view) = editor.applet_buffer::<BranchView>(id)?;
    let Item::Branch(group, index) = *view.items.get(buffer.line_of(buffer.point()))? else {
        return None;
    };
    Some((group, view.branches.name(group, index)?.to_owned()))
}

fn branch_buffer(editor: &mut Editor, root: &Path) -> Option<BufferId> {
    if let Some(id) = editor.find_applet_buffer::<BranchView>(|view| view.root == root) {
        return Some(id);
    }
    let scope = editor
        .applet_state::<GitState>()
        .map(|state| state.branches_scope)?;
    let name = format!("git branches: {}", view::folder_name(root));
    let root = root.to_owned();
    Some(editor.add_applet_buffer_with(&name, |buffer| {
        buffer.set_read_only(true);
        BranchView {
            scope,
            root,
            branches: Branches::default(),
            filter: String::new(),
            items: Vec::new(),
            rendered: Rendered::default(),
        }
    }))
}

pub(crate) fn open(editor: &mut Editor) {
    let Some(root) = view::current(editor).map(|(_, view)| view.root.clone()) else {
        return;
    };
    let branches = match read(&root) {
        Ok(branches) => branches,
        Err(error) => {
            editor.show_message(error);
            return;
        }
    };
    let Some(id) = branch_buffer(editor, &root) else {
        return;
    };
    if let Some((_, view)) = editor.applet_buffer_mut::<BranchView>(id) {
        view.branches = branches;
        view.filter.clear();
    }
    redraw(editor, id, None);
    editor.show_buffer(id);
}
pub(crate) fn refresh_root(editor: &mut Editor, root: &Path) {
    for id in editor.applet_buffers::<BranchView>() {
        let same = editor
            .applet_buffer::<BranchView>(id)
            .is_some_and(|(_, view)| view.root == root);
        if !same {
            continue;
        }
        let keep = at_point(editor, id);
        if let Ok(branches) = read(root)
            && let Some((_, view)) = editor.applet_buffer_mut::<BranchView>(id)
        {
            view.branches = branches;
        }
        redraw(editor, id, keep);
    }
}

fn current(editor: &Editor) -> Option<BufferId> {
    let id = editor.buffer().id();
    editor.applet_buffer::<BranchView>(id).map(|_| id)
}

pub(crate) fn refresh(editor: &mut Editor) {
    let Some(id) = current(editor) else {
        return;
    };
    let Some(root) = editor
        .applet_buffer::<BranchView>(id)
        .map(|(_, view)| view.root.clone())
    else {
        return;
    };
    refresh_root(editor, &root);
}

pub(crate) fn step(editor: &mut Editor, forward: bool) {
    let Some(id) = current(editor) else {
        return;
    };
    let Some((buffer, view)) = editor.applet_buffer_mut::<BranchView>(id) else {
        return;
    };
    let line = buffer.line_of(buffer.point());
    let branch = |at: &usize| matches!(view.items[*at], Item::Branch(..));
    let found = if forward {
        (line + 1..view.items.len()).find(branch)
    } else {
        (0..line).rev().find(branch)
    };
    if let Some(found) = found {
        place(buffer, found);
    }
}

pub(crate) fn checkout(editor: &mut Editor) {
    let Some(id) = current(editor) else {
        return;
    };
    let Some((_, name)) = at_point(editor, id) else {
        editor.show_message("No branch on this line");
        return;
    };
    let Some(root) = editor
        .applet_buffer::<BranchView>(id)
        .map(|(_, view)| view.root.clone())
    else {
        return;
    };
    run::run(
        editor,
        root,
        Plan::default().step(["checkout", name.as_str()]),
    );
}

pub(crate) fn filter(editor: &mut Editor) {
    let Some(id) = current(editor) else {
        return;
    };
    let before = editor
        .applet_buffer::<BranchView>(id)
        .map(|(_, view)| view.filter.clone())
        .unwrap_or_default();
    if let Some(state) = editor.applet_state_mut::<GitState>() {
        state.filtering = Some((id, before.clone()));
    }
    editor.prompt_with_text("Filter branches: ", &before, move |editor, text| {
        if let Some(state) = editor.applet_state_mut::<GitState>() {
            state.filtering = None;
        }
        set_filter(editor, id, &text);
    });
}

fn set_filter(editor: &mut Editor, id: BufferId, text: &str) {
    let changed = editor
        .applet_buffer_mut::<BranchView>(id)
        .is_some_and(|(_, view)| {
            let changed = view.filter != text;
            text.clone_into(&mut view.filter);
            changed
        });
    if changed {
        redraw(editor, id, None);
    }
}

pub(crate) fn live(editor: &mut Editor) {
    let Some((id, before)) = editor
        .applet_state::<GitState>()
        .and_then(|state| state.filtering.clone())
    else {
        return;
    };
    if let Some(typed) = editor
        .minibuffer()
        .map(|minibuffer| minibuffer.input().contents())
    {
        set_filter(editor, id, &typed);
        return;
    }
    if let Some(state) = editor.applet_state_mut::<GitState>() {
        state.filtering = None;
    }
    set_filter(editor, id, &before);
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::Editor;

    use super::*;
    use crate::repo::fixtures::{git, repository};
    use crate::testing::{at_point, installed, lines, press, run_jobs, type_text};

    fn commit(dir: &Path, file: &str, text: &str, message: &str) {
        fs::write(dir.join(file), text).unwrap();
        git(dir, &["add", file]);
        git(dir, &["commit", "-q", "-m", message]);
    }

    fn branched() -> tempfile::TempDir {
        let dir = repository();
        git(dir.path(), &["checkout", "-q", "-b", "feature"]);
        commit(dir.path(), "f.txt", "f\n", "Feature work");
        git(dir.path(), &["checkout", "-q", "main"]);
        git(dir.path(), &["tag", "v1"]);
        git(
            dir.path(),
            &["remote", "add", "origin", "https://example.com/repo.git"],
        );
        git(
            dir.path(),
            &["update-ref", "refs/remotes/origin/main", "HEAD"],
        );
        git(
            dir.path(),
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        git(
            dir.path(),
            &["branch", "-q", "--set-upstream-to=origin/main"],
        );
        commit(dir.path(), "m.txt", "m\n", "Main work");
        dir
    }

    fn local(name: &str, current: bool, upstream: Option<Upstream>) -> Local {
        Local {
            name: name.to_owned(),
            current,
            upstream,
        }
    }

    #[test]
    fn branches_are_read_in_groups() {
        let dir = branched();

        let branches = read(dir.path()).unwrap();

        assert_eq!(
            branches.local,
            [
                local("feature", false, None),
                local(
                    "main",
                    true,
                    Some(Upstream {
                        name: "origin/main".to_owned(),
                        ahead: 1,
                        behind: 0
                    })
                ),
            ]
        );
        assert_eq!(branches.remote, ["origin/main"]);
        assert_eq!(branches.recent, ["feature"]);
        assert_eq!(branches.tags, ["v1"]);
    }

    fn sample() -> Branches {
        Branches {
            local: vec![
                local("feature", false, None),
                local(
                    "main",
                    true,
                    Some(Upstream {
                        name: "origin/main".to_owned(),
                        ahead: 1,
                        behind: 0,
                    }),
                ),
            ],
            remote: vec!["origin/main".to_owned()],
            recent: vec!["feature".to_owned()],
            tags: vec!["v1".to_owned()],
        }
    }

    #[test]
    fn the_tree_lists_each_group_with_the_current_branch_marked() {
        let (rendered, items) = render(&sample(), "");

        assert_eq!(
            rendered.text.split('\n').collect::<Vec<_>>(),
            [
                "Local (2)",
                "    feature",
                "  ● main     origin/main  ↑1 ↓0",
                "",
                "Remote (1)",
                "    origin/main",
                "",
                "Recent (1)",
                "    feature",
                "",
                "Tags (1)",
                "    v1",
            ]
        );
        assert_eq!(items[2], Item::Branch(Group::Local, 1));
        assert_eq!(items[4], Item::Heading(Group::Remote));
        assert_eq!(
            rendered.pills[2],
            [(4..8, Highlight::Link), (13..24, Highlight::Function)]
        );
    }

    #[test]
    fn filters_keep_matching_branches_in_any_case() {
        let (rendered, _) = render(&sample(), "MAIN");

        assert_eq!(
            rendered.text.split('\n').collect::<Vec<_>>(),
            [
                "Filter: MAIN",
                "",
                "Local (1)",
                "  ● main     origin/main  ↑1 ↓0",
                "",
                "Remote (1)",
                "    origin/main",
            ]
        );
    }

    #[test]
    fn filters_that_match_nothing_say_so() {
        let (rendered, _) = render(&sample(), "nope");

        assert_eq!(
            rendered.text.split('\n').collect::<Vec<_>>(),
            ["Filter: nope", "", "No branches match"]
        );
    }

    fn tree() -> (tempfile::TempDir, Editor) {
        let dir = branched();
        let mut editor = installed(dir.path());
        press(&mut editor, "C-x g b l");
        (dir, editor)
    }

    #[test]
    fn b_l_shows_the_branches_with_point_on_the_first() {
        let (dir, editor) = tree();

        let name = dir.path().file_name().unwrap().to_string_lossy();
        assert_eq!(editor.buffer().name(), format!("git branches: {name}"));
        assert!(editor.buffer().is_read_only());
        assert_eq!(at_point(&editor), "    feature");
    }

    #[test]
    fn n_and_p_move_between_branches() {
        let (_dir, mut editor) = tree();

        press(&mut editor, "n n");
        assert_eq!(at_point(&editor), "    origin/main");

        press(&mut editor, "p");
        assert!(at_point(&editor).contains("main"));
    }

    #[test]
    fn return_checks_out_the_branch_at_point() {
        let (dir, mut editor) = tree();

        press(&mut editor, "RET");
        run_jobs(&mut editor);

        assert_eq!(
            git(dir.path(), &["branch", "--show-current"]).trim(),
            "feature"
        );
        assert!(
            lines(&editor).contains(&"  ● feature".to_owned()),
            "{:?}",
            lines(&editor)
        );
        assert_eq!(at_point(&editor), "  ● feature");
    }

    #[test]
    fn typing_a_filter_narrows_the_tree_as_it_goes() {
        let (_dir, mut editor) = tree();

        press(&mut editor, "/");
        type_text(&mut editor, "fea");
        assert_eq!(lines(&editor)[0], "Filter: fea");
        assert!(!lines(&editor).contains(&"Remote (1)".to_owned()));

        press(&mut editor, "RET");
        assert_eq!(lines(&editor)[0], "Filter: fea");
        assert_eq!(at_point(&editor), "    feature");
    }

    #[test]
    fn quitting_the_filter_prompt_restores_the_tree() {
        let (_dir, mut editor) = tree();

        press(&mut editor, "/");
        type_text(&mut editor, "fea");
        press(&mut editor, "C-g");

        assert_eq!(lines(&editor)[0], "Local (2)");
    }

    #[test]
    fn g_reads_the_branches_again() {
        let (dir, mut editor) = tree();
        git(dir.path(), &["branch", "-q", "another"]);

        press(&mut editor, "g");

        assert!(lines(&editor).contains(&"    another".to_owned()));
        assert_eq!(at_point(&editor), "    feature");
    }
}
