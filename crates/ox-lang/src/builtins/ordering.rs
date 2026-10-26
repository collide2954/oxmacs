use std::cmp::Ordering;

use crate::builtins::collections;
use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::equal::identical;
use crate::value::number::Number;
use crate::value::print;
use crate::vm::build::make_vector;

fn numeric(heap: &Heap, value: Value) -> Option<Number> {
    match value {
        Value::Int(_) | Value::Float(_) => Number::of(heap, "", value).ok(),
        _ => heap
            .bigint_of(value)
            .map(|number| Number::Big(number.clone())),
    }
}

pub(crate) fn order(heap: &Heap, a: Value, b: Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Nil, Value::Nil) => return Some(Ordering::Equal),
        (Value::Nil, _) => return Some(Ordering::Less),
        (_, Value::Nil) => return Some(Ordering::Greater),
        (Value::Bool(x), Value::Bool(y)) => return Some(x.cmp(&y)),
        (Value::Char(x), Value::Char(y)) => return Some(x.cmp(&y)),
        (Value::Keyword(x), Value::Keyword(y)) | (Value::Symbol(x), Value::Symbol(y)) => {
            return Some(x.name().cmp(y.name()));
        }
        _ => {}
    }
    if let (Some(x), Some(y)) = (numeric(heap, a), numeric(heap, b)) {
        return x.compare(&y);
    }
    if let (Some(x), Some(y)) = (heap.text(a), heap.text(b)) {
        return Some(x.cmp(y));
    }
    let (x, y) = (heap.elements(a)?, heap.elements(b)?);
    for (left, right) in x.iter().zip(&y) {
        match order(heap, *left, *right)? {
            Ordering::Equal => {}
            unequal => return Some(unequal),
        }
    }
    Some(x.len().cmp(&y.len()))
}

fn incomparable(heap: &Heap, name: &str, a: Value, b: Value) -> Error {
    Error::new(format!(
        "{name} can't compare {} with {}",
        print::brief(heap, a),
        print::brief(heap, b)
    ))
    .of(ErrorKind::Type)
}

pub(crate) fn sort(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let original = collections::sequence(heap, "sort", arguments[0])?;
    let mut items = original.clone();
    let mut failure = None;
    items.sort_by(|a, b| {
        order(heap, *a, *b).unwrap_or_else(|| {
            failure.get_or_insert((*a, *b));
            Ordering::Equal
        })
    });
    if let Some((a, b)) = failure {
        let position = |value: Value| original.iter().position(|item| identical(*item, value));
        let (first, second) = if position(a) <= position(b) {
            (a, b)
        } else {
            (b, a)
        };
        return Err(incomparable(heap, "sort", first, second));
    }
    make_vector(heap, &items)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native("sort", 1, Some(1), sort);
    isolate.define_native("compare", 2, Some(2), |heap, arguments| {
        let ordering = order(heap, arguments[0], arguments[1])
            .ok_or_else(|| incomparable(heap, "compare", arguments[0], arguments[1]))?;
        Ok(Value::Int(match ordering {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        }))
    });
}
