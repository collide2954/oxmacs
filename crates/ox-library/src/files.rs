use std::fs::{self, Metadata};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::crypto::{from_base64, to_base64};
use crate::text;
use ox_lang::Isolate;
use ox_lang::error::Error;
use ox_lang::heap::Heap;
use ox_lang::portable::{Failure, Portable};
use ox_lang::value::Value;

const MODULE: &str = "ox/file";

fn failed(verb: &str, path: &Path, error: &dyn std::fmt::Display) -> Failure {
    Failure {
        value: None,
        message: format!("Couldn't {verb} {}: {error}", path.display()),
    }
}

fn string(text: &str) -> Portable {
    Portable::String(text.into())
}

fn modified(metadata: &Metadata) -> Portable {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_millis()).ok())
        .map_or(Portable::Nil, Portable::Int)
}

fn entry(path: &Path, metadata: &Metadata, link: bool) -> Portable {
    let target = link
        .then(|| fs::read_link(path).ok())
        .flatten()
        .map_or(Portable::Nil, |target| {
            string(&target.display().to_string())
        });
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let size = if metadata.is_dir() {
        Portable::Nil
    } else {
        i64::try_from(metadata.len()).map_or(Portable::Nil, Portable::Int)
    };
    Portable::Map(vec![
        (Portable::keyword("name"), string(&name)),
        (
            Portable::keyword("path"),
            string(&path.display().to_string()),
        ),
        (Portable::keyword("dir"), Portable::Bool(metadata.is_dir())),
        (Portable::keyword("link"), Portable::Bool(link)),
        (Portable::keyword("target"), target),
        (
            Portable::keyword("hidden"),
            Portable::Bool(name.starts_with('.')),
        ),
        (Portable::keyword("size"), size),
        (Portable::keyword("modified"), modified(metadata)),
    ])
}

fn describe(path: &Path) -> io::Result<Portable> {
    let link = fs::symlink_metadata(path)?.file_type().is_symlink();
    let metadata = fs::metadata(path).or_else(|_| fs::symlink_metadata(path))?;
    Ok(entry(path, &metadata, link))
}

fn list(directory: &Path) -> Result<Portable, Failure> {
    let reading = |error: &dyn std::fmt::Display| failed("list", directory, error);
    let mut paths: Vec<PathBuf> = fs::read_dir(directory)
        .map_err(|error| reading(&error))?
        .filter_map(Result::ok)
        .map(|found| found.path())
        .collect();
    paths.sort();
    let entries = paths
        .iter()
        .filter_map(|path| describe(path).ok())
        .collect();
    Ok(Portable::Vector(entries))
}

fn refused(reason: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, reason)
}

fn resolved(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok().or_else(|| {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())?;
        Some(fs::canonicalize(parent).ok()?.join(path.file_name()?))
    })
}

fn check_copy(from: &Path, to: &Path) -> io::Result<()> {
    if same_file::is_same_file(from, to).unwrap_or(false) {
        return Err(refused(format!("it is the same file as {}", to.display())));
    }
    let source = fs::canonicalize(from)?;
    if fs::symlink_metadata(from)?.is_dir()
        && resolved(to).is_some_and(|target| target.starts_with(&source))
    {
        return Err(refused(format!("{} would land inside it", to.display())));
    }
    Ok(())
}

#[cfg(unix)]
fn copy_link(from: &Path, to: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(fs::read_link(from)?, to)
}

#[cfg(windows)]
fn copy_link(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::fs::{FileTypeExt, symlink_dir, symlink_file};
    let target = fs::read_link(from)?;
    let made = if fs::symlink_metadata(from)?.file_type().is_symlink_dir() {
        symlink_dir(&target, to)
    } else {
        symlink_file(&target, to)
    };
    made.map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("can't make the link {} again: {error}", from.display()),
        )
    })
}

