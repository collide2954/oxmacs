mod natives;
mod rect;
mod vec;

use ox_lang::build::make_vector;
use ox_lang::error::Error;
use ox_lang::heap::Heap;
use ox_lang::native::{expected, field, number};
use ox_lang::value::Value;

pub(crate) use natives::install;
pub use rect::Rect;
pub use vec::Vec2;

pub(crate) fn items(heap: &Heap, value: Value) -> Option<Vec<Value>> {
    heap.vector_of(value).map(|vector| vector.items(heap))
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
        .ok_or_else(|| expected(heap, name, "a point as [x y] or a map of :x and :y", value))??;
    Ok(Vec2::new(x, y))
}

pub(crate) fn rect(heap: &Heap, name: &str, value: Value) -> Result<Rect, Error> {
    let [x, y, w, h] = numbers(heap, name, value, ["x", "y", "w", "h"]).ok_or_else(|| {
        expected(
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
