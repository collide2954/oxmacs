mod vec;

use ox::Isolate;
use ox::build::make_vector;
use ox::error::{Error, ErrorKind};
use ox::heap::Heap;
use ox::value::{Value, print, to_float};

use crate::vec::Vec2;

pub const DOCS: &str = include_str!("docs.ox");

pub const MODULES: &[(&str, &str)] = &[(
    "rodeo/vec",
    "Plain [x y] vectors for games: adding, scaling, lengths, angles and turning",
)];

pub fn install(isolate: &mut Isolate) {
    vec::install(isolate);
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
        .ok_or_else(|| wrong(heap, name, "a point as [x y] or {x: … y: …}", value))??;
    Ok(Vec2::new(x, y))
}

pub(crate) fn point_value(heap: &mut Heap, point: Vec2) -> Result<Value, Error> {
    make_vector(heap, &[Value::Float(point.x), Value::Float(point.y)])
}
