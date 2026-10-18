use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};

use jiff::{Timestamp, Zoned};

use crate::dired::{human_size, ls_time};
use crate::display::LineStyle;
use crate::files::{Entry, Kind, Sort, paths, read_entries, sort};
use crate::theme::Highlight;

const TIME_WIDTH: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Marked,
    Flagged,
}

impl Mark {
    fn symbol(self) -> char {
        match self {
            Self::Marked => '*',
            Self::Flagged => 'D',
        }
    }
}

#[derive(Debug, Clone)]
pub struct Listing {
    directory: PathBuf,
    entries: Vec<Entry>,
    sort: Sort,
    details: bool,
    marks: HashMap<OsString, Mark>,
}

impl Listing {
    pub fn new(directory: PathBuf, entries: Vec<Entry>) -> Self {
        let mut listing = Self {
            directory,
            entries: Vec::new(),
            sort: Sort::default(),
            details: true,
            marks: HashMap::new(),
        };
        listing.set_entries(entries);
        listing
    }

    pub fn read(directory: PathBuf) -> io::Result<Self> {
        let entries = read_entries(&directory)?;
        Ok(Self::new(directory, entries))
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn set_directory(&mut self, directory: PathBuf) {
        self.directory = directory;
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn set_entries(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
        sort(&mut self.entries, self.sort);
        let names: Vec<&OsStr> = self.entries.iter().map(|entry| &*entry.name).collect();
        self.marks
            .retain(|name, _| names.contains(&name.as_os_str()));
    }

    pub fn sort(&self) -> Sort {
        self.sort
    }

    pub fn set_sort(&mut self, order: Sort) {
        self.sort = order;
        sort(&mut self.entries, order);
    }

    pub fn details(&self) -> bool {
        self.details
    }

    pub fn set_details(&mut self, details: bool) {
        self.details = details;
    }

    pub fn mark(&self, name: &OsStr) -> Option<Mark> {
        self.marks.get(name).copied()
    }

    pub fn set_mark(&mut self, name: &OsStr, mark: Option<Mark>) {
        match mark {
            Some(mark) => self.marks.insert(name.to_owned(), mark),
            None => self.marks.remove(name),
        };
    }

    pub fn clear_marks(&mut self) {
        self.marks.clear();
    }

    pub fn toggle_marks(&mut self) {
        for entry in &self.entries {
            match self.marks.get(&entry.name) {
                Some(Mark::Marked) => {
                    self.marks.remove(&entry.name);
                }
                None => {
                    self.marks.insert(entry.name.clone(), Mark::Marked);
                }
                Some(Mark::Flagged) => {}
            }
        }
    }

    pub fn marked(&self, mark: Mark) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|entry| self.mark(&entry.name) == Some(mark))
            .collect()
    }

    pub fn parent_line(&self) -> Option<usize> {
        self.directory.parent().map(|_| 1)
    }

    pub fn first_entry_line(&self) -> usize {
        1 + usize::from(self.parent_line().is_some())
    }

    pub fn entry_at(&self, line: usize) -> Option<&Entry> {
        let index = line.checked_sub(self.first_entry_line())?;
        self.entries.get(index)
    }

    pub fn line_of(&self, name: &OsStr) -> Option<usize> {
        let index = self.entries.iter().position(|entry| entry.name == name)?;
        Some(self.first_entry_line() + index)
    }

    pub fn name_column(&self) -> usize {
        if self.details {
            2 + self.size_width() + 2 + TIME_WIDTH + 2
        } else {
            2
        }
    }

    pub fn render(&self, now: &Zoned, home: Option<&Path>) -> String {
        let mut lines = vec![self.header(home)];
        if self.parent_line().is_some() {
            lines.push(format!("{}..", " ".repeat(self.name_column())));
        }
        let width = self.size_width();
        for entry in &self.entries {
            let mark = self.mark(&entry.name).map_or(' ', Mark::symbol);
            let name = entry_name(entry);
            if self.details {
                let size = size_of(entry);
                let time = time_of(entry, now);
                lines.push(format!(
                    "{mark} {size:>width$}  {time:<TIME_WIDTH$}  {name}"
                ));
            } else {
                lines.push(format!("{mark} {name}"));
            }
        }
        let mut text = lines.join("\n");
        text.push('\n');
        text
    }

    pub fn line_style(&self, line: usize) -> LineStyle {
        let column = self.name_column();
        if line == 0 {
            return LineStyle {
                background: None,
                spans: vec![(0..usize::MAX, Highlight::Heading)],
            };
        }
        if Some(line) == self.parent_line() {
            return LineStyle {
                background: None,
                spans: vec![(column..column + 2, Highlight::Link)],
            };
        }
        let Some(entry) = self.entry_at(line) else {
            return LineStyle::default();
        };
        let name = entry.display_name().chars().count();
        let span = match entry.kind {
            Kind::Directory => Some((column..column + name + 1, Highlight::Link)),
            Kind::Symlink { .. } => Some((column..column + name, Highlight::Symlink)),
            Kind::File => None,
        };
        LineStyle {
            background: self.mark(&entry.name).map(|mark| match mark {
                Mark::Marked => Highlight::Marked,
                Mark::Flagged => Highlight::Flagged,
            }),
            spans: span.into_iter().collect(),
        }
    }

