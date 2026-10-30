mod adjust;
mod colors;
mod draw;
mod edit;
mod filters;
mod geometry;
mod io;

use std::sync::Arc;

use image::RgbaImage;

pub use colors::set_faces;

use ox_lang::Isolate;
use ox_lang::build::{make_map, make_vector};
use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::portable::{Failure, Portable};
use ox_lang::symbol::Sym;
use ox_lang::value::{Value, print};

const MODULE: &str = "ox/image";
const KIND: &str = "image";
const MOST_SIDE: u32 = 32_768;
const MOST_PIXELS: u64 = 134_217_728;

fn room(name: &str, width: u32, height: u32) -> Result<(), String> {
    if width <= MOST_SIDE
        && height <= MOST_SIDE
        && u64::from(width) * u64::from(height) <= MOST_PIXELS
    {
        return Ok(());
    }
    Err(format!(
        "image/{name} would make a {width}×{height} image; images can be at most {MOST_SIDE} pixels a side and {} megapixels",
        MOST_PIXELS / 1_000_000
    ))
}

fn roomy(name: &str, width: u32, height: u32) -> Result<(), Error> {
    room(name, width, height).map_err(|message| Error::new(message).of(ErrorKind::Range))
}

#[derive(Debug, Clone)]
pub struct Image {
    pub pixels: RgbaImage,
    pub format: Option<String>,
    pub alpha: bool,
    pub exif: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
}

impl Image {
    pub fn new(pixels: RgbaImage) -> Self {
        Self {
            pixels,
            format: None,
            alpha: true,
            exif: None,
            icc: None,
            xmp: None,
        }
    }

    fn with(&self, pixels: RgbaImage) -> Self {
        Self {
            pixels,
            format: self.format.clone(),
            alpha: self.alpha,
            exif: None,
            icc: self.icc.clone(),
            xmp: None,
        }
    }
}

pub fn image_of(heap: &Heap, value: Value) -> Option<Arc<Image>> {
    heap.shared_of::<Image>(value)
}

