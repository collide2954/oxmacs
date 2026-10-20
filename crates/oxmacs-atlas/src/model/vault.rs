use std::fs;
use std::path::{Path, PathBuf};

use jiff::civil::Date;

use crate::model::note::{self, Note};

pub const EXTENSION: &str = "atlas";
pub const JOURNAL: &str = "journal";
const SKIPPED: [&str; 1] = ["assets"];
const MAX_DEPTH: usize = 12;
const MAX_PAGES: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub vault: String,
    pub path: PathBuf,
    pub title: String,
    pub note: Note,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    pub pages: Vec<Page>,
}

pub fn title_of(path: &Path, note: &Note) -> String {
    note.field("title")
        .filter(|title| !title.trim().is_empty())
        .map(|title| title.trim().to_owned())
        .or_else(|| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
}

pub fn journal_path(directory: &Path, day: Date) -> PathBuf {
    directory.join(JOURNAL).join(format!("{day}.{EXTENSION}"))
}

pub fn page_path(directory: &Path, title: &str) -> PathBuf {
    let name: String = title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect();
    directory.join(format!("{}.{EXTENSION}", name.trim()))
}

fn files(directory: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if found.len() >= MAX_PAGES {
            return;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if depth < MAX_DEPTH && !SKIPPED.contains(&name.as_str()) {
                files(&path, depth + 1, found);
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
        {
            found.push(path);
        }
    }
}

pub fn read_page(vault: &str, path: &Path, today: Date) -> Option<Page> {
    let text = fs::read_to_string(path).ok()?;
    Some(page(vault, path, &text, today))
}

pub fn page(vault: &str, path: &Path, text: &str, today: Date) -> Page {
    let note = note::parse(text, today);
    Page {
        vault: vault.to_owned(),
        path: path.to_owned(),
        title: title_of(path, &note),
        note,
    }
}

impl Index {
    pub fn build(vaults: &[(String, PathBuf)], today: Date) -> Self {
        let mut pages = Vec::new();
        for (name, directory) in vaults {
            let mut found = Vec::new();
            files(directory, 0, &mut found);
            pages.extend(found.iter().filter_map(|path| read_page(name, path, today)));
        }
        Self { pages }
    }

    pub fn find(&self, title: &str) -> Option<&Page> {
        let title = title.trim();
        self.pages
            .iter()
            .find(|page| page.title.eq_ignore_ascii_case(title))
    }

    pub fn backlinks(&self, title: &str) -> Vec<&Page> {
        self.pages
            .iter()
            .filter(|page| {
                page.note
                    .links
                    .iter()
                    .any(|link| link.eq_ignore_ascii_case(title))
            })
            .collect()
    }

    pub fn update(&mut self, page: Page) {
        self.pages.retain(|known| known.path != page.path);
        self.pages.push(page);
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    const TODAY: Date = date(2026, 10, 19);

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("projects")).unwrap();
        fs::create_dir_all(root.join(JOURNAL)).unwrap();
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::create_dir_all(root.join(".trash")).unwrap();
        fs::write(
            root.join("Garden plan.atlas"),
            "title: Garden plan\n---\nsee [Seed catalog] and [kitchen]\n",
        )
        .unwrap();
        fs::write(root.join("Seed catalog.atlas"), "= Seeds\n").unwrap();
        fs::write(
            root.join("projects").join("Kitchen.atlas"),
            "title: Kitchen\n---\n[ ] Paint  due:fri\n",
        )
        .unwrap();
        fs::write(
            root.join(JOURNAL).join("2026-10-19.atlas"),
            "met about the [garden plan]\n",
        )
        .unwrap();
        fs::write(root.join("assets").join("ignored.atlas"), "").unwrap();
        fs::write(root.join(".trash").join("hidden.atlas"), "").unwrap();
        fs::write(root.join("notes.md"), "").unwrap();
        dir
    }

    fn index(dir: &tempfile::TempDir) -> Index {
        Index::build(&[("personal".to_owned(), dir.path().to_owned())], TODAY)
    }

    #[test]
    fn pages_come_from_every_folder_but_assets_and_hidden_ones() {
        let dir = vault();
        let index = index(&dir);

        let mut titles: Vec<&str> = index.pages.iter().map(|page| page.title.as_str()).collect();
        titles.sort_unstable();

        assert_eq!(
            titles,
            ["2026-10-19", "Garden plan", "Kitchen", "Seed catalog"]
        );
    }

    #[test]
    fn titles_come_from_the_header_or_the_file_name() {
        let dir = vault();
        let index = index(&dir);

        let kitchen = index.find("KITCHEN").unwrap();
        assert_eq!(kitchen.vault, "personal");
        assert_eq!(
            kitchen.path,
            dir.path().join("projects").join("Kitchen.atlas")
        );
        assert_eq!(kitchen.note.tasks[0].due, Some(date(2026, 10, 23)));
        assert!(index.find("Seed catalog").is_some());
        assert!(index.find("Nothing").is_none());
    }

    #[test]
    fn backlinks_are_the_pages_that_link_to_a_title() {
        let dir = vault();
        let index = index(&dir);

        let linking: Vec<&str> = index
            .backlinks("Garden Plan")
            .iter()
            .map(|page| page.title.as_str())
            .collect();

        assert_eq!(linking, ["2026-10-19"]);
        assert_eq!(index.backlinks("Kitchen").len(), 1);
    }

    #[test]
    fn saved_pages_replace_their_old_entry() {
        let dir = vault();
        let mut index = index(&dir);
        let path = dir.path().join("Seed catalog.atlas");

        index.update(page("personal", &path, "title: Seeds\n---\n", TODAY));

        assert!(index.find("Seed catalog").is_none());
        assert_eq!(index.find("Seeds").unwrap().path, path);
        assert_eq!(index.pages.len(), 4);
    }

    #[test]
    fn journal_and_page_paths_follow_the_layout() {
        let root = Path::new("vault");

        assert_eq!(
            journal_path(root, TODAY),
            root.join("journal").join("2026-10-19.atlas")
        );
        assert_eq!(
            page_path(root, "Plans: 2027/Q1"),
            root.join("Plans- 2027-Q1.atlas")
        );
    }
}