    pub fn last_entry_line(&self) -> usize {
        match self.entries.len() {
            0 => self.parent_line().unwrap_or(0),
            count => self.first_entry_line() + count - 1,
        }
    }

    fn header(&self, home: Option<&Path>) -> String {
        let directory = paths::abbreviate(&self.directory, home);
        match self.sort.label() {
            Some(label) => format!("  {directory} ({label}):"),
            None => format!("  {directory}:"),
        }
    }

    fn size_width(&self) -> usize {
        self.entries
            .iter()
            .map(|entry| size_of(entry).len())
            .max()
            .unwrap_or(1)
    }
}

fn size_of(entry: &Entry) -> String {
    if entry.is_dir() {
        "-".to_owned()
    } else {
        human_size(entry.size)
    }
}

fn time_of(entry: &Entry, now: &Zoned) -> String {
    entry
        .modified
        .and_then(|modified| Timestamp::try_from(modified).ok())
        .map(|modified| ls_time(&modified.to_zoned(now.time_zone().clone()), now))
        .unwrap_or_default()
}

fn entry_name(entry: &Entry) -> String {
    let name = entry.display_name();
    match &entry.kind {
        Kind::Directory => format!("{name}{}", paths::SEPARATOR),
        Kind::File => name.into_owned(),
        Kind::Symlink { target, .. } => format!("{name} -> {}", target.display()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::MAIN_SEPARATOR_STR;
    use std::time::SystemTime;

    use jiff::tz::TimeZone;

    use super::*;
    use crate::files::paths::SEPARATOR as SEP;

    fn time(text: &str) -> SystemTime {
        SystemTime::from(text.parse::<Timestamp>().unwrap())
    }

    fn now() -> Zoned {
        "2026-10-18T15:00:00Z"
            .parse::<Timestamp>()
            .unwrap()
            .to_zoned(TimeZone::UTC)
    }

    fn entry(name: &str, kind: Kind, size: u64, modified: &str) -> Entry {
        Entry {
            name: name.into(),
            kind,
            size,
            modified: Some(time(modified)),
        }
    }

    fn listing() -> Listing {
        Listing::new(
            PathBuf::from("/home/u/mx"),
            vec![
                entry("Cargo.toml", Kind::File, 1124, "2026-10-18T13:55:00Z"),
                entry("crates", Kind::Directory, 4096, "2026-10-18T14:02:00Z"),
                entry("CHANGELOG.md", Kind::File, 6301, "2025-10-18T14:09:00Z"),
            ],
        )
    }

    fn render(listing: &Listing) -> Vec<String> {
        listing
            .render(&now(), Some(Path::new("/home/u")))
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn listings_show_a_header_the_parent_and_entries() {
        assert_eq!(
            render(&listing()),
            [
                format!("  ~{SEP}mx:"),
                format!("{}..", " ".repeat(22)),
                format!("     -  Oct 18 14:02  crates{SEP}"),
                "  1.1K  Oct 18 13:55  Cargo.toml".to_owned(),
                "  6.2K  Oct 18  2025  CHANGELOG.md".to_owned(),
            ]
        );
        assert_eq!(listing().name_column(), 22);
    }

    #[test]
    fn hiding_details_leaves_the_names() {
        let mut listing = listing();

        listing.set_details(false);

        assert_eq!(
            render(&listing),
            [
                format!("  ~{SEP}mx:"),
                "  ..".to_owned(),
                format!("  crates{SEP}"),
                "  Cargo.toml".to_owned(),
                "  CHANGELOG.md".to_owned(),
            ]
        );
        assert_eq!(listing.name_column(), 2);
    }

    #[test]
    fn marks_show_in_the_first_column() {
        let mut listing = listing();

        listing.set_mark("Cargo.toml".as_ref(), Some(Mark::Marked));
        listing.set_mark("crates".as_ref(), Some(Mark::Flagged));

        let lines = render(&listing);
        assert!(lines[2].starts_with("D "), "{}", lines[2]);
        assert!(lines[3].starts_with("* "), "{}", lines[3]);
        assert!(lines[4].starts_with("  "), "{}", lines[4]);
        let marked: Vec<String> = listing
            .marked(Mark::Marked)
            .iter()
            .map(|entry| entry.display_name().into_owned())
            .collect();
        assert_eq!(marked, ["Cargo.toml"]);
        assert_eq!(listing.mark("crates".as_ref()), Some(Mark::Flagged));
    }

    #[test]
    fn marks_can_be_cleared_or_toggled() {
        let mut listing = listing();
        listing.set_mark("Cargo.toml".as_ref(), Some(Mark::Marked));
        listing.set_mark("crates".as_ref(), Some(Mark::Flagged));

        listing.toggle_marks();
        assert_eq!(listing.mark("Cargo.toml".as_ref()), None);
        assert_eq!(listing.mark("crates".as_ref()), Some(Mark::Flagged));
        assert_eq!(listing.mark("CHANGELOG.md".as_ref()), Some(Mark::Marked));

        listing.clear_marks();
        for entry in listing.entries() {
            assert_eq!(listing.mark(&entry.name), None);
        }
    }

    #[test]
    fn the_sort_order_is_named_in_the_header() {
        let mut listing = listing();

        listing.set_sort(Sort::Largest);

        let lines = render(&listing);
        assert_eq!(lines[0], format!("  ~{SEP}mx (largest first):"));
        assert!(lines[3].ends_with("CHANGELOG.md"), "{}", lines[3]);
    }

    #[test]
    fn lines_map_to_entries() {
        let listing = listing();
        let name = |line| {
            listing
                .entry_at(line)
                .map(|entry| entry.display_name().into_owned())
        };

        assert_eq!(listing.parent_line(), Some(1));
        assert_eq!(listing.first_entry_line(), 2);
        assert_eq!(name(0), None);
        assert_eq!(name(1), None);
        assert_eq!(name(2).as_deref(), Some("crates"));
        assert_eq!(name(4).as_deref(), Some("CHANGELOG.md"));
        assert_eq!(name(5), None);
        assert_eq!(listing.line_of("Cargo.toml".as_ref()), Some(3));
        assert_eq!(listing.line_of("missing".as_ref()), None);
    }

    #[test]
    fn the_root_has_no_parent_line() {
        let listing = Listing::new(PathBuf::from(MAIN_SEPARATOR_STR), Vec::new());

        assert_eq!(listing.parent_line(), None);
        assert_eq!(listing.first_entry_line(), 1);
        assert_eq!(listing.last_entry_line(), 0);
    }

    #[test]
    fn listings_end_with_a_newline_after_the_last_entry() {
        let listing = listing();

        assert!(listing.render(&now(), None).ends_with("CHANGELOG.md\n"));
        assert_eq!(listing.last_entry_line(), 4);
    }

    #[test]
    fn new_entries_keep_the_marks_of_surviving_names() {
        let mut listing = listing();
        listing.set_mark("Cargo.toml".as_ref(), Some(Mark::Marked));
        listing.set_mark("CHANGELOG.md".as_ref(), Some(Mark::Marked));

        listing.set_entries(vec![entry(
            "Cargo.toml",
            Kind::File,
            1,
            "2026-10-18T13:55:00Z",
        )]);

        assert_eq!(listing.mark("Cargo.toml".as_ref()), Some(Mark::Marked));
        assert_eq!(listing.mark("CHANGELOG.md".as_ref()), None);
    }

    #[test]
    fn lines_are_styled_by_kind_and_mark() {
        let mut listing = listing();
        listing.set_mark("crates".as_ref(), Some(Mark::Marked));
        listing.set_mark("Cargo.toml".as_ref(), Some(Mark::Flagged));

        assert_eq!(
            listing.line_style(0),
            LineStyle {
                background: None,
                spans: vec![(0..usize::MAX, Highlight::Heading)],
            }
        );
        assert_eq!(listing.line_style(1).spans, [(22..24, Highlight::Link)]);
        assert_eq!(
            listing.line_style(2),
            LineStyle {
                background: Some(Highlight::Marked),
                spans: vec![(22..29, Highlight::Link)],
            }
        );
        assert_eq!(
            listing.line_style(3),
            LineStyle {
                background: Some(Highlight::Flagged),
                spans: Vec::new(),
            }
        );
        assert_eq!(listing.line_style(4), LineStyle::default());
        assert_eq!(listing.line_style(9), LineStyle::default());
    }

    #[test]
    fn symlink_names_are_highlighted_without_their_target() {
        let listing = Listing::new(
            PathBuf::from("/home/u/mx"),
            vec![entry(
                "link",
                Kind::Symlink {
                    target: "a.txt".into(),
                    directory: false,
                },
                5,
                "2026-10-18T13:55:00Z",
            )],
        );

        assert_eq!(listing.line_style(2).spans, [(20..24, Highlight::Symlink)]);
    }

    #[test]
    fn symlinks_show_their_target() {
        let listing = Listing::new(
            PathBuf::from("/home/u/mx"),
            vec![entry(
                "link",
                Kind::Symlink {
                    target: "a.txt".into(),
                    directory: false,
                },
                5,
                "2026-10-18T13:55:00Z",
            )],
        );

        assert!(render(&listing)[2].ends_with("  link -> a.txt"));
    }
}
