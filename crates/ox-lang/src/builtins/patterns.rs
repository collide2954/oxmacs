use crate::builtins::collections::is_list;
use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::native::{awaiting, expected};
use crate::value::Value;
use crate::value::equal::equal;
use crate::value::print;
use crate::value::vector::Vector;
use crate::vm::build::{self, no_field};

fn items(heap: &Heap, value: Value) -> Option<Vec<Value>> {
    if let Some(vector) = heap.vector_of(value) {
        return Some(vector.items(heap));
    }
    if let Some(text) = heap.text(value) {
        return Some(text.chars().map(Value::Char).collect());
    }
    is_list(heap, value).then(|| heap.items(value))
}

fn taken_apart(heap: &Heap, value: Value) -> Result<Vec<Value>, Error> {
    items(heap, value)
        .ok_or_else(|| expected(heap, "A vector pattern", "a vector, list or string", value))
}

fn index(value: Value) -> usize {
    match value {
        Value::Int(number) => usize::try_from(number).unwrap_or(usize::MAX),
        _ => usize::MAX,
    }
}

fn has(heap: &Heap, value: Value, key: Value) -> bool {
    if let Some(map) = heap.map_of(value) {
        return map.get(heap, key).is_some();
    }
    match (heap.record_of(value), key) {
        (Some(record), Value::Keyword(name)) => record.slot(heap, name).is_some(),
        _ => false,
    }
}

fn field(heap: &Heap, value: Value, key: Value) -> Result<Value, Error> {
    if let Some(record) = heap.record_of(value) {
        let owner = record.type_name(heap);
        return match key {
            Value::Keyword(name) => record.get(heap, name).ok_or_else(|| no_field(owner, name)),
            other => Err(Error::new(format!(
                "{owner} has no field {}",
                print::brief(heap, other)
            ))),
        };
    }
    match key {
        Value::Keyword(name) => build::field(heap, value, name),
        _ if value == Value::Nil => Ok(Value::Nil),
        _ => heap
            .map_of(value)
            .map(|map| map.get(heap, key).unwrap_or(Value::Nil))
            .ok_or_else(|| expected(heap, "A map pattern", "a map or record", value)),
    }
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "#=")
        .arity(2, Some(2))
        .function(|heap, arguments| Ok(Value::Bool(equal(heap, arguments[0], arguments[1]))));
    isolate
        .native("ox", "#sequential?")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let count = index(arguments[1]);
            let fits = items(heap, arguments[0]).is_some_and(|found| {
                if arguments[2].truthy() {
                    found.len() >= count
                } else {
                    found.len() == count
                }
            });
            Ok(Value::Bool(fits))
        });
    isolate
        .native("ox", "#item")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            Ok(taken_apart(heap, arguments[0])?
                .get(index(arguments[1]))
                .copied()
                .unwrap_or(Value::Nil))
        });
    isolate
        .native("ox", "#from")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let found = taken_apart(heap, arguments[0])?;
            let rest = found.get(index(arguments[1])..).unwrap_or_default();
            let vector = Vector::from_items(heap, rest)?;
            heap.vector(vector).map_err(Error::from)
        });
    isolate
        .native("ox", "#keyed?")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Ok(Value::Bool(
                heap.map_of(arguments[0]).is_some() || heap.record_of(arguments[0]).is_some(),
            ))
        });
    isolate
        .native("ox", "#has?")
        .arity(2, Some(2))
        .function(|heap, arguments| Ok(Value::Bool(has(heap, arguments[0], arguments[1]))));
    isolate
        .native("ox", "#field")
        .arity(2, Some(2))
        .function(|heap, arguments| field(heap, arguments[0], arguments[1]));
    isolate
        .native("ox", "#instance?")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            Ok(Value::Bool(
                heap.record_of(arguments[0])
                    .is_some_and(|record| record.kind == arguments[1]),
            ))
        });
    isolate
        .native("ox", "#no-match")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Err(Error::new(format!(
                "No match for {}{}",
                print::brief(heap, arguments[0]),
                awaiting(heap, arguments[0])
            ))
            .of(ErrorKind::NoMatch))
        });
}
