use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use ox::Isolate;
use ox::portable::{Failure, Portable};
use ox::symbol::Sym;
use zip::ZipArchive;

use crate::crypto::to_base64;
use crate::text;

const MODULE: &str = "ox/zip";

fn failure(message: String) -> Failure {
    Failure {
        value: None,
        message,
    }
}

fn open(path: &Path) -> Result<ZipArchive<File>, Failure> {
    let failed = |error: &dyn std::fmt::Display| {
        failure(format!("Couldn't open {}: {error}", path.display()))
    };
    let file = File::open(path).map_err(|error| failed(&error))?;
    ZipArchive::new(file).map_err(|error| failed(&error))
}

fn entries(path: &Path) -> Result<Portable, Failure> {
    let mut archive = open(path)?;
    let key = |name: &str| Portable::Keyword(Sym::intern(name));
    let mut found = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let unreadable = |error: &dyn std::fmt::Display| {
            failure(format!("Couldn't read {}: {error}", path.display()))
        };
        let entry = archive
            .by_index_raw(index)
            .map_err(|error| unreadable(&error))?;
        let name = entry.name().map_err(|error| unreadable(&error))?;
        found.push(Portable::Map(vec![
            (key("name"), Portable::String(name.as_ref().into())),
            (
                key("size"),
                i64::try_from(entry.size()).map_or(Portable::Nil, Portable::Int),
            ),
            (key("dir"), Portable::Bool(entry.is_dir())),
        ]));
    }
    Ok(Portable::Vector(found))
}

fn bytes(path: &Path, name: &str) -> Result<Vec<u8>, Failure> {
    let mut archive = open(path)?;
    let failed = |error: &dyn std::fmt::Display| {
        failure(format!(
            "Couldn't read {name} from {}: {error}",
            path.display()
        ))
    };
    let mut entry = archive.by_name(name).map_err(|error| failed(&error))?;
    let mut read = Vec::new();
    entry
        .read_to_end(&mut read)
        .map_err(|error| failed(&error))?;
    Ok(read)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native_in(MODULE, "entries", 1, Some(1), |heap, arguments| {
        let path = PathBuf::from(text(heap, "zip/entries", arguments[0])?);
        heap.background("ox zip/entries", move || entries(&path))
    });
    isolate.define_native_in(MODULE, "read", 2, Some(2), |heap, arguments| {
        let path = PathBuf::from(text(heap, "zip/read", arguments[0])?);
        let name = text(heap, "zip/read", arguments[1])?.to_owned();
        heap.background("ox zip/read", move || {
            bytes(&path, &name)
                .map(|read| Portable::String(String::from_utf8_lossy(&read).as_ref().into()))
        })
    });
    isolate.define_native_in(MODULE, "read-base64", 2, Some(2), |heap, arguments| {
        let path = PathBuf::from(text(heap, "zip/read-base64", arguments[0])?);
        let name = text(heap, "zip/read-base64", arguments[1])?.to_owned();
        heap.background("ox zip/read-base64", move || {
            bytes(&path, &name).map(|read| Portable::String(to_base64(&read).into()))
        })
    });
}
