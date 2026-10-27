use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tempfile::NamedTempFile;
use thiserror::Error;

use crate::files::{DecodeError, Format, decode, encode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskState {
    modified: SystemTime,
    len: u64,
}

impl DiskState {
    pub fn of(path: &Path) -> io::Result<Option<Self>> {
        match fs::metadata(path) {
            Ok(metadata) => Ok(Some(Self {
                modified: metadata.modified()?,
                len: metadata.len(),
            })),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub text: String,
    pub format: Format,
    pub disk: Option<DiskState>,
    pub read_only: bool,
}

#[derive(Debug, Error)]
pub enum FileError {
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("{}: {source}", path.display())]
    Decode { path: PathBuf, source: DecodeError },
    #[error("{} is a directory", path.display())]
    IsDirectory { path: PathBuf },
}

impl FileError {
    fn io(path: &Path) -> impl FnOnce(io::Error) -> Self {
        move |source| Self::Io {
            path: path.to_owned(),
            source,
        }
    }
}

pub fn load(path: &Path) -> Result<Loaded, FileError> {
    if path.is_dir() {
        return Err(FileError::IsDirectory {
            path: path.to_owned(),
        });
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Loaded {
                text: String::new(),
                format: Format::default(),
                disk: None,
                read_only: false,
            });
        }
        Err(error) => return Err(FileError::io(path)(error)),
    };
    let decoded = decode(&bytes).map_err(|source| FileError::Decode {
        path: path.to_owned(),
        source,
    })?;
    Ok(Loaded {
        text: decoded.text,
        format: decoded.format,
        disk: DiskState::of(path).map_err(FileError::io(path))?,
        read_only: fs::metadata(path).is_ok_and(|metadata| metadata.permissions().readonly()),
    })
}

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

#[cfg(unix)]
fn shared(_: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink() > 1
}

#[cfg(windows)]
fn shared(path: &Path, _: &fs::Metadata) -> bool {
    fs::File::open(path)
        .and_then(|file| winapi_util::file::information(&file))
        .is_ok_and(|information| information.number_of_links() > 1)
}

