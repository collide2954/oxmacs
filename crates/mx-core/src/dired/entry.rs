use std::borrow::Cow;
use std::cmp::Reverse;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::{fs, io};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Directory,
    File,
    Symlink { target: PathBuf, directory: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: OsString,
    pub kind: Kind,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

impl Entry {
    pub fn display_name(&self) -> Cow<'_, str> {
        self.name.to_string_lossy()
    }

    pub fn is_dir(&self) -> bool {
        matches!(
            self.kind,
            Kind::Directory
                | Kind::Symlink {
                    directory: true,
                    ..
                }
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[must_use]
pub enum Sort {
    #[default]
    Name,
    Newest,
    Largest,
}

impl Sort {
    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::Newest,
            Self::Newest => Self::Largest,
            Self::Largest => Self::Name,
        }
    }

    pub fn label(self) -> Option<&'static str> {
        match self {
            Self::Name => None,
            Self::Newest => Some("newest first"),
            Self::Largest => Some("largest first"),
        }
    }
}

pub fn sort(entries: &mut [Entry], sort: Sort) {
    match sort {
        Sort::Name => entries.sort_by_cached_key(|entry| {
            let name = entry.display_name().to_lowercase();
            (!entry.is_dir(), name.starts_with('.'), name)
        }),
        Sort::Newest => entries.sort_by_cached_key(|entry| {
            let name = entry.display_name().to_lowercase();
            (!entry.is_dir(), Reverse(entry.modified), name)
        }),
        Sort::Largest => entries.sort_by_cached_key(|entry| {
            let name = entry.display_name().to_lowercase();
            (!entry.is_dir(), Reverse(entry.size), name)
        }),
    }
}

pub fn read_entries(directory: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in fs::read_dir(directory)?.filter_map(Result::ok) {
        let path = item.path();
        let Ok(metadata) = fs::metadata(&path).or_else(|_| fs::symlink_metadata(&path)) else {
            continue;
        };
        let kind = if item.file_type().is_ok_and(|kind| kind.is_symlink()) {
            Kind::Symlink {
                target: fs::read_link(&path).unwrap_or_default(),
                directory: metadata.is_dir(),
            }
        } else if metadata.is_dir() {
            Kind::Directory
        } else {
            Kind::File
        };
        entries.push(Entry {
            name: item.file_name(),
            kind,
            size: metadata.len(),
            modified: metadata.modified().ok(),
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn entry(name: &str, kind: Kind, size: u64, age: u64) -> Entry {
        Entry {
            name: name.into(),
            kind,
            size,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 - age)),
        }
    }

    fn names(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| entry.display_name().into_owned())
            .collect()
    }

    fn sample() -> Vec<Entry> {
        vec![
            entry("b.txt", Kind::File, 10, 5),
            entry("A.txt", Kind::File, 300, 1),
            entry(".hidden", Kind::File, 20, 0),
            entry("zdir", Kind::Directory, 0, 9),
            entry(".git", Kind::Directory, 0, 2),
        ]
    }

    #[test]
    fn name_order_puts_directories_first_and_dotfiles_last() {
        let mut entries = sample();

        sort(&mut entries, Sort::Name);

        assert_eq!(
            names(&entries),
            ["zdir", ".git", "A.txt", "b.txt", ".hidden"]
        );
    }

    #[test]
    fn newest_order_keeps_directories_first() {
        let mut entries = sample();

        sort(&mut entries, Sort::Newest);

        assert_eq!(
            names(&entries),
            [".git", "zdir", ".hidden", "A.txt", "b.txt"]
        );
    }

    #[test]
    fn largest_order_keeps_directories_first() {
        let mut entries = sample();

        sort(&mut entries, Sort::Largest);

        assert_eq!(
            names(&entries),
            [".git", "zdir", "A.txt", ".hidden", "b.txt"]
        );
    }

    #[test]
    fn sort_orders_cycle_and_describe_themselves() {
        assert_eq!(Sort::Name.next(), Sort::Newest);
        assert_eq!(Sort::Newest.next(), Sort::Largest);
        assert_eq!(Sort::Largest.next(), Sort::Name);
        assert_eq!(Sort::Name.label(), None);
        assert_eq!(Sort::Newest.label(), Some("newest first"));
        assert_eq!(Sort::Largest.label(), Some("largest first"));
    }

    #[test]
    fn reading_a_directory_lists_its_entries() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();

        let mut entries = read_entries(dir.path()).unwrap();
        sort(&mut entries, Sort::Name);

        assert_eq!(names(&entries), ["sub", "a.txt"]);
        assert_eq!(entries[0].kind, Kind::Directory);
        assert!(entries[0].is_dir());
        assert_eq!(entries[1].kind, Kind::File);
        assert_eq!(entries[1].size, 5);
        assert!(entries[1].modified.is_some());
    }

    #[test]
    fn reading_a_missing_directory_fails() {
        let dir = tempfile::tempdir().unwrap();

        assert!(read_entries(&dir.path().join("missing")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_record_their_target() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        symlink("a.txt", dir.path().join("to-file")).unwrap();
        symlink("sub", dir.path().join("to-dir")).unwrap();

        let entries = read_entries(dir.path()).unwrap();
        let kind = |name: &str| {
            entries
                .iter()
                .find(|entry| entry.name == name)
                .map(|entry| entry.kind.clone())
        };

        assert_eq!(
            kind("to-file"),
            Some(Kind::Symlink {
                target: "a.txt".into(),
                directory: false
            })
        );
        assert_eq!(
            kind("to-dir"),
            Some(Kind::Symlink {
                target: "sub".into(),
                directory: true
            })
        );
    }
}
