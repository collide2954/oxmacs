use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Added,
    Modified,
    Deleted,
    Renamed,
    TypeChange,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    pub kind: Kind,
    pub from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub id: String,
    pub summary: String,
    pub author: String,
    pub time: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    pub name: String,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stash {
    pub name: String,
    pub summary: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub root: PathBuf,
    pub metadata: PathBuf,
    pub branch: Option<String>,
    pub head: Option<Commit>,
    pub upstream: Option<Upstream>,
    pub untracked: Vec<String>,
    pub unstaged: Vec<Change>,
    pub staged: Vec<Change>,
    pub stashes: Vec<Stash>,
    pub recent: Vec<Commit>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub branch: Option<String>,
    pub upstream: Option<(usize, usize)>,
    pub dirty: bool,
}

impl Summary {
    pub fn of(snapshot: &Snapshot) -> Self {
        Self {
            branch: snapshot.branch.clone(),
            upstream: snapshot
                .upstream
                .as_ref()
                .map(|upstream| (upstream.ahead, upstream.behind)),
            dirty: !(snapshot.untracked.is_empty()
                && snapshot.unstaged.is_empty()
                && snapshot.staged.is_empty()),
        }
    }
}

const RECENT: usize = 10;

fn failed(error: impl std::fmt::Display) -> String {
    error.to_string()
}

pub fn snapshot(path: &Path) -> Result<Snapshot, String> {
    let start = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    let repository = gix::discover(start).map_err(failed)?;
    let root = repository
        .workdir()
        .ok_or_else(|| "the repository has no working tree".to_owned())?
        .to_owned();
    let (untracked, unstaged, staged) = changes(&repository)?;
    Ok(Snapshot {
        root,
        metadata: repository.git_dir().to_owned(),
        branch: branch(&repository),
        head: repository
            .head_commit()
            .ok()
            .map(|commit| describe(&commit)),
        upstream: upstream(&repository),
        untracked,
        unstaged,
        staged,
        stashes: stashes(&repository),
        recent: recent(&repository),
    })
}

fn branch(repository: &gix::Repository) -> Option<String> {
    repository
        .head_name()
        .ok()
        .flatten()
        .map(|name| name.shorten().to_string())
}

pub fn root(directory: &Path) -> Option<PathBuf> {
    gix::discover(directory)
        .ok()?
        .workdir()
        .map(Path::to_path_buf)
}

pub fn summary(path: &Path) -> Result<Summary, String> {
    let repository = gix::discover(path).map_err(failed)?;
    Ok(Summary {
        branch: branch(&repository),
        upstream: upstream(&repository).map(|upstream| (upstream.ahead, upstream.behind)),
        dirty: dirty(&repository)?,
    })
}

fn dirty(repository: &gix::Repository) -> Result<bool, String> {
    let status = repository
        .status(gix::progress::Discard)
        .map_err(failed)?
        .untracked_files(gix::status::UntrackedFiles::Files)
        .into_iter(None)
        .map_err(failed)?;
    for item in status {
        match item.map_err(failed)? {
            gix::status::Item::IndexWorktree(entry) if entry.summary().is_none() => {}
            _ => return Ok(true),
        }
    }
    Ok(false)
}

pub(crate) fn describe(commit: &gix::Commit<'_>) -> Commit {
    Commit {
        id: commit.id.to_hex_with_len(7).to_string(),
        summary: commit
            .message()
            .map(|message| message.summary().to_string())
            .unwrap_or_default(),
        author: commit
            .author()
            .map(|author| author.name.to_string())
            .unwrap_or_default(),
        time: commit.time().map(|time| time.seconds).unwrap_or_default(),
    }
}

fn upstream(repository: &gix::Repository) -> Option<Upstream> {
    let head = repository.head_name().ok()??;
    upstream_of(repository, head.as_ref())
}

pub(crate) fn upstream_tip(
    repository: &gix::Repository,
    branch: &gix::refs::FullNameRef,
) -> Option<(String, gix::ObjectId)> {
    let fetch = gix::remote::Direction::Fetch;
    let tracking =
        if let Some(Ok(tracking)) = repository.branch_remote_tracking_ref_name(branch, fetch) {
            tracking
        } else {
            let remote = repository.branch_remote_name(branch.shorten(), fetch)?;
            if remote.as_bstr() != "." {
                return None;
            }
            repository.branch_remote_ref_name(branch, fetch)?.ok()?
        };
    let id = repository
        .find_reference(tracking.as_ref())
        .ok()?
        .peel_to_id()
        .ok()?
        .detach();
    Some((tracking.shorten().to_string(), id))
}

