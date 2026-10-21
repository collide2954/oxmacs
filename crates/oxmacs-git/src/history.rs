use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use gix::ObjectId;
use gix::traverse::commit::simple::CommitTimeOrder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) id: ObjectId,
    pub(crate) parents: Vec<ObjectId>,
    pub(crate) short: String,
    pub(crate) summary: String,
    pub(crate) author: String,
    pub(crate) time: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tips {
    Head,
    All,
    Branch(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LabelKind {
    Head,
    Local,
    Remote,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Label {
    pub(crate) name: String,
    pub(crate) kind: LabelKind,
}

fn failed(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn peeled<'repo, E>(
    references: impl Iterator<Item = Result<gix::Reference<'repo>, E>>,
) -> Vec<(String, ObjectId)> {
    references
        .filter_map(Result::ok)
        .filter(|reference| !reference.name().as_bstr().ends_with(b"/HEAD"))
        .filter_map(|mut reference| {
            let name = reference.name().shorten().to_string();
            let id = reference.peel_to_id().ok()?.detach();
            Some((name, id))
        })
        .collect()
}

fn starts(repository: &gix::Repository, tips: &Tips) -> Result<Vec<ObjectId>, String> {
    let head: Vec<ObjectId> = repository
        .head_id()
        .map(gix::Id::detach)
        .into_iter()
        .collect();
    match tips {
        Tips::Head => Ok(head),
        Tips::Branch(name) => repository
            .rev_parse_single(name.as_str())
            .map(|id| vec![id.detach()])
            .map_err(|_| format!("No branch or revision named {name}")),
        Tips::All => {
            let references = repository.references().map_err(failed)?;
            let mut found = head;
            for iter in [references.local_branches(), references.remote_branches()] {
                found.extend(peeled(iter.map_err(failed)?).into_iter().map(|(_, id)| id));
            }
            found.sort();
            found.dedup();
            Ok(found)
        }
    }
}

pub(crate) fn log(
    root: &Path,
    tips: &Tips,
    skip: usize,
    take: usize,
) -> Result<Vec<Entry>, String> {
    let repository = gix::open(root).map_err(failed)?;
    let starts = starts(&repository, tips)?;
    if starts.is_empty() {
        return Ok(Vec::new());
    }
    let walk = repository
        .rev_walk(starts)
        .sorting(gix::revision::walk::Sorting::ByCommitTime(
            CommitTimeOrder::NewestFirst,
        ))
        .all()
        .map_err(failed)?;
    let entries = walk
        .skip(skip)
        .take(take)
        .filter_map(Result::ok)
        .filter_map(|info| info.object().ok())
        .map(|commit| entry(&commit))
        .collect();
    Ok(topological(entries))
}

fn entry(commit: &gix::Commit<'_>) -> Entry {
    let described = crate::repo::describe(commit);
    Entry {
        id: commit.id,
        parents: commit.parent_ids().map(gix::Id::detach).collect(),
        short: described.id,
        summary: described.summary,
        author: described.author,
        time: described.time,
    }
}

pub(crate) fn topological(entries: Vec<Entry>) -> Vec<Entry> {
    let position: HashMap<ObjectId, usize> = entries
        .iter()
        .enumerate()
        .map(|(at, entry)| (entry.id, at))
        .collect();
    let mut waiting = vec![0_usize; entries.len()];
    for entry in &entries {
        for parent in &entry.parents {
            if let Some(at) = position.get(parent) {
                waiting[*at] += 1;
            }
        }
    }
    let mut ready: BTreeSet<usize> = (0..entries.len()).filter(|at| waiting[*at] == 0).collect();
    let mut order = Vec::with_capacity(entries.len());
    while let Some(at) = ready.pop_first() {
        order.push(at);
        for parent in &entries[at].parents {
            if let Some(parent_at) = position.get(parent) {
                waiting[*parent_at] -= 1;
                if waiting[*parent_at] == 0 {
                    ready.insert(*parent_at);
                }
            }
        }
    }
    let mut slots: Vec<Option<Entry>> = entries.into_iter().map(Some).collect();
    order
        .into_iter()
        .filter_map(|at| slots[at].take())
        .collect()
}

pub(crate) fn labels(root: &Path) -> Result<HashMap<ObjectId, Vec<Label>>, String> {
    let repository = gix::open(root).map_err(failed)?;
    let current = repository
        .head_name()
        .ok()
        .flatten()
        .map(|name| name.shorten().to_string());
    let mut found: HashMap<ObjectId, Vec<Label>> = HashMap::new();
    if current.is_none()
        && let Ok(head) = repository.head_id()
    {
        found.entry(head.detach()).or_default().push(Label {
            name: "HEAD".to_owned(),
            kind: LabelKind::Head,
        });
    }
    let references = repository.references().map_err(failed)?;
    for (iter, kind) in [
        (references.local_branches(), LabelKind::Local),
        (references.remote_branches(), LabelKind::Remote),
        (references.tags(), LabelKind::Tag),
    ] {
        for (name, id) in peeled(iter.map_err(failed)?) {
            let kind = if kind == LabelKind::Local && current.as_ref() == Some(&name) {
                LabelKind::Head
            } else {
                kind
            };
            found.entry(id).or_default().push(Label { name, kind });
        }
    }
    for labels in found.values_mut() {
        labels.sort_by(|a, b| (a.kind, &a.name).cmp(&(b.kind, &b.name)));
    }
    Ok(found)
}

pub(crate) fn branch_names(root: &Path) -> (Vec<String>, Vec<String>) {
    let Ok(repository) = gix::open(root) else {
        return (Vec::new(), Vec::new());
    };
    let Ok(references) = repository.references() else {
        return (Vec::new(), Vec::new());
    };
    let names = |iter: Result<_, _>| -> Vec<String> {
        let mut names: Vec<String> = iter
            .map(|iter| peeled(iter).into_iter().map(|(name, _)| name).collect())
            .unwrap_or_default();
        names.sort();
        names
    };
    (
        names(references.local_branches()),
        names(references.remote_branches()),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::repo::fixtures::{git, repository};

    fn summaries(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.summary.as_str()).collect()
    }

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
        commit(dir.path(), "m.txt", "m\n", "Main work");
        dir
    }

    #[test]
    fn the_head_log_lists_its_commits_newest_first_with_parents() {
        let dir = repository();
        commit(dir.path(), "b.txt", "bee bee\n", "Second commit");

        let entries = log(dir.path(), &Tips::Head, 0, 10).unwrap();

        assert_eq!(summaries(&entries), ["Second commit", "First commit"]);
        assert_eq!(entries[0].parents, [entries[1].id]);
        assert_eq!(entries[1].parents, Vec::<ObjectId>::new());
        assert_eq!(entries[0].short.len(), 7);
        assert_eq!(entries[0].author, "Ada Lovelace");
    }

    #[test]
    fn all_branches_include_the_others() {
        let dir = branched();

        let head = log(dir.path(), &Tips::Head, 0, 10).unwrap();
        let all = log(dir.path(), &Tips::All, 0, 10).unwrap();
        let feature = log(dir.path(), &Tips::Branch("feature".to_owned()), 0, 10).unwrap();

        assert_eq!(summaries(&head), ["Main work", "First commit"]);
        assert_eq!(all.len(), 3);
        assert_eq!(summaries(&feature), ["Feature work", "First commit"]);
    }

    #[test]
    fn pages_skip_what_was_loaded() {
        let dir = repository();
        commit(dir.path(), "b.txt", "2\n", "Second");
        commit(dir.path(), "b.txt", "3\n", "Third");

        let page = log(dir.path(), &Tips::Head, 1, 1).unwrap();

        assert_eq!(summaries(&page), ["Second"]);
    }

    #[test]
    fn unknown_branches_say_so() {
        let dir = repository();

        assert!(log(dir.path(), &Tips::Branch("nope".to_owned()), 0, 10).is_err());
    }

    fn entry(name: &str, parents: &[&str], time: i64) -> Entry {
        let id = |name: &str| ObjectId::from_hex(format!("{name:0>40}").as_bytes()).unwrap();
        Entry {
            id: id(name),
            parents: parents.iter().map(|parent| id(parent)).collect(),
            short: name.to_owned(),
            summary: name.to_owned(),
            author: String::new(),
            time,
        }
    }

    #[test]
    fn children_come_before_their_parents_even_with_skewed_clocks() {
        let entries = vec![
            entry("c", &["b"], 30),
            entry("a", &[], 20),
            entry("b", &["a"], 10),
            entry("d", &[], 5),
        ];

        assert_eq!(summaries(&topological(entries)), ["c", "b", "a", "d"]);
    }

    #[test]
    fn labels_name_branches_remotes_and_tags_at_their_commits() {
        let dir = branched();
        git(dir.path(), &["tag", "v1"]);
        git(
            dir.path(),
            &["tag", "-a", "v2", "-m", "Annotated", "feature"],
        );
        git(
            dir.path(),
            &["update-ref", "refs/remotes/origin/main", "HEAD"],
        );
        let head =
            ObjectId::from_hex(git(dir.path(), &["rev-parse", "HEAD"]).trim().as_bytes()).unwrap();
        let feature =
            ObjectId::from_hex(git(dir.path(), &["rev-parse", "feature"]).trim().as_bytes())
                .unwrap();

        let labels = labels(dir.path()).unwrap();

        let label = |name: &str, kind| Label {
            name: name.to_owned(),
            kind,
        };
        assert_eq!(
            labels[&head],
            [
                label("main", LabelKind::Head),
                label("origin/main", LabelKind::Remote),
                label("v1", LabelKind::Tag),
            ]
        );
        assert_eq!(
            labels[&feature],
            [
                label("feature", LabelKind::Local),
                label("v2", LabelKind::Tag)
            ]
        );
    }

    #[test]
    fn a_detached_head_is_labeled_head() {
        let dir = repository();
        git(dir.path(), &["checkout", "-q", "--detach"]);
        let head =
            ObjectId::from_hex(git(dir.path(), &["rev-parse", "HEAD"]).trim().as_bytes()).unwrap();

        let labels = labels(dir.path()).unwrap();

        assert_eq!(
            labels[&head],
            [
                Label {
                    name: "HEAD".to_owned(),
                    kind: LabelKind::Head
                },
                Label {
                    name: "main".to_owned(),
                    kind: LabelKind::Local
                },
            ]
        );
    }

    #[test]
    fn branch_names_are_local_and_remote() {
        let dir = branched();
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

        let (local, remote) = branch_names(dir.path());

        assert_eq!(local, ["feature", "main"]);
        assert_eq!(remote, ["origin/main"]);
    }
}
