use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: usize,
    pub old_len: usize,
    pub new_start: usize,
    pub new_len: usize,
    pub lines: Vec<DiffLine>,
}

impl Hunk {
    pub fn header(&self) -> String {
        format!(
            "@@ -{},{} +{},{} @@",
            self.old_start, self.old_len, self.new_start, self.new_len
        )
    }
}

type Change = (Range<usize>, Range<usize>);

fn index(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

pub fn hunks(old: &str, new: &str, context: usize) -> Vec<Hunk> {
    use gix::diff::blob::{Algorithm, Diff, InternedInput};

    let input = InternedInput::new(old, new);
    let diff = Diff::compute(Algorithm::Histogram, &input);
    let before: Vec<&str> = old.lines().collect();
    let after: Vec<&str> = new.lines().collect();
    let mut hunks = Vec::new();
    let mut group: Vec<Change> = Vec::new();
    for hunk in diff.hunks() {
        let change = (
            index(hunk.before.start)..index(hunk.before.end),
            index(hunk.after.start)..index(hunk.after.end),
        );
        if let Some(last) = group.last()
            && change.0.start - last.0.end > 2 * context
        {
            hunks.push(gather(&group, &before, &after, context));
            group.clear();
        }
        group.push(change);
    }
    if !group.is_empty() {
        hunks.push(gather(&group, &before, &after, context));
    }
    hunks
}

fn gather(group: &[Change], before: &[&str], after: &[&str], context: usize) -> Hunk {
    let (first, last) = (&group[0], &group[group.len() - 1]);
    let old_from = first.0.start.saturating_sub(context);
    let new_from = first.1.start - (first.0.start - old_from);
    let old_to = (last.0.end + context).min(before.len());
    let mut lines = Vec::new();
    let line = |kind, text: &str| DiffLine {
        kind,
        text: text.to_owned(),
    };
    let mut at = old_from;
    for (removed, added) in group {
        lines.extend(
            before[at..removed.start]
                .iter()
                .map(|text| line(LineKind::Context, text)),
        );
        lines.extend(
            before[removed.clone()]
                .iter()
                .map(|text| line(LineKind::Removed, text)),
        );
        lines.extend(
            after[added.clone()]
                .iter()
                .map(|text| line(LineKind::Added, text)),
        );
        at = removed.end;
    }
    lines.extend(
        before[at..old_to]
            .iter()
            .map(|text| line(LineKind::Context, text)),
    );
    let old_len = lines
        .iter()
        .filter(|line| line.kind != LineKind::Added)
        .count();
    let new_len = lines
        .iter()
        .filter(|line| line.kind != LineKind::Removed)
        .count();
    let start = |from: usize, len: usize| if len == 0 { from } else { from + 1 };
    Hunk {
        old_start: start(old_from, old_len),
        old_len,
        new_start: start(new_from, new_len),
        new_len,
        lines,
    }
}

pub fn patch(path: &str, hunk: &Hunk, selected: Option<&Range<usize>>, reverse: bool) -> String {
    let chosen = |at: usize| selected.is_none_or(|range| range.contains(&at));
    let mut body = String::new();
    let (mut old_len, mut new_len) = (0, 0);
    for (at, line) in hunk.lines.iter().enumerate() {
        let mark = match (line.kind, chosen(at), reverse) {
            (LineKind::Added, true, _) => '+',
            (LineKind::Removed, true, _) => '-',
            (LineKind::Context, ..)
            | (LineKind::Added, false, true)
            | (LineKind::Removed, false, false) => ' ',
            (LineKind::Added | LineKind::Removed, false, _) => continue,
        };
        match mark {
            '+' => new_len += 1,
            '-' => old_len += 1,
            _ => {
                old_len += 1;
                new_len += 1;
            }
        }
        body.push(mark);
        body.push_str(&line.text);
        body.push('\n');
    }
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -{},{old_len} +{},{new_len} @@\n{body}",
        hunk.old_start, hunk.new_start
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::repo::fixtures::{git, repository};

    fn line(kind: LineKind, text: &str) -> DiffLine {
        DiffLine {
            kind,
            text: text.to_owned(),
        }
    }

    const OLD: &str = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";

    #[test]
    fn a_changed_line_becomes_one_hunk_with_context() {
        let new = OLD.replace("e\n", "E\n");

        let hunks = hunks(OLD, &new, 3);

        assert_eq!(
            hunks,
            [Hunk {
                old_start: 2,
                old_len: 7,
                new_start: 2,
                new_len: 7,
                lines: vec![
                    line(LineKind::Context, "b"),
                    line(LineKind::Context, "c"),
                    line(LineKind::Context, "d"),
                    line(LineKind::Removed, "e"),
                    line(LineKind::Added, "E"),
                    line(LineKind::Context, "f"),
                    line(LineKind::Context, "g"),
                    line(LineKind::Context, "h"),
                ],
            }]
        );
        assert_eq!(hunks[0].header(), "@@ -2,7 +2,7 @@");
    }

    #[test]
    fn far_apart_changes_make_separate_hunks_and_near_ones_merge() {
        let apart = OLD.replace("a\n", "A\n").replace("j\n", "J\n");
        let near = OLD.replace("c\n", "C\n").replace("f\n", "F\n");

        assert_eq!(hunks(OLD, &apart, 3).len(), 2);
        assert_eq!(hunks(OLD, &near, 3).len(), 1);
    }

    #[test]
    fn new_and_deleted_files_are_one_hunk() {
        let added = hunks("", "x\ny\n", 3);
        assert_eq!(added[0].header(), "@@ -0,0 +1,2 @@");

        let removed = hunks("x\n", "", 3);
        assert_eq!(removed[0].header(), "@@ -1,1 +0,0 @@");
        assert_eq!(removed[0].lines, [line(LineKind::Removed, "x")]);
    }

    #[test]
    fn identical_text_has_no_hunks() {
        assert_eq!(hunks(OLD, OLD, 3), []);
    }

    fn staged_text(dir: &std::path::Path) -> String {
        git(dir, &["show", ":a.txt"])
    }

    #[test]
    fn a_whole_hunk_patch_stages_with_git_apply() {
        let dir = repository();
        let new = "one\nTWO\nthree\nfour\n";
        fs::write(dir.path().join("a.txt"), new).unwrap();
        let hunk = hunks("one\ntwo\nthree\n", new, 3).remove(0);
        fs::write(
            dir.path().join("p.diff"),
            patch("a.txt", &hunk, None, false),
        )
        .unwrap();

        git(dir.path(), &["apply", "--cached", "p.diff"]);

        assert_eq!(staged_text(dir.path()), new);
    }

    #[test]
    fn selected_lines_stage_only_themselves() {
        let dir = repository();
        let new = "one\nTWO\nthree\nfour\n";
        fs::write(dir.path().join("a.txt"), new).unwrap();
        let hunk = hunks("one\ntwo\nthree\n", new, 3).remove(0);
        let added_four = hunk
            .lines
            .iter()
            .position(|line| line.text == "four")
            .unwrap();
        fs::write(
            dir.path().join("p.diff"),
            patch("a.txt", &hunk, Some(&(added_four..added_four + 1)), false),
        )
        .unwrap();

        git(dir.path(), &["apply", "--cached", "p.diff"]);

        assert_eq!(staged_text(dir.path()), "one\ntwo\nthree\nfour\n");
    }

    #[test]
    fn reversed_selections_unstage_only_themselves() {
        let dir = repository();
        let new = "one\nTWO\nthree\nfour\n";
        fs::write(dir.path().join("a.txt"), new).unwrap();
        git(dir.path(), &["add", "a.txt"]);
        let hunk = hunks("one\ntwo\nthree\n", new, 3).remove(0);
        let removed_two = hunk
            .lines
            .iter()
            .position(|line| line.kind == LineKind::Removed)
            .unwrap();
        fs::write(
            dir.path().join("p.diff"),
            patch("a.txt", &hunk, Some(&(removed_two..removed_two + 2)), true),
        )
        .unwrap();

        git(dir.path(), &["apply", "--cached", "--reverse", "p.diff"]);

        assert_eq!(staged_text(dir.path()), "one\ntwo\nthree\nfour\n");
    }
}
