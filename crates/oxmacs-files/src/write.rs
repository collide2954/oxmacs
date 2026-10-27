use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tempfile::Builder;

use crate::platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewFile {
    #[default]
    Shared,
    Private,
}

fn resolved(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    for _ in 0..40 {
        let linked =
            fs::symlink_metadata(&current).is_ok_and(|metadata| metadata.file_type().is_symlink());
        let Some(target) = linked.then(|| fs::read_link(&current).ok()).flatten() else {
            break;
        };
        current = match current.parent() {
            Some(parent) if target.is_relative() => parent.join(target),
            _ => target,
        };
    }
    current
}

fn in_place(target: &Path, fill: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    let mut opened = OpenOptions::new().write(true).truncate(true).open(target)?;
    fill(&mut opened)?;
    opened.sync_all()
}

fn copied_in_place(target: &Path, from: &Path) -> io::Result<()> {
    let mut from = File::open(from)?;
    in_place(target, |file| io::copy(&mut from, file).map(drop))
}

pub fn write(path: &Path, bytes: &[u8], new: NewFile) -> io::Result<()> {
    write_with(path, new, |file| file.write_all(bytes))
}

pub fn write_with(
    path: &Path,
    new: NewFile,
    fill: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let target = resolved(path);
    let existing = fs::metadata(&target).ok();
    if existing
        .as_ref()
        .is_some_and(|metadata| platform::linked(&target, metadata))
    {
        return in_place(&target, fill);
    }
    let directory = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut builder = Builder::new();
    builder.prefix(".oxmacs-").suffix(".tmp");
    platform::new_file_mode(&mut builder, new);
    let mut temp = match builder.tempfile_in(directory) {
        Ok(temp) => temp,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied && existing.is_some() => {
            return in_place(&target, fill);
        }
        Err(error) => return Err(error),
    };
    fill(temp.as_file_mut())?;
    temp.as_file().sync_all()?;
    let Some(metadata) = existing else {
        return temp.persist(&target).map(drop).map_err(|error| error.error);
    };
    fs::set_permissions(temp.path(), metadata.permissions())?;
    platform::keep_owner(temp.path(), &metadata);
    platform::copy_attributes(&target, temp.path());
    let kept = temp.into_temp_path().keep()?;
    let result = platform::replace(&target, &kept).or_else(|_| copied_in_place(&target, &kept));
    if kept.exists() {
        let _ = fs::remove_file(&kept);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn writing_replaces_the_whole_file_and_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, "old and longer").unwrap();

        write(&path, b"new", NewFile::Shared).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn writing_makes_new_files_but_not_new_folders() {
        let dir = tempfile::tempdir().unwrap();

        write(&dir.path().join("new.txt"), b"hi", NewFile::Shared).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("new.txt")).unwrap(),
            "hi"
        );
        assert!(
            write(
                &dir.path().join("missing").join("a.txt"),
                b"hi",
                NewFile::Shared
            )
            .is_err()
        );
    }

    #[test]
    fn a_writer_fills_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("made.bin");

        write_with(&path, NewFile::Shared, |file| {
            file.write_all(b"one ")?;
            file.write_all(b"two")
        })
        .unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "one two");
    }

    #[test]
    fn a_writer_that_fails_leaves_the_old_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kept.txt");
        fs::write(&path, "old").unwrap();

        let failed = write_with(&path, NewFile::Shared, |file| {
            file.write_all(b"half")?;
            Err(io::Error::other("encoding failed"))
        });

        assert!(failed.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn hard_links_stay_together() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        let twin = dir.path().join("b.txt");
        fs::write(&path, "old").unwrap();
        fs::hard_link(&path, &twin).unwrap();

        write(&path, b"new", NewFile::Shared).unwrap();

        assert_eq!(fs::read_to_string(&twin).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn writing_through_a_symlink_writes_its_target_and_keeps_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.txt");
        let link = dir.path().join("link.txt");
        fs::write(&target, "old").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write(&link, b"new", NewFile::Shared).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn modes_are_kept_and_new_files_follow_the_umask_unless_private() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let script = dir.path().join("script.sh");
        fs::write(&script, "old").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o750)).unwrap();
        let plain = dir.path().join("plain.txt");
        fs::write(&plain, "").unwrap();

        write(&script, b"new", NewFile::Shared).unwrap();
        write(&dir.path().join("shared.txt"), b"x", NewFile::Shared).unwrap();
        write(&dir.path().join("private.txt"), b"x", NewFile::Private).unwrap();

        assert_eq!(mode(&script), 0o750);
        assert_eq!(mode(&dir.path().join("shared.txt")), mode(&plain));
        assert_eq!(mode(&dir.path().join("private.txt")), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_that_cannot_take_new_files_is_written_in_place() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("locked");
        fs::create_dir(&folder).unwrap();
        let path = folder.join("a.txt");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
        let unlocked = fs::write(folder.join("probe"), "").is_ok();

        let written = write(&path, b"new", NewFile::Shared);

        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        if unlocked {
            return;
        }
        written.unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[cfg(windows)]
    #[test]
    fn a_file_another_program_holds_open_is_written_in_place() {
        use std::fs::OpenOptions;
        use std::os::windows::fs::OpenOptionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("held.txt");
        fs::write(&path, "old").unwrap();
        let held = OpenOptions::new()
            .read(true)
            .share_mode(1 | 2)
            .open(&path)
            .unwrap();

        write(&path, b"new", NewFile::Shared).unwrap();

        drop(held);
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn the_downloaded_from_the_internet_mark_survives_a_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("setup.txt");
        fs::write(&path, "old").unwrap();
        let mark = format!("{}:Zone.Identifier", path.display());
        fs::write(&mark, "[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();

        write(&path, b"new", NewFile::Shared).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(
            fs::read_to_string(&mark).unwrap(),
            "[ZoneTransfer]\r\nZoneId=3\r\n"
        );
    }
}
