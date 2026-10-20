use std::collections::{HashMap, HashSet};
use std::ops::Range;

use oxmacs_core::display::LineStyle;
use oxmacs_core::theme::{Highlight, NICK_COLORS};

use crate::diff::LineKind;
use crate::repo::{Change, Commit, FileDiff, Kind, Side, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Section {
    Untracked,
    Unstaged,
    Staged,
    Stashes,
    Recent,
}

impl Section {
    pub(crate) fn side(self) -> Option<Side> {
        match self {
            Self::Untracked => Some(Side::Untracked),
            Self::Unstaged => Some(Side::Unstaged),
            Self::Staged => Some(Side::Staged),
            Self::Stashes | Self::Recent => None,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Untracked => "Untracked files",
            Self::Unstaged => "Unstaged changes",
            Self::Staged => "Staged changes",
            Self::Stashes => "Stashes",
            Self::Recent => "Recent commits",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    Info,
    Blank,
    Section(Section),
    File(Section, usize),
    Hunk(Section, usize, usize),
    Line(Section, usize, usize, usize),
    Stash(usize),
    Commit(usize),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Folds {
    pub(crate) closed: HashSet<Section>,
    pub(crate) open: HashSet<(Section, String)>,
    pub(crate) hidden: HashSet<(Section, String, usize)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Status {
    pub(crate) snapshot: Snapshot,
    pub(crate) diffs: HashMap<(Section, String), FileDiff>,
}

impl Status {
    pub(crate) fn files(&self, section: Section) -> Vec<Change> {
        match section {
            Section::Untracked => self
                .snapshot
                .untracked
                .iter()
                .map(|path| Change {
                    path: path.clone(),
                    kind: Kind::Added,
                    from: None,
                })
                .collect(),
            Section::Unstaged => self.snapshot.unstaged.clone(),
            Section::Staged => self.snapshot.staged.clone(),
            Section::Stashes | Section::Recent => Vec::new(),
        }
    }

    pub(crate) fn path(&self, section: Section, index: usize) -> Option<&str> {
        match section {
            Section::Untracked => self.snapshot.untracked.get(index).map(String::as_str),
            Section::Unstaged => self
                .snapshot
                .unstaged
                .get(index)
                .map(|change| change.path.as_str()),
            Section::Staged => self
                .snapshot
                .staged
                .get(index)
                .map(|change| change.path.as_str()),
            Section::Stashes | Section::Recent => None,
        }
    }

    pub(crate) fn diff(&self, section: Section, path: &str) -> Option<&FileDiff> {
        self.diffs.get(&(section, path.to_owned()))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Rendered {
    pub(crate) text: String,
    pub(crate) rows: Vec<Row>,
    pub(crate) styles: Vec<LineStyle>,
    pub(crate) pills: Vec<Vec<(Range<usize>, Highlight)>>,
}

#[derive(Debug, Default)]
pub(crate) struct Line {
    text: String,
    width: usize,
    style: LineStyle,
    pills: Vec<(Range<usize>, Highlight)>,
}

impl Line {
    pub(crate) fn push(&mut self, text: &str, face: Option<Highlight>) -> Range<usize> {
        let start = self.width;
        self.text.push_str(text);
        self.width += text.chars().count();
        let range = start..self.width;
        if let Some(face) = face
            && !range.is_empty()
        {
            self.style.spans.push((range.clone(), face));
        }
        range
    }

    pub(crate) fn pill(&mut self, text: &str, face: Highlight) {
        let range = self.push(text, Some(face));
        self.pills.push((range, face));
    }

    pub(crate) fn pad(&mut self, column: usize) {
        let spaces = column.saturating_sub(self.width);
        self.push(&" ".repeat(spaces), None);
    }

    pub(crate) fn background(&mut self, face: Highlight) {
        self.style.background = Some(face);
    }
}

impl Rendered {
    pub(crate) fn add(&mut self, row: Row, line: Line) {
        if !self.rows.is_empty() {
            self.text.push('\n');
        }
        self.text.push_str(&line.text);
        self.rows.push(row);
        self.styles.push(line.style);
        self.pills.push(line.pills);
    }

    fn blank(&mut self) {
        self.add(Row::Blank, Line::default());
    }
}

const PATH_COLUMN: usize = 60;
const SUMMARY_COLUMN: usize = 50;

pub(crate) fn render(status: &Status, folds: &Folds, now: i64) -> Rendered {
    let mut out = Rendered::default();
    head(&mut out, &status.snapshot);
    for section in [Section::Untracked, Section::Unstaged, Section::Staged] {
        let files = status.files(section);
        if files.is_empty() {
            continue;
        }
        out.blank();
        heading(&mut out, section, Some(files.len()), folds);
        if folds.closed.contains(&section) {
            continue;
        }
        let width = files
            .iter()
            .map(|change| change.path.chars().count())
            .max()
            .unwrap_or(0)
            .min(PATH_COLUMN);
        for (index, change) in files.iter().enumerate() {
            file(&mut out, status, folds, section, index, change, width);
        }
    }
    let stashes = &status.snapshot.stashes;
    if !stashes.is_empty() {
        out.blank();
        heading(&mut out, Section::Stashes, Some(stashes.len()), folds);
        if !folds.closed.contains(&Section::Stashes) {
            for (index, stash) in stashes.iter().enumerate() {
                let mut line = Line::default();
                line.push("  ", None);
                line.push(&stash.name, Some(Highlight::Constant));
                line.push("  ", None);
                line.push(&stash.summary, None);
                out.add(Row::Stash(index), line);
            }
        }
    }
    let recent = &status.snapshot.recent;
    if !recent.is_empty() {
        out.blank();
        heading(&mut out, Section::Recent, None, folds);
        if !folds.closed.contains(&Section::Recent) {
            commits(&mut out, recent, now);
        }
    }
    out
}

fn head(out: &mut Rendered, snapshot: &Snapshot) {
    let mut line = Line::default();
    line.push("Head:     ", Some(Highlight::Comment));
    match &snapshot.branch {
        Some(branch) => line.pill(branch, Highlight::Link),
        None => {
            line.push("(detached)", Some(Highlight::Warning));
        }
    }
    line.push("  ", None);
    match &snapshot.head {
        Some(commit) => {
            line.push(&commit.id, Some(Highlight::Constant));
            line.push(" ", None);
            line.push(&commit.summary, None);
        }
        None => {
            line.push("No commits yet", Some(Highlight::Comment));
        }
    }
    out.add(Row::Info, line);
    if let Some(upstream) = &snapshot.upstream {
        let mut line = Line::default();
        line.push("Upstream: ", Some(Highlight::Comment));
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
        out.add(Row::Info, line);
    }
}

fn heading(out: &mut Rendered, section: Section, count: Option<usize>, folds: &Folds) {
    let mut line = Line::default();
    line.push(section.title(), Some(Highlight::Heading));
    if let Some(count) = count {
        line.push(&format!(" ({count})"), Some(Highlight::Comment));
    }
    if folds.closed.contains(&section) {
        line.push(" …", Some(Highlight::Comment));
    }
    out.add(Row::Section(section), line);
}

pub(crate) fn letter(section: Section, kind: Kind) -> (&'static str, Highlight) {
    if section == Section::Untracked {
        return ("?", Highlight::Comment);
    }
    match kind {
        Kind::Added => ("A", Highlight::Success),
        Kind::Modified => ("M", Highlight::Warning),
        Kind::Deleted => ("D", Highlight::Error),
        Kind::Renamed => ("R", Highlight::Link),
        Kind::TypeChange => ("T", Highlight::Constant),
        Kind::Conflict => ("U", Highlight::Error),
    }
}

pub(crate) fn counts(diff: &FileDiff) -> (usize, usize) {
    let mut added = 0;
    let mut removed = 0;
    for line in diff.hunks.iter().flat_map(|hunk| &hunk.lines) {
        match line.kind {
            LineKind::Added => added += 1,
            LineKind::Removed => removed += 1,
            LineKind::Context => {}
        }
    }
    (added, removed)
}

pub(crate) fn stats(line: &mut Line, diff: &FileDiff) {
    if diff.binary || diff.large {
        let label = if diff.binary { "binary" } else { "large" };
        line.push(label, Some(Highlight::Comment));
        return;
    }
    let (added, removed) = counts(diff);
    if added > 0 {
        line.push(&format!("+{added}"), Some(Highlight::Success));
    }
    if added > 0 && removed > 0 {
        line.push(" ", None);
    }
    if removed > 0 {
        line.push(&format!("-{removed}"), Some(Highlight::Error));
    }
}

fn file(
    out: &mut Rendered,
    status: &Status,
    folds: &Folds,
    section: Section,
    index: usize,
    change: &Change,
    width: usize,
) {
    let mut line = Line::default();
    let (letter, face) = letter(section, change.kind);
    line.push("  ", None);
    line.pill(letter, face);
    line.push("  ", None);
    line.push(&change.path, None);
    if let Some(from) = &change.from {
        line.push(&format!(" ← {from}"), Some(Highlight::Comment));
    }
    let diff = status.diff(section, &change.path);
    if let Some(diff) = diff {
        line.pad(5 + width + 2);
        stats(&mut line, diff);
    }
    out.add(Row::File(section, index), line);
    if !folds.open.contains(&(section, change.path.clone())) {
        return;
    }
    let Some(diff) = diff else {
        return;
    };
    for (number, hunk) in diff.hunks.iter().enumerate() {
        let mut line = Line::default();
        line.push(&hunk.header(), Some(Highlight::Keyword));
        out.add(Row::Hunk(section, index, number), line);
        if folds
            .hidden
            .contains(&(section, change.path.clone(), number))
        {
            continue;
        }
        for (at, diff_line) in hunk.lines.iter().enumerate() {
            let mut line = Line::default();
            match diff_line.kind {
                LineKind::Context => {
                    line.push(" ", None);
                }
                LineKind::Added => {
                    line.push("+", Some(Highlight::Success));
                    line.background(Highlight::Added);
                }
                LineKind::Removed => {
                    line.push("-", Some(Highlight::Error));
                    line.background(Highlight::Removed);
                }
            }
            line.push(&diff_line.text, None);
            out.add(Row::Line(section, index, number, at), line);
        }
    }
}

fn commits(out: &mut Rendered, recent: &[Commit], now: i64) {
    let summary_width = recent
        .iter()
        .map(|commit| commit.summary.chars().count())
        .max()
        .unwrap_or(0)
        .min(SUMMARY_COLUMN);
    let author_width = recent
        .iter()
        .map(|commit| commit.author.chars().count())
        .max()
        .unwrap_or(0);
    for (index, commit) in recent.iter().enumerate() {
        let mut line = Line::default();
        line.push("  ", None);
        line.push(&commit.id, Some(Highlight::Constant));
        line.push("  ", None);
        line.push(&shorten(&commit.summary, summary_width), None);
        line.pad(2 + commit.id.chars().count() + 2 + summary_width + 2);
        let column = line.width;
        line.push(&commit.author, Some(author_color(&commit.author)));
        line.pad(column + author_width + 2);
        line.push(&ago(commit.time, now), Some(Highlight::Comment));
        out.add(Row::Commit(index), line);
    }
}

fn shorten(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

pub(crate) fn author_color(name: &str) -> Highlight {
    let sum = name.bytes().fold(0_u32, |sum, byte| {
        sum.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    Highlight::Nick(u8::try_from(sum % u32::from(NICK_COLORS)).unwrap_or(0))
}

pub(crate) fn ago(then: i64, now: i64) -> String {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const WEEK: i64 = 7 * DAY;
    const MONTH: i64 = 2_629_746;
    const YEAR: i64 = 31_556_952;
    let seconds = (now - then).max(0);
    let (count, unit) = match seconds {
        ..MINUTE => return "just now".to_owned(),
        MINUTE..HOUR => (seconds / MINUTE, "minute"),
        HOUR..DAY => (seconds / HOUR, "hour"),
        DAY..WEEK => (seconds / DAY, "day"),
        WEEK..MONTH => (seconds / WEEK, "week"),
        MONTH..YEAR => (seconds / MONTH, "month"),
        _ => (seconds / YEAR, "year"),
    };
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {unit}{plural} ago")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::hunks;
    use crate::repo::{Stash, Upstream};

    fn commit(id: &str, summary: &str, author: &str, time: i64) -> Commit {
        Commit {
            id: id.to_owned(),
            summary: summary.to_owned(),
            author: author.to_owned(),
            time,
        }
    }

    fn change(path: &str, kind: Kind) -> Change {
        Change {
            path: path.to_owned(),
            kind,
            from: None,
        }
    }

    fn sample() -> Status {
        let snapshot = Snapshot {
            branch: Some("main".to_owned()),
            head: Some(commit("abc1234", "First commit", "Ada", 0)),
            upstream: Some(Upstream {
                name: "origin/main".to_owned(),
                ahead: 2,
                behind: 0,
            }),
            untracked: vec!["c.txt".to_owned()],
            unstaged: vec![change("a.txt", Kind::Modified)],
            staged: vec![change("d.txt", Kind::Added)],
            stashes: vec![Stash {
                name: "stash@{0}".to_owned(),
                summary: "On main: wip".to_owned(),
            }],
            recent: vec![commit("abc1234", "First commit", "Ada", 0)],
            ..Snapshot::default()
        };
        let mut diffs = HashMap::new();
        diffs.insert(
            (Section::Unstaged, "a.txt".to_owned()),
            FileDiff {
                hunks: hunks("one\ntwo\nthree\n", "one\nTWO\nthree\n", 3),
                ..FileDiff::default()
            },
        );
        diffs.insert(
            (Section::Staged, "d.txt".to_owned()),
            FileDiff {
                hunks: hunks("", "staged\n", 3),
                ..FileDiff::default()
            },
        );
        Status { snapshot, diffs }
    }

    fn lines(rendered: &Rendered) -> Vec<&str> {
        rendered.text.split('\n').collect()
    }

    #[test]
    fn the_status_lists_head_upstream_and_each_section() {
        let rendered = render(&sample(), &Folds::default(), 7200);

        assert_eq!(
            lines(&rendered),
            [
                "Head:     main  abc1234 First commit",
                "Upstream: origin/main  ↑2 ↓0",
                "",
                "Untracked files (1)",
                "  ?  c.txt",
                "",
                "Unstaged changes (1)",
                "  M  a.txt  +1 -1",
                "",
                "Staged changes (1)",
                "  A  d.txt  +1",
                "",
                "Stashes (1)",
                "  stash@{0}  On main: wip",
                "",
                "Recent commits",
                "  abc1234  First commit  Ada  2 hours ago",
            ]
        );
        assert_eq!(rendered.rows.len(), rendered.styles.len());
        assert_eq!(rendered.rows[7], Row::File(Section::Unstaged, 0));
        assert_eq!(rendered.rows[16], Row::Commit(0));
    }

    #[test]
    fn status_letters_and_branches_are_pills() {
        let rendered = render(&sample(), &Folds::default(), 0);

        assert_eq!(rendered.pills[0], [(10..14, Highlight::Link)]);
        assert_eq!(rendered.pills[1], [(10..21, Highlight::Function)]);
        assert_eq!(rendered.pills[7], [(2..3, Highlight::Warning)]);
        assert_eq!(rendered.pills[10], [(2..3, Highlight::Success)]);
        assert!(
            rendered.styles[7]
                .spans
                .contains(&(12..14, Highlight::Success))
        );
        assert!(
            rendered.styles[7]
                .spans
                .contains(&(15..17, Highlight::Error))
        );
    }

    #[test]
    fn open_files_show_their_hunks_with_tinted_lines() {
        let mut folds = Folds::default();
        folds.open.insert((Section::Unstaged, "a.txt".to_owned()));

        let rendered = render(&sample(), &folds, 0);

        let shown = lines(&rendered);
        assert_eq!(
            shown[7..13],
            [
                "  M  a.txt  +1 -1",
                "@@ -1,3 +1,3 @@",
                " one",
                "-two",
                "+TWO",
                " three"
            ]
        );
        assert_eq!(rendered.rows[8], Row::Hunk(Section::Unstaged, 0, 0));
        assert_eq!(rendered.rows[10], Row::Line(Section::Unstaged, 0, 0, 1));
        assert_eq!(rendered.styles[10].background, Some(Highlight::Removed));
        assert_eq!(rendered.styles[11].background, Some(Highlight::Added));
        assert_eq!(rendered.styles[9].background, None);
    }

    #[test]
    fn closed_sections_and_hidden_hunks_fold_away() {
        let mut folds = Folds::default();
        folds.closed.insert(Section::Untracked);
        folds.open.insert((Section::Unstaged, "a.txt".to_owned()));
        folds
            .hidden
            .insert((Section::Unstaged, "a.txt".to_owned(), 0));

        let rendered = render(&sample(), &folds, 0);

        let shown = lines(&rendered);
        assert_eq!(
            shown[3..9],
            [
                "Untracked files (1) …",
                "",
                "Unstaged changes (1)",
                "  M  a.txt  +1 -1",
                "@@ -1,3 +1,3 @@",
                "",
            ]
        );
    }

    #[test]
    fn empty_sections_and_missing_upstreams_are_left_out() {
        let status = Status {
            snapshot: Snapshot {
                branch: Some("main".to_owned()),
                ..Snapshot::default()
            },
            diffs: HashMap::new(),
        };

        let rendered = render(&status, &Folds::default(), 0);

        assert_eq!(lines(&rendered), ["Head:     main  No commits yet"]);
    }

    #[test]
    fn renames_name_where_the_file_came_from() {
        let mut status = sample();
        status.snapshot.staged = vec![Change {
            path: "new.txt".to_owned(),
            kind: Kind::Renamed,
            from: Some("old.txt".to_owned()),
        }];

        let rendered = render(&status, &Folds::default(), 0);

        assert!(lines(&rendered).contains(&"  R  new.txt ← old.txt"));
    }

    #[test]
    fn commit_columns_line_up() {
        let mut status = sample();
        status.snapshot.recent = vec![
            commit("aaaaaaa", "Short", "Ada Lovelace", 0),
            commit("bbbbbbb", "A longer summary", "Grace", 0),
        ];

        let rendered = render(&status, &Folds::default(), 90_000);

        let shown = lines(&rendered);
        let last = shown.len();
        assert_eq!(
            shown[last - 2..],
            [
                "  aaaaaaa  Short             Ada Lovelace  1 day ago",
                "  bbbbbbb  A longer summary  Grace         1 day ago",
            ]
        );
    }

    #[test]
    fn dates_read_as_how_long_ago() {
        for (seconds, expected) in [
            (5, "just now"),
            (60, "1 minute ago"),
            (7_199, "1 hour ago"),
            (3 * 86_400, "3 days ago"),
            (15 * 86_400, "2 weeks ago"),
            (70 * 86_400, "2 months ago"),
            (800 * 86_400, "2 years ago"),
        ] {
            assert_eq!(ago(0, seconds), expected, "{seconds}");
        }
        assert_eq!(ago(10, 0), "just now");
    }

    #[test]
    fn each_author_keeps_one_color() {
        assert_eq!(author_color("Ada"), author_color("Ada"));
        let colors: HashSet<Highlight> = ["Ada", "Grace", "Linus", "Margaret", "Ken"]
            .into_iter()
            .map(author_color)
            .collect();
        assert!(colors.len() > 1);
    }
}
