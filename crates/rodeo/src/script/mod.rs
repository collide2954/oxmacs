mod camera;
mod collide;
mod ease;
mod random;
mod rect;
mod tiled;
mod vec;

use ox_lang::Isolate;
use ox_lang::build::{make_map, make_vector};
use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::value::{Value, print, to_float};

use crate::rect::Rect;
use crate::vec::Vec2;

pub const DOCS: &str = include_str!("docs.ox");

const MODULES: &[(&str, &str)] = &[
    (
        "rodeo/vec",
        "Plain [x y] vectors for games: adding, scaling, lengths, angles and turning",
    ),
    (
        "rodeo/rect",
        "Plain [x y w h] rectangles: edges, centers, overlaps, unions and clamping",
    ),
    (
        "rodeo/ease",
        "Easing curves for tweens, from linear to bounce",
    ),
    (
        "rodeo/random",
        "Seeded random numbers kept in atoms, so replays and rewinds repeat them",
    ),
    (
        "rodeo/camera",
        "Cameras as plain maps: following, bounds, zoom, shake and world to screen points",
    ),
    (
        "rodeo/collide",
        "Overlaps, swept boxes that slide along walls, tile collision and spatial grids",
    ),
    (
        "rodeo/tiled",
        "Tiled .tmx maps as plain data: layers, tiles, objects, properties and tilesets",
    ),
];

pub fn install(isolate: &mut Isolate) {
    for (name, summary) in MODULES {
        isolate.add_module(name, summary);
    }
    vec::install(isolate);
    rect::install(isolate);
    ease::install(isolate);
    random::install(isolate);
    camera::install(isolate);
    collide::install(isolate);
    tiled::install(isolate);
    isolate.add_native_docs(DOCS);
}

pub(crate) fn wrong(heap: &Heap, name: &str, what: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} expects {what}, got {}",
        print::brief(heap, value)
    ))
    .of(ErrorKind::Type)
}

pub(crate) fn number(heap: &Heap, name: &str, value: Value) -> Result<f64, Error> {
    match value {
        Value::Int(number) => Ok(to_float(number)),
        Value::Float(number) => Ok(number),
        _ => Err(wrong(heap, name, "a number", value)),
    }
}

pub(crate) fn items(heap: &Heap, value: Value) -> Option<Vec<Value>> {
    heap.vector_of(value).map(|vector| vector.items(heap))
}

pub(crate) fn field(heap: &Heap, value: Value, key: &str) -> Option<Value> {
    heap.map_of(value)?.get(heap, Value::keyword(key))
}

fn numbers<const COUNT: usize>(
    heap: &Heap,
    name: &str,
    value: Value,
    keys: [&str; COUNT],
) -> Option<Result<[f64; COUNT], Error>> {
    let found: Vec<Value> = match items(heap, value) {
        Some(found) if found.len() == COUNT => found,
        Some(_) => return None,
        None => keys
            .iter()
            .map(|key| field(heap, value, key))
            .collect::<Option<_>>()?,
    };
    let mut parsed = [0.0; COUNT];
    for (slot, item) in parsed.iter_mut().zip(found) {
        match number(heap, name, item) {
            Ok(number) => *slot = number,
            Err(error) => return Some(Err(error)),
        }
    }
    Some(Ok(parsed))
}

pub(crate) fn point(heap: &Heap, name: &str, value: Value) -> Result<Vec2, Error> {
    let [x, y] = numbers(heap, name, value, ["x", "y"])
        .ok_or_else(|| wrong(heap, name, "a point as [x y] or a map of :x and :y", value))??;
    Ok(Vec2::new(x, y))
}

pub(crate) fn rect(heap: &Heap, name: &str, value: Value) -> Result<Rect, Error> {
    let [x, y, w, h] = numbers(heap, name, value, ["x", "y", "w", "h"]).ok_or_else(|| {
        wrong(
            heap,
            name,
            "a rect as [x y w h] or a map of :x, :y, :w and :h",
            value,
        )
    })??;
    Ok(Rect::new(x, y, w, h))
}

pub(crate) fn point_value(heap: &mut Heap, point: Vec2) -> Result<Value, Error> {
    make_vector(heap, &[Value::Float(point.x), Value::Float(point.y)])
}

pub(crate) fn rect_value(heap: &mut Heap, rect: Rect) -> Result<Value, Error> {
    make_vector(
        heap,
        &[
            Value::Float(rect.x),
            Value::Float(rect.y),
            Value::Float(rect.w),
            Value::Float(rect.h),
        ],
    )
}

pub(crate) fn map_value(heap: &mut Heap, pairs: &[(&str, Value)]) -> Result<Value, Error> {
    let items: Vec<Value> = pairs
        .iter()
        .flat_map(|(key, value)| [Value::keyword(key), *value])
        .collect();
    make_map(heap, &items)
}

pub(crate) fn bits(value: i64) -> u64 {
    u64::from_ne_bytes(value.to_ne_bytes())
}

pub(crate) fn signed(value: u64) -> i64 {
    i64::from_ne_bytes(value.to_ne_bytes())
}

pub(crate) fn name_of(heap: &Heap, value: Value) -> Option<&str> {
    match value {
        Value::Keyword(word) => Some(word.name()),
        _ => heap.text(value),
    }
}
