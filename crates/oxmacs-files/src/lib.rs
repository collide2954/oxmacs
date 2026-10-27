use std::fs;
use std::path::{Path, PathBuf};

pub mod paths;
#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod platform;
mod write;

pub use write::{NewFile, write, write_with};

#[cfg(windows)]
fn plain(path: PathBuf) -> PathBuf {
    use std::path::{Component, Prefix};
    let mut parts = path.components();
    let letter = match parts.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::VerbatimDisk(letter) => Some(letter),
            _ => None,
        },
        _ => None,
    };
    match letter {
        Some(letter) => {
            let mut plain = PathBuf::from(format!("{}:\\", char::from(letter)));
            plain.extend(parts.filter(|part| !matches!(part, Component::RootDir)));
            plain
        }
        None => path,
    }
}

#[cfg(not(windows))]
fn plain(path: PathBuf) -> PathBuf {
    path
}

pub fn identity(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut missing = Vec::new();
    let mut existing = absolute.as_path();
    loop {
        if let Ok(found) = fs::canonicalize(existing) {
            let mut identity = plain(found);
            identity.extend(missing.iter().rev());
            return identity;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name);
                existing = parent;
            }
            _ => return absolute,
        }
    }
}

pub fn same_file(first: &Path, second: &Path) -> bool {
    identity(first) == identity(second)
}

pub fn linked(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| platform::linked(path, &metadata))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn the_same_file_is_recognized_through_other_spellings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, "").unwrap();
        let other = dir.path().join("b.txt");
        fs::write(&other, "").unwrap();

        assert!(same_file(&path, &dir.path().join(".").join("a.txt")));
        assert!(!same_file(&path, &other));
        assert!(same_file(
            &dir.path().join("new.txt"),
            &dir.path().join("new.txt")
        ));
        if cfg!(windows) {
            assert!(same_file(&path, &dir.path().join("A.TXT")));
        }
    }

    #[test]
    fn files_in_folders_that_do_not_exist_yet_sit_under_their_folder_s_identity() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("new").join("deeper").join("file.ox");

        assert_eq!(
            identity(&deep),
            identity(dir.path())
                .join("new")
                .join("deeper")
                .join("file.ox")
        );
        assert!(!identity(dir.path()).to_string_lossy().starts_with(r"\\?\"));
        assert!(identity(dir.path()).is_absolute());
    }

    #[test]
    fn hard_links_are_told_apart_from_lone_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, "").unwrap();
        assert!(!linked(&path));

        fs::hard_link(&path, dir.path().join("b.txt")).unwrap();

        assert!(linked(&path));
        assert!(!linked(&dir.path().join("missing")));
    }
}