#[cfg(not(any(unix, windows)))]
fn shared(_: &Path, _: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn keep_owner(path: &Path, metadata: &fs::Metadata) {
    use std::os::unix::fs::MetadataExt;
    let _ = std::os::unix::fs::chown(path, Some(metadata.uid()), Some(metadata.gid()));
}

#[cfg(not(unix))]
fn keep_owner(_: &Path, _: &fs::Metadata) {}

enum Replaced {
    Done,
    InPlace,
}

fn replace(target: &Path, bytes: &[u8], existing: Option<&fs::Metadata>) -> io::Result<Replaced> {
    let directory = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temp = match NamedTempFile::new_in(directory) {
        Ok(temp) => temp,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied && existing.is_some() => {
            return Ok(Replaced::InPlace);
        }
        Err(error) => return Err(error),
    };
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    if let Some(metadata) = existing {
        fs::set_permissions(temp.path(), metadata.permissions())?;
        keep_owner(temp.path(), metadata);
    }
    match temp.persist(target) {
        Ok(_) => Ok(Replaced::Done),
        Err(_) if existing.is_some() => Ok(Replaced::InPlace),
        Err(error) => Err(error.error),
    }
}

fn write_in_place(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(target)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub fn save(path: &Path, text: &str, format: Format) -> Result<DiskState, FileError> {
    let target = resolved(path);
    let bytes = encode(text, format);
    let existing = fs::metadata(&target).ok();
    let replaced = if existing
        .as_ref()
        .is_some_and(|metadata| shared(&target, metadata))
    {
        Replaced::InPlace
    } else {
        replace(&target, &bytes, existing.as_ref()).map_err(FileError::io(path))?
    };
    if matches!(replaced, Replaced::InPlace) {
        write_in_place(&target, &bytes).map_err(FileError::io(path))?;
    }
    DiskState::of(path)
        .map_err(FileError::io(path))?
        .ok_or_else(|| FileError::io(path)(io::ErrorKind::NotFound.into()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::files::LineEnding;

    fn temp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn loading_reads_decodes_and_records_disk_state() {
        let dir = temp();
        let path = dir.path().join("a.txt");
        fs::write(&path, "one\r\ntwo\r\n").unwrap();

        let loaded = load(&path).unwrap();

        assert_eq!(loaded.text, "one\ntwo\n");
        assert_eq!(loaded.format.line_ending, LineEnding::CrLf);
        assert_eq!(loaded.disk, DiskState::of(&path).unwrap());
        assert!(loaded.disk.is_some());
    }

    #[test]
    fn loading_a_missing_file_starts_empty() {
        let dir = temp();

        let loaded = load(&dir.path().join("new.txt")).unwrap();

        assert_eq!(loaded.text, "");
        assert_eq!(loaded.disk, None);
    }

    #[test]
    fn loading_a_directory_is_an_error() {
        let dir = temp();

        let error = load(dir.path()).unwrap_err();

        assert!(error.to_string().ends_with("is a directory"), "{error}");
    }

    #[test]
    fn loading_invalid_utf8_names_the_file() {
        let dir = temp();
        let path = dir.path().join("bin.dat");
        fs::write(&path, b"ok\xFF").unwrap();

        let error = load(&path).unwrap_err().to_string();

        assert!(error.contains("bin.dat"), "{error}");
        assert!(error.contains("not valid UTF-8 at byte 2"), "{error}");
    }

    #[test]
    fn saving_writes_the_encoded_text_atomically() {
        let dir = temp();
        let path = dir.path().join("a.txt");
        fs::write(&path, "old").unwrap();
        let format = Format {
            line_ending: LineEnding::CrLf,
            bom: true,
        };

        let disk = save(&path, "one\ntwo\n", format).unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"\xEF\xBB\xBFone\r\ntwo\r\n");
        assert_eq!(Some(disk), DiskState::of(&path).unwrap());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn saving_creates_new_files() {
        let dir = temp();
        let path = dir.path().join("new.txt");

        save(&path, "hi", Format::default()).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "hi");
    }

    #[test]
    fn saving_into_a_missing_directory_is_an_error() {
        let dir = temp();
        let path = dir.path().join("missing").join("a.txt");

        assert!(save(&path, "hi", Format::default()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn saving_through_a_symlink_writes_its_target_and_keeps_the_link() {
        let dir = temp();
        let target = dir.path().join("target.txt");
        let link = dir.path().join("link.txt");
        fs::write(&target, "old").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        save(&link, "new", Format::default()).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn saving_keeps_hard_links_together() {
        let dir = temp();
        let path = dir.path().join("a.txt");
        let twin = dir.path().join("b.txt");
        fs::write(&path, "old").unwrap();
        fs::hard_link(&path, &twin).unwrap();

        save(&path, "new", Format::default()).unwrap();

        assert_eq!(fs::read_to_string(&twin).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn saving_in_a_folder_that_cannot_take_new_files_writes_in_place() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp();
        let folder = dir.path().join("locked");
        fs::create_dir(&folder).unwrap();
        let path = folder.join("a.txt");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
        let unlocked = fs::write(folder.join("probe"), "").is_ok();

        let saved = save(&path, "new", Format::default());

        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        if unlocked {
            return;
        }
        saved.unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[cfg(windows)]
    #[test]
    fn saving_a_file_another_program_holds_open_writes_in_place() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = temp();
        let path = dir.path().join("held.txt");
        fs::write(&path, "old").unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(1 | 2)
            .open(&path)
            .unwrap();

        save(&path, "new", Format::default()).unwrap();

        drop(held);
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn the_same_file_is_recognized_through_other_spellings() {
        let dir = temp();
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
    fn identities_match_through_other_spellings_even_before_a_file_exists() {
        let dir = temp();
        let path = dir.path().join("a.txt");
        fs::write(&path, "").unwrap();
        let fresh = dir.path().join("new.txt");

        assert_eq!(
            identity(&path),
            identity(&dir.path().join(".").join("a.txt"))
        );
        assert_eq!(
            identity(&fresh),
            identity(&dir.path().join(".").join("new.txt"))
        );
        assert!(!identity(&path).to_string_lossy().starts_with(r"\\?\"));
        assert!(identity(&path).is_absolute());
    }

    #[test]
    fn files_in_folders_that_do_not_exist_yet_sit_under_their_folder_s_identity() {
        let dir = temp();
        let deep = dir.path().join("new").join("deeper").join("file.ox");

        assert!(identity(&deep).starts_with(identity(dir.path())));
        assert_eq!(
            identity(&deep),
            identity(dir.path())
                .join("new")
                .join("deeper")
                .join("file.ox")
        );
    }

    #[cfg(unix)]
    #[test]
    fn saving_keeps_the_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp();
        let path = dir.path().join("script.sh");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).unwrap();

        save(&path, "new", Format::default()).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o750);
    }
}
