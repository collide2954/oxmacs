use std::path::{Path, PathBuf};
use std::{fs, io};

pub fn remove(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path).or_else(|error| fs::remove_dir(path).map_err(|_| error))
    }
}

#[cfg(not(test))]
pub fn trash(paths: &[PathBuf]) -> Result<(), String> {
    trash::delete_all(paths).map_err(|error| error.to_string())
}

#[cfg(test)]
thread_local! {
    pub static TRASHED: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub fn trash(paths: &[PathBuf]) -> Result<(), String> {
    for path in paths {
        remove(path).map_err(|error| error.to_string())?;
        TRASHED.with_borrow_mut(|trashed| trashed.push(path.clone()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_deletes_files_and_whole_directories() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        let tree = dir.path().join("sub");
        fs::write(&file, "a").unwrap();
        fs::create_dir_all(tree.join("inner")).unwrap();
        fs::write(tree.join("inner").join("x.txt"), "x").unwrap();

        remove(&file).unwrap();
        remove(&tree).unwrap();

        assert!(!file.exists());
        assert!(!tree.exists());
    }

    #[test]
    fn removing_a_missing_path_fails() {
        let dir = tempfile::tempdir().unwrap();

        assert!(remove(&dir.path().join("missing")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn removing_a_symlink_keeps_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep.txt"), "k").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        remove(&link).unwrap();

        assert!(!link.exists());
        assert!(target.join("keep.txt").exists());
    }
}
