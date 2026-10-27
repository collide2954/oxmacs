use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use oxmacs_files::NewFile;
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

pub use oxmacs_files::{identity, same_file};

pub fn save(path: &Path, text: &str, format: Format) -> Result<DiskState, FileError> {
    oxmacs_files::write(path, &encode(text, format), NewFile::Shared)
        .map_err(FileError::io(path))?;
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
}
