use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::error::Error;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::library::paths::text;
use crate::portable::{Failure, Portable};
use crate::symbol::Sym;
use crate::value::Value;

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
        done(fs::write(&path, contents), "write", &path)
    })
}

pub fn install(isolate: &mut Isolate) {
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
