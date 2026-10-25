use std::fs::{self, Metadata};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::crypto::{from_base64, to_base64};
use crate::text;
use ox::Isolate;
use ox::error::Error;
use ox::heap::Heap;
use ox::portable::{Failure, Portable};
use ox::symbol::Sym;
use ox::value::Value;

const MODULE: &str = "ox/file";

fn failed(verb: &str, path: &Path, error: &dyn std::fmt::Display) -> Failure {
    Failure {
        value: None,
        message: format!("Couldn't {verb} {}: {error}", path.display()),
    }
}

fn keyword(name: &str) -> Portable {
    Portable::Keyword(Sym::intern(name))
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
        (keyword("name"), string(&name)),
        (keyword("path"), string(&path.display().to_string())),
        (keyword("dir"), Portable::Bool(metadata.is_dir())),
        (keyword("link"), Portable::Bool(link)),
        (keyword("target"), target),
        (keyword("hidden"), Portable::Bool(name.starts_with('.'))),
        (keyword("size"), size),
        (keyword("modified"), modified(metadata)),
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

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    if fs::metadata(from)?.is_dir() {
        fs::create_dir_all(to)?;
        for found in fs::read_dir(from)? {
            let found = found?;
            copy_tree(&found.path(), &to.join(found.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(|_| ())
    }
}

fn remove(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(unix)]
fn linked(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).is_ok_and(|metadata| metadata.nlink() > 1)
}

#[cfg(windows)]
fn linked(path: &Path) -> bool {
    fs::File::open(path)
        .and_then(|file| winapi_util::file::information(&file))
        .is_ok_and(|information| information.number_of_links() > 1)
}

#[cfg(not(any(unix, windows)))]
fn linked(_: &Path) -> bool {
    false
}

fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let existing = fs::metadata(&target).ok();
    if existing.is_some() && linked(&target) {
        return fs::write(&target, bytes);
    }
    let directory = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let Ok(mut temp) = tempfile::NamedTempFile::new_in(directory) else {
        return fs::write(&target, bytes);
    };
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    if let Some(metadata) = &existing {
        fs::set_permissions(temp.path(), metadata.permissions())?;
    }
    let (file, temp) = temp.into_parts();
    drop(file);
    let temp = temp.keep()?;
    fs::rename(&temp, &target).or_else(|_| {
        let _ = fs::remove_file(&temp);
        fs::write(&target, bytes)
    })
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
    isolate.define_native_in(MODULE, name, 1, Some(1), move |heap, arguments| {
        let path = PathBuf::from(text(heap, &format!("file/{name}"), arguments[0])?);
        heap.background(&format!("ox file/{name}"), move || work(path))
    });
}

fn pair(
    isolate: &mut Isolate,
    name: &'static str,
    work: fn(PathBuf, PathBuf) -> Result<Portable, Failure>,
) {
    isolate.define_native_in(MODULE, name, 2, Some(2), move |heap, arguments| {
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
    isolate.define_native_in(MODULE, "append", 2, Some(2), append);
    isolate.define_native_in(MODULE, "write-base64", 2, Some(2), write_base64);
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
    isolate.define_native_in(MODULE, "write", 2, Some(2), write);
}
