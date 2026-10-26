pub mod crypto;
mod csv;
mod files;
mod http;
pub mod image;
mod net;
mod process;
mod zip;

use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};
use ox_lang::{Isolate, MODULES as CORE};

pub const DOCS: &str = include_str!("docs.ox");

pub const MODULES: &[(&str, &str)] = &[
    ("ox/csv", "Reading CSV into rows"),
    (
        "ox/file",
        "Reading, writing, listing, copying, renaming and removing files and folders",
    ),
    (
        "ox/process",
        "Running programs, talking to them while they run, and the environment",
    ),
    ("ox/net", "TCP connections, with or without TLS"),
    ("ox/http", "HTTP requests and URL encoding"),
    (
        "ox/crypto",
        "Hashes, encryption with a password, random numbers, base64 and one-time codes",
    ),
    (
        "ox/image",
        "Opening, editing, drawing on and saving images, with filters and color adjustments",
    ),
    (
        "ox/zip",
        "Reading the files inside zip archives, such as EPUB books",
    ),
];

pub fn modules() -> Vec<(&'static str, &'static str)> {
    CORE.iter().chain(MODULES).copied().collect()
}

pub(crate) fn text<'a>(heap: &'a Heap, name: &str, value: Value) -> Result<&'a str, Error> {
    heap.text(value).ok_or_else(|| {
        Error::new(format!(
            "{name} expects a string, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

pub fn install(isolate: &mut Isolate) {
    http::install(isolate);
    net::install(isolate);
    crypto::install(isolate);
    csv::install(isolate);
    files::install(isolate);
    process::install(isolate);
    image::install(isolate);
    zip::install(isolate);
    isolate.add_native_docs(DOCS);
}

pub fn isolate() -> Isolate {
    let mut isolate = Isolate::new();
    install(&mut isolate);
    isolate.set_worker_setup(install);
    isolate
}