pub(crate) fn upstream_of(
    repository: &gix::Repository,
    branch: &gix::refs::FullNameRef,
) -> Option<Upstream> {
    let (name, theirs) = upstream_tip(repository, branch)?;
    let ours = repository
        .find_reference(branch)
        .ok()?
        .peel_to_id()
        .ok()?
        .detach();
    let count = |tip: gix::ObjectId, hidden: gix::ObjectId| {
        repository
            .rev_walk([tip])
            .with_hidden([hidden])
            .all()
            .map_or(0, Iterator::count)
    };
    Some(Upstream {
        name,
        ahead: count(ours, theirs),
        behind: count(theirs, ours),
    })
}

type Changes = (Vec<String>, Vec<Change>, Vec<Change>);

fn change(path: String, kind: Kind, from: Option<String>) -> Change {
    Change { path, kind, from }
}

fn changes(repository: &gix::Repository) -> Result<Changes, String> {
    use gix::status::index_worktree::iter::Summary;

    let mut untracked = Vec::new();
    let mut unstaged = Vec::new();
    let mut staged = Vec::new();
    let status = repository
        .status(gix::progress::Discard)
        .map_err(failed)?
        .untracked_files(gix::status::UntrackedFiles::Files)
        .into_iter(None)
        .map_err(failed)?;
    for item in status {
        match item.map_err(failed)? {
            gix::status::Item::IndexWorktree(entry) => {
                let Some(summary) = entry.summary() else {
                    continue;
                };
                let path = entry.rela_path().to_string();
                let source = match &entry {
                    gix::status::index_worktree::Item::Rewrite { source, .. } => {
                        Some(source.rela_path().to_string())
                    }
                    _ => None,
                };
                match summary {
                    Summary::Added => untracked.push(path),
                    Summary::Removed => unstaged.push(change(path, Kind::Deleted, None)),
                    Summary::Modified => unstaged.push(change(path, Kind::Modified, None)),
                    Summary::TypeChange => unstaged.push(change(path, Kind::TypeChange, None)),
                    Summary::Renamed => unstaged.push(change(path, Kind::Renamed, source)),
                    Summary::Copied | Summary::IntentToAdd => {
                        unstaged.push(change(path, Kind::Added, None));
                    }
                    Summary::Conflict => unstaged.push(change(path, Kind::Conflict, None)),
                }
            }
            gix::status::Item::TreeIndex(found) => {
                use gix::diff::index::Change as Staged;
                staged.push(match found {
                    Staged::Addition { location, .. } => {
                        change(location.to_string(), Kind::Added, None)
                    }
                    Staged::Deletion { location, .. } => {
                        change(location.to_string(), Kind::Deleted, None)
                    }
                    Staged::Modification { location, .. } => {
                        change(location.to_string(), Kind::Modified, None)
                    }
                    Staged::Rewrite {
                        source_location,
                        location,
                        ..
                    } => change(
                        location.to_string(),
                        Kind::Renamed,
                        Some(source_location.to_string()),
                    ),
                });
            }
        }
    }
    untracked.sort();
    unstaged.sort_by(|a, b| a.path.cmp(&b.path));
    staged.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((untracked, unstaged, staged))
}

fn stashes(repository: &gix::Repository) -> Vec<Stash> {
    let Ok(reference) = repository.find_reference("refs/stash") else {
        return Vec::new();
    };
    let mut log = reference.log_iter();
    let Ok(Some(lines)) = log.rev() else {
        return Vec::new();
    };
    lines
        .filter_map(Result::ok)
        .enumerate()
        .map(|(index, line)| Stash {
            name: format!("stash@{{{index}}}"),
            summary: line.message.to_string(),
        })
        .collect()
}

