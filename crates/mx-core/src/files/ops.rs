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

pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            copy(from, to)?;
            remove(from)
        }
        result => result,
    }
}

pub fn copy(from: &Path, to: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(from)?;
    if metadata.is_symlink() {
        return copy_link(from, to);
    }
    if !metadata.is_dir() {
        fs::copy(from, to)?;
        if let Ok(modified) = metadata.modified() {
            fs::File::options()
                .write(true)
                .open(to)?
                .set_modified(modified)?;
        }
        return Ok(());
    }
    if to.starts_with(from) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("cannot copy {} into itself", from.display()),
        ));
    }
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        copy(&entry.path(), &to.join(entry.file_name()))?;
    }
    fs::set_permissions(to, metadata.permissions())
}

#[cfg(unix)]
fn copy_link(from: &Path, to: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(fs::read_link(from)?, to)
}

#[cfg(windows)]
fn copy_link(from: &Path, to: &Path) -> io::Result<()> {
    let target = fs::read_link(from)?;
    if fs::metadata(from).is_ok_and(|metadata| metadata.is_dir()) {
        std::os::windows::fs::symlink_dir(target, to)
    } else {
        std::os::windows::fs::symlink_file(target, to)
    }
}

#[cfg(not(any(unix, windows)))]
fn copy_link(from: &Path, to: &Path) -> io::Result<()> {
    fs::copy(from, to).map(|_| ())
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
    use std::time::{Duration, SystemTime};

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

    fn tree(root: &Path) {
        fs::create_dir_all(root.join("inner")).unwrap();
        fs::write(root.join("inner").join("x.txt"), "x").unwrap();
    }

    #[test]
    fn renaming_moves_files_and_directories() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        tree(&dir.path().join("sub"));

        rename(&dir.path().join("a.txt"), &dir.path().join("b.txt")).unwrap();
        rename(&dir.path().join("sub"), &dir.path().join("moved")).unwrap();

        assert_eq!(fs::read_to_string(dir.path().join("b.txt")).unwrap(), "a");
        assert!(!dir.path().join("a.txt").exists());
        assert!(
            dir.path()
                .join("moved")
                .join("inner")
                .join("x.txt")
                .exists()
        );
        assert!(!dir.path().join("sub").exists());
    }

    #[test]
    fn copying_a_file_keeps_its_contents_and_time() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("a.txt");
        fs::write(&from, "a").unwrap();
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        fs::File::options()
            .write(true)
            .open(&from)
            .unwrap()
            .set_modified(old)
            .unwrap();

        copy(&from, &dir.path().join("b.txt")).unwrap();

        let to = dir.path().join("b.txt");
        assert_eq!(fs::read_to_string(&to).unwrap(), "a");
        assert_eq!(fs::metadata(&to).unwrap().modified().unwrap(), old);
        assert!(from.exists());
    }

    #[test]
    fn copying_a_directory_copies_its_tree() {
        let dir = tempfile::tempdir().unwrap();
        tree(&dir.path().join("sub"));

        copy(&dir.path().join("sub"), &dir.path().join("dup")).unwrap();

        assert_eq!(
            fs::read_to_string(dir.path().join("dup").join("inner").join("x.txt")).unwrap(),
            "x"
        );
        assert!(dir.path().join("sub").join("inner").join("x.txt").exists());
    }

    #[test]
    fn copying_a_directory_into_itself_fails() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        tree(&sub);

        assert!(copy(&sub, &sub.join("inner").join("dup")).is_err());
        assert!(!sub.join("inner").join("dup").exists());
    }

    #[cfg(unix)]
    #[test]
    fn copying_keeps_symlinks_as_links() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        std::os::unix::fs::symlink("elsewhere", sub.join("link")).unwrap();

        copy(&sub, &dir.path().join("dup")).unwrap();

        assert_eq!(
            fs::read_link(dir.path().join("dup").join("link")).unwrap(),
            Path::new("elsewhere")
        );
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