fn wanted(heap: &Heap, name: &str, value: Value) -> Result<Arc<Image>, Error> {
    image_of(heap, value).ok_or_else(|| {
        Error::new(format!(
            "image/{name} expects an image, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

fn stored(heap: &mut Heap, image: Image) -> Result<Value, Error> {
    heap.shared(KIND, Arc::new(image)).map_err(Error::from)
}

fn portable(image: Image) -> Portable {
    Portable::Foreign(Sym::intern(KIND), Arc::new(image))
}

fn later(
    heap: &mut Heap,
    name: &str,
    work: impl FnOnce() -> Result<Image, String> + Send + 'static,
) -> Result<Value, Error> {
    heap.background(&format!("ox image/{name}"), move || {
        work().map(portable).map_err(Failure::from)
    })
}

fn text<'a>(heap: &'a Heap, name: &str, what: &str, value: Value) -> Result<&'a str, Error> {
    heap.text(value).ok_or_else(|| {
        Error::new(format!(
            "image/{name} expects {what}, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

fn number(heap: &Heap, name: &str, what: &str, value: Value) -> Result<f64, Error> {
    match value {
        Value::Int(number) => Ok(ox_lang::value::to_float(number)),
        Value::Float(number) => Ok(number),
        _ => Err(Error::new(format!(
            "image/{name} expects {what} as a number, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)),
    }
}

fn size(heap: &Heap, name: &str, what: &str, value: Value) -> Result<u32, Error> {
    match value {
        Value::Int(number) => u32::try_from(number).ok().filter(|size| *size > 0),
        _ => None,
    }
    .ok_or_else(|| {
        Error::new(format!(
            "image/{name} expects {what} as a whole number above 0, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

struct Options<'a> {
    name: &'static str,
    pairs: Vec<(&'a str, Value)>,
}

impl<'a> Options<'a> {
    fn slots(name: &'static str, known: &[&'a str], values: &[Value]) -> Self {
        let pairs = known
            .iter()
            .zip(values)
            .filter(|(_, value)| **value != Value::Nil)
            .map(|(key, value)| (*key, *value))
            .collect();
        Self { name, pairs }
    }

    fn get(&self, key: &str) -> Option<Value> {
        self.pairs
            .iter()
            .rev()
            .find(|(known, _)| *known == key)
            .map(|(_, value)| *value)
    }

    fn number(&self, heap: &Heap, key: &str, default: f64) -> Result<f64, Error> {
        self.get(key).map_or(Ok(default), |value| {
            number(heap, self.name, &format!(":{key}"), value)
        })
    }
}

fn keyword_value(name: &str) -> Value {
    Value::Keyword(Sym::intern(name))
}

fn int(number: u32) -> Value {
    Value::Int(i64::from(number))
}

fn info(heap: &mut Heap, image: &Image) -> Result<Value, Error> {
    let format = match &image.format {
        Some(format) => heap.string(format)?,
        None => Value::Nil,
    };
    make_map(
        heap,
        &[
            keyword_value("width"),
            int(image.pixels.width()),
            keyword_value("height"),
            int(image.pixels.height()),
            keyword_value("format"),
            format,
            keyword_value("alpha"),
            Value::Bool(image.alpha),
        ],
    )
}

fn color_value(heap: &mut Heap, color: image::Rgba<u8>) -> Result<Value, Error> {
    let channels: Vec<Value> = color
        .0
        .iter()
        .map(|channel| Value::Int(i64::from(*channel)))
        .collect();
    make_vector(heap, &channels)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "image?")
        .arity(1, Some(1))
        .function(|heap, arguments| Ok(Value::Bool(image_of(heap, arguments[0]).is_some())));
    isolate
        .native(MODULE, "color")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let color = colors::color(heap, "color", arguments[0])?;
            color_value(heap, color)
        });
    isolate
        .native(MODULE, "new")
        .arity(2, Some(2))
        .keywords(&["color"])
        .function(|heap, arguments| {
            let width = size(heap, "new", "a width", arguments[0])?;
            let height = size(heap, "new", "a height", arguments[1])?;
            roomy("new", width, height)?;
            let options = Options::slots("new", &["color"], &arguments[2..]);
            let color = match options.get("color") {
                Some(value) => colors::color(heap, "new", value)?,
                None => image::Rgba([0, 0, 0, 0]),
            };
            stored(
                heap,
                Image::new(RgbaImage::from_pixel(width, height, color)),
            )
        });
    isolate
        .native(MODULE, "info")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let image = wanted(heap, "info", arguments[0])?;
            info(heap, &image)
        });
    isolate
        .native(MODULE, "width")
        .arity(1, Some(1))
        .function(|heap, arguments| Ok(int(wanted(heap, "width", arguments[0])?.pixels.width())));
    isolate
        .native(MODULE, "height")
        .arity(1, Some(1))
        .function(|heap, arguments| Ok(int(wanted(heap, "height", arguments[0])?.pixels.height())));
    isolate
        .native(MODULE, "pixel")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let image = wanted(heap, "pixel", arguments[0])?;
            let place = |value: Value| match value {
                Value::Int(number) => u32::try_from(number).ok(),
                _ => None,
            };
            let pixel = place(arguments[1])
                .zip(place(arguments[2]))
                .and_then(|(x, y)| image.pixels.get_pixel_checked(x, y))
                .copied()
                .ok_or_else(|| {
                    Error::new(format!(
                        "image/pixel expects a point inside the {}×{} image, got {} {}",
                        image.pixels.width(),
                        image.pixels.height(),
                        print::brief(heap, arguments[1]),
                        print::brief(heap, arguments[2])
                    ))
                    .of(ErrorKind::Range)
                })?;
            color_value(heap, pixel)
        });
    io::install(isolate);
    geometry::install(isolate);
    adjust::install(isolate);
    filters::install(isolate);
    draw::install(isolate);
    edit::install(isolate);
}
