pub mod animation;
pub mod camera;
pub mod collide;
pub mod color;
pub mod crypto;
mod csv;
pub mod ease;
mod files;
pub use files::{remove, to_trash};
pub mod geometry;
mod http;
pub mod image;
mod lexer;
mod net;
mod process;
pub mod random;
pub mod tiled;
mod zip;

use ox_lang::Isolate;
use ox_lang::build::make_map;
use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};

pub const DOCS: &str = include_str!("docs.ox");

const MODULES: &[(&str, &str)] = &[
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
    ("ox/http", "HTTP requests"),
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
    (
        "ox/random",
        "Random numbers, from the system or from seeded generators kept in atoms so replays repeat them",
    ),
    (
        "ox/geometry",
        "Plain [x y] points and [x y w h] rectangles: adding, lengths, angles, edges, overlaps and clamping",
    ),
    ("ox/ease", "Easing curves for tweens, from linear to bounce"),
    (
        "ox/camera",
        "Cameras as plain maps: following, bounds, zoom, shake and world to screen points",
    ),
    (
        "ox/collide",
        "Overlaps, swept boxes that slide along walls, tile collision and spatial grids",
    ),
    (
        "ox/tiled",
        "Tiled .tmx maps as plain data: layers, tiles, objects, properties and tilesets",
    ),
    (
        "ox/lexer",
        "Line lexers that highlight code from a token regex, blocks that run across lines and word lists",
    ),
];

pub(crate) fn text<'a>(heap: &'a Heap, name: &str, value: Value) -> Result<&'a str, Error> {
    heap.text(value).ok_or_else(|| {
        Error::new(format!(
            "{name} expects a string, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

pub(crate) fn map_value(heap: &mut Heap, pairs: &[(&str, Value)]) -> Result<Value, Error> {
    let items: Vec<Value> = pairs
        .iter()
        .flat_map(|(key, value)| [Value::keyword(key), *value])
        .collect();
    make_map(heap, &items)
}

pub fn install(isolate: &mut Isolate) {
    for (name, summary) in MODULES {
        isolate.add_module(name, summary);
    }
    http::install(isolate);
    net::install(isolate);
    crypto::install(isolate);
    csv::install(isolate);
    files::install(isolate);
    process::install(isolate);
    image::install(isolate);
    zip::install(isolate);
    random::install(isolate);
    geometry::install(isolate);
    ease::install(isolate);
    camera::install(isolate);
    collide::install(isolate);
    tiled::install(isolate);
    lexer::install(isolate);
    ox_synth::install(isolate);
    isolate.add_native_docs(DOCS);
}

pub fn isolate() -> Isolate {
    let mut isolate = Isolate::new();
    install(&mut isolate);
    isolate.set_worker_setup(install);
    isolate
}