fn recent(repository: &gix::Repository) -> Vec<Commit> {
    let Ok(head) = repository.head_id() else {
        return Vec::new();
    };
    let Ok(walk) = repository.rev_walk([head.detach()]).all() else {
        return Vec::new();
    };
    walk.filter_map(Result::ok)
        .take(RECENT)
        .filter_map(|info| info.object().ok())
        .map(|commit| describe(&commit))
        .collect()
}

pub fn messages(path: &Path, count: usize) -> Vec<String> {
    let Ok(repository) = gix::discover(path) else {
        return Vec::new();
    };
    let Ok(head) = repository.head_id() else {
        return Vec::new();
    };
    let Ok(walk) = repository.rev_walk([head.detach()]).all() else {
        return Vec::new();
    };
    walk.filter_map(Result::ok)
        .take(count)
        .filter_map(|info| info.object().ok())
        .filter_map(|commit| {
            commit
                .message_raw()
                .ok()
                .map(|raw| raw.to_string().trim_end().to_owned())
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Unstaged,
    Staged,
    Untracked,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileDiff {
    pub hunks: Vec<crate::diff::Hunk>,
    pub binary: bool,
    pub large: bool,
}

const CONTEXT: usize = 3;
const LARGE: u64 = 1 << 20;

pub fn file_diffs(root: &Path, wanted: &[(&str, Side)]) -> Result<Vec<FileDiff>, String> {
    let repository = gix::open(root).map_err(failed)?;
    Ok(wanted
        .iter()
        .map(|(path, side)| diff_in(&repository, root, path, *side).unwrap_or_default())
        .collect())
}

fn diff_in(
    repository: &gix::Repository,
    root: &Path,
    path: &str,
    side: Side,
) -> Result<FileDiff, String> {
    let on_disk = side != Side::Staged;
    if on_disk && fs::metadata(root.join(path)).is_ok_and(|metadata| metadata.len() > LARGE) {
        return Ok(FileDiff {
            large: true,
            ..FileDiff::default()
        });
    }
    let (old, new) = match side {
        Side::Untracked => (Vec::new(), worktree(root, path)),
        Side::Unstaged => (staged(repository, path)?, worktree(root, path)),
        Side::Staged => (committed(repository, path)?, staged(repository, path)?),
    };
    let (Some(old), Some(new)) = (text(&old), text(&new)) else {
        return Ok(FileDiff {
            binary: true,
            ..FileDiff::default()
        });
    };
    let new = if old.contains('\r') {
        new.to_owned()
    } else {
        new.replace("\r\n", "\n")
    };
    Ok(FileDiff {
        hunks: crate::diff::hunks(old, &new, CONTEXT),
        ..FileDiff::default()
    })
}

pub(crate) fn text(bytes: &[u8]) -> Option<&str> {
    if bytes.iter().take(8000).any(|byte| *byte == 0) {
        return None;
    }
    std::str::from_utf8(bytes).ok()
}

fn worktree(root: &Path, path: &str) -> Vec<u8> {
    fs::read(root.join(path)).unwrap_or_default()
}

fn staged(repository: &gix::Repository, path: &str) -> Result<Vec<u8>, String> {
    let index = repository.index_or_empty().map_err(failed)?;
    let Some(entry) = index.entry_by_path(path.into()) else {
        return Ok(Vec::new());
    };
    Ok(repository.find_blob(entry.id).map_err(failed)?.take_data())
}

fn committed(repository: &gix::Repository, path: &str) -> Result<Vec<u8>, String> {
    let Ok(tree) = repository.head_tree() else {
        return Ok(Vec::new());
    };
    let Some(entry) = tree.lookup_entry_by_path(path).map_err(failed)? else {
        return Ok(Vec::new());
    };
    Ok(repository
        .find_blob(entry.object_id())
        .map_err(failed)?
        .take_data())
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    pub(crate) fn isolated() -> Command {
        let mut command = Command::new("git");
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("GIT_") {
                command.env_remove(name);
            }
        }
        command
    }

    pub(crate) fn git(directory: &Path, args: &[&str]) -> String {
        let output = isolated()
            .args([
                "-c",
                "user.name=Ada Lovelace",
                "-c",
                "user.email=ada@example.com",
                "-c",
                "init.defaultBranch=main",
                "-c",
                "core.autocrlf=false",
            ])
            .args(args)
            .current_dir(directory)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    pub(crate) fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
        fs::write(dir.path().join("b.txt"), "bee\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-q", "-m", "First commit"]);
        dir
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::fixtures::{git, repository};
    use super::*;

    #[test]
    fn a_clean_repository_names_its_branch_and_head() {
        let dir = repository();

        let snapshot = snapshot(&dir.path().join("a.txt")).unwrap();

        assert_eq!(snapshot.root, dir.path());
        assert_eq!(snapshot.metadata, dir.path().join(".git"));
        assert_eq!(snapshot.branch.as_deref(), Some("main"));
        let head = snapshot.head.unwrap();
        assert_eq!(head.summary, "First commit");
        assert_eq!(head.author, "Ada Lovelace");
        assert_eq!(head.id.len(), 7);
        assert_eq!(snapshot.recent.len(), 1);
        assert_eq!(snapshot.untracked, Vec::<String>::new());
        assert_eq!(snapshot.unstaged, []);
        assert_eq!(snapshot.staged, []);
        assert_eq!(snapshot.upstream, None);
    }

    #[test]
    fn changes_are_sorted_into_untracked_unstaged_and_staged() {
        let dir = repository();
        fs::write(dir.path().join("a.txt"), "one\n2\nthree\n").unwrap();
        fs::remove_file(dir.path().join("b.txt")).unwrap();
        fs::write(dir.path().join("c.txt"), "new\n").unwrap();
        fs::write(dir.path().join("d.txt"), "staged\n").unwrap();
        git(dir.path(), &["add", "d.txt"]);

        let snapshot = snapshot(dir.path()).unwrap();

        assert_eq!(snapshot.untracked, ["c.txt"]);
        assert_eq!(
            snapshot.unstaged,
            [
                Change {
                    path: "a.txt".to_owned(),
                    kind: Kind::Modified,
                    from: None
                },
                Change {
                    path: "b.txt".to_owned(),
                    kind: Kind::Deleted,
                    from: None
                },
            ]
        );
        assert_eq!(
            snapshot.staged,
            [Change {
                path: "d.txt".to_owned(),
                kind: Kind::Added,
                from: None
            }]
        );
    }

    #[test]
    fn recent_commits_come_newest_first() {
        let dir = repository();
        fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
        git(dir.path(), &["commit", "-q", "-am", "Second commit"]);

        let snapshot = snapshot(dir.path()).unwrap();

        let summaries: Vec<&str> = snapshot
            .recent
            .iter()
            .map(|commit| commit.summary.as_str())
            .collect();
        assert_eq!(summaries, ["Second commit", "First commit"]);
    }

    #[test]
    fn upstreams_count_commits_ahead_and_behind() {
        let dir = repository();
        git(dir.path(), &["branch", "-q", "base"]);
        git(dir.path(), &["branch", "-q", "--set-upstream-to=base"]);
        fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
        git(dir.path(), &["commit", "-q", "-am", "Ahead"]);

        let snapshot = snapshot(dir.path()).unwrap();

        assert_eq!(
            snapshot.upstream,
            Some(Upstream {
                name: "base".to_owned(),
                ahead: 1,
                behind: 0
            })
        );
    }

    #[test]
    fn stashes_are_listed() {
        let dir = repository();
        fs::write(dir.path().join("a.txt"), "changed\n").unwrap();
        git(
            dir.path(),
            &["stash", "push", "-q", "-m", "work in progress"],
        );

        let snapshot = snapshot(dir.path()).unwrap();

        assert_eq!(snapshot.stashes.len(), 1);
        assert_eq!(snapshot.stashes[0].name, "stash@{0}");
        assert!(
            snapshot.stashes[0].summary.contains("work in progress"),
            "{:?}",
            snapshot.stashes
        );
    }

    fn file_diff(root: &Path, path: &str, side: Side) -> Result<FileDiff, String> {
        file_diffs(root, &[(path, side)]).map(|mut found| found.remove(0))
    }

    fn shown(diff: &FileDiff) -> Vec<String> {
        diff.hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .map(|line| {
                let mark = match line.kind {
                    crate::diff::LineKind::Context => ' ',
                    crate::diff::LineKind::Added => '+',
                    crate::diff::LineKind::Removed => '-',
                };
                format!("{mark}{}", line.text)
            })
            .collect()
    }

    #[test]
    fn unstaged_diffs_compare_the_index_with_the_file() {
        let dir = repository();
        fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();

        let diff = file_diff(dir.path(), "a.txt", Side::Unstaged).unwrap();

        assert_eq!(shown(&diff), [" one", "-two", "+TWO", " three"]);
        assert!(!diff.binary);
    }

    #[test]
    fn staged_diffs_compare_head_with_the_index() {
        let dir = repository();
        fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();
        git(dir.path(), &["add", "a.txt"]);
        fs::write(dir.path().join("a.txt"), "changed again\n").unwrap();

        let diff = file_diff(dir.path(), "a.txt", Side::Staged).unwrap();

        assert_eq!(shown(&diff), [" one", "-two", "+TWO", " three"]);
    }

    #[test]
    fn untracked_and_deleted_files_show_all_their_lines() {
        let dir = repository();
        fs::write(dir.path().join("c.txt"), "new\n").unwrap();
        fs::remove_file(dir.path().join("b.txt")).unwrap();

        let added = file_diff(dir.path(), "c.txt", Side::Untracked).unwrap();
        let deleted = file_diff(dir.path(), "b.txt", Side::Unstaged).unwrap();

        assert_eq!(shown(&added), ["+new"]);
        assert_eq!(shown(&deleted), ["-bee"]);
    }

    #[test]
    fn windows_line_endings_in_the_file_are_not_changes() {
        let dir = repository();
        fs::write(dir.path().join("a.txt"), "one\r\nTWO\r\nthree\r\n").unwrap();

        let diff = file_diff(dir.path(), "a.txt", Side::Unstaged).unwrap();

        assert_eq!(shown(&diff), [" one", "-two", "+TWO", " three"]);
    }

    #[test]
    fn binary_files_are_not_diffed() {
        let dir = repository();
        fs::write(dir.path().join("image.bin"), b"\x89PNG\x00\x01").unwrap();

        let diff = file_diff(dir.path(), "image.bin", Side::Untracked).unwrap();

        assert!(diff.binary);
        assert_eq!(diff.hunks, []);
    }

    #[test]
    fn large_files_are_not_read() {
        let dir = repository();
        fs::write(dir.path().join("huge.txt"), "x\n".repeat(600_000)).unwrap();

        let diff = file_diff(dir.path(), "huge.txt", Side::Untracked).unwrap();

        assert!(diff.large);
        assert_eq!(diff.hunks, []);
    }

    #[test]
    fn messages_come_newest_first_in_full() {
        let dir = repository();
        fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
        git(
            dir.path(),
            &["commit", "-q", "-am", "Second commit\n\nWith a body"],
        );

        assert_eq!(
            messages(dir.path(), 5),
            ["Second commit\n\nWith a body", "First commit"]
        );
        assert_eq!(messages(dir.path(), 1), ["Second commit\n\nWith a body"]);
    }

    #[test]
    fn folders_outside_a_repository_say_so() {
        let dir = tempfile::tempdir().unwrap();

        assert!(snapshot(dir.path()).is_err());
        assert_eq!(root(dir.path()), None);
    }

    #[test]
    fn roots_are_found_from_folders_inside() {
        let dir = repository();
        fs::create_dir(dir.path().join("sub")).unwrap();

        assert_eq!(root(&dir.path().join("sub")), Some(dir.path().to_owned()));
    }

    #[test]
    fn summaries_agree_with_the_snapshot() {
        let dir = repository();
        git(dir.path(), &["branch", "-q", "base"]);
        git(dir.path(), &["branch", "-q", "--set-upstream-to=base"]);
        fs::write(dir.path().join("b.txt"), "bee bee\n").unwrap();
        git(dir.path(), &["commit", "-q", "-am", "Ahead"]);
        fs::write(dir.path().join("c.txt"), "new\n").unwrap();

        let summary = summary(dir.path()).unwrap();

        assert_eq!(
            summary,
            Summary {
                branch: Some("main".to_owned()),
                upstream: Some((1, 0)),
                dirty: true,
            }
        );
        assert_eq!(Summary::of(&snapshot(dir.path()).unwrap()), summary);
    }

    #[test]
    fn clean_trees_are_not_dirty() {
        let dir = repository();

        assert!(!summary(dir.path()).unwrap().dirty);
    }
}