#[cfg(not(any(unix, windows)))]
fn copy_link(from: &Path, _: &Path) -> io::Result<()> {
    Err(refused(format!("can't copy the link {}", from.display())))
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    check_copy(from, to)?;
    let mut work = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((from, to)) = work.pop() {
        let kind = fs::symlink_metadata(&from)?.file_type();
        if kind.is_symlink() {
            copy_link(&from, &to)?;
        } else if kind.is_dir() {
            fs::create_dir_all(&to)?;
            for found in fs::read_dir(&from)? {
                let found = found?;
                work.push((found.path(), to.join(found.file_name())));
            }
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn folder_link(kind: fs::FileType) -> bool {
    use std::os::windows::fs::FileTypeExt;
    kind.is_symlink_dir()
}

#[cfg(not(windows))]
fn folder_link(_: fs::FileType) -> bool {
    false
}

fn remove(path: &Path) -> io::Result<()> {
    let kind = fs::symlink_metadata(path)?.file_type();
    if kind.is_dir() {
        fs::remove_dir_all(path)
    } else if folder_link(kind) {
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    oxmacs_files::write(path, bytes, oxmacs_files::NewFile::Shared)
}

fn done(result: io::Result<()>, verb: &str, path: &Path) -> Result<Portable, Failure> {
    result
        .map(|()| Portable::Nil)
        .map_err(|error| failed(verb, path, &error))
}

fn single(
    isolate: &mut Isolate,
    name: &'static str,
    work: fn(PathBuf) -> Result<Portable, Failure>,
) {
    isolate
        .native(MODULE, name)
        .arity(1, Some(1))
        .outside()
        .function(move |heap, arguments| {
            let path = PathBuf::from(text(heap, &format!("file/{name}"), arguments[0])?);
            heap.background(&format!("ox file/{name}"), move || work(path))
        });
}

fn pair(
    isolate: &mut Isolate,
    name: &'static str,
    work: fn(PathBuf, PathBuf) -> Result<Portable, Failure>,
) {
    isolate
        .native(MODULE, name)
        .arity(2, Some(2))
        .outside()
        .function(move |heap, arguments| {
            let shown = format!("file/{name}");
            let from = PathBuf::from(text(heap, &shown, arguments[0])?);
            let to = PathBuf::from(text(heap, &shown, arguments[1])?);
            heap.background(&format!("ox {shown}"), move || work(from, to))
        });
}

fn write(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let path = PathBuf::from(text(heap, "file/write", arguments[0])?);
    let contents = text(heap, "file/write", arguments[1])?.to_owned();
    heap.background("ox file/write", move || {
        done(replace(&path, contents.as_bytes()), "write", &path)
    })
}

fn append(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let path = PathBuf::from(text(heap, "file/append", arguments[0])?);
    let contents = text(heap, "file/append", arguments[1])?.to_owned();
    heap.background("ox file/append", move || {
        let written = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| file.write_all(contents.as_bytes()));
        done(written, "append to", &path)
    })
}

fn write_base64(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let path = PathBuf::from(text(heap, "file/write-base64", arguments[0])?);
    let bytes = from_base64(text(heap, "file/write-base64", arguments[1])?)
        .ok_or_else(|| Error::new("file/write-base64 got text that isn't base64"))?;
    heap.background("ox file/write-base64", move || {
        done(replace(&path, &bytes), "write", &path)
    })
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "append")
        .arity(2, Some(2))
        .outside()
        .function(append);
    isolate
        .native(MODULE, "write-base64")
        .arity(2, Some(2))
        .outside()
        .function(write_base64);
    single(isolate, "read-base64", |path| {
        fs::read(&path)
            .map(|bytes| string(&to_base64(&bytes)))
            .map_err(|error| failed("read", &path, &error))
    });
    single(isolate, "list", |path| list(&path));
    single(isolate, "read", |path| {
        fs::read_to_string(&path)
            .map(|contents| string(&contents))
            .map_err(|error| failed("read", &path, &error))
    });
    single(isolate, "exists?", |path| Ok(Portable::Bool(path.exists())));
    single(isolate, "metadata", |path| {
        describe(&path).map_err(|error| failed("read", &path, &error))
    });
    single(isolate, "make-dir", |path| {
        done(fs::create_dir_all(&path), "create", &path)
    });
    single(isolate, "remove", |path| {
        done(remove(&path), "remove", &path)
    });
    single(isolate, "trash", |path| {
        trash::delete(&path)
            .map(|()| Portable::Nil)
            .map_err(|error| failed("move to the trash", &path, &error))
    });
    pair(isolate, "rename", |from, to| {
        done(fs::rename(&from, &to), "rename", &from)
    });
    pair(isolate, "copy", |from, to| {
        done(copy_tree(&from, &to), "copy", &from)
    });
    isolate
        .native(MODULE, "write")
        .arity(2, Some(2))
        .outside()
        .function(write);
}
