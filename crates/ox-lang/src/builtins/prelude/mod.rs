mod errors;
mod functions;
mod macros;
mod maps;
mod profiling;
mod sequences;
mod testing;

use std::cmp::Ordering;

use crate::builtins::{collections, numbers};
use crate::error::Error;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::native::Drains;
use crate::value::Value;

fn int(count: usize) -> Value {
    Value::Int(i64::try_from(count).unwrap_or(i64::MAX))
}

fn less(heap: &Heap, a: Value, b: Value) -> Result<bool, Error> {
    Ok(numbers::chain(heap, "<", &[a, b], Ordering::is_lt)?.truthy())
}

fn plus(heap: &mut Heap, a: Value, b: Value) -> Result<Value, Error> {
    numbers::sum(heap, &[a, b])
}

fn smaller(heap: &Heap, a: Value, b: Value) -> Result<Value, Error> {
    numbers::extreme(heap, "min", &[a, b], Ordering::is_lt)
}

fn nth(heap: &Heap, items: &[Value], at: Value) -> Result<Value, Error> {
    let wanted = collections::index(heap, "nth", at)?;
    usize::try_from(wanted)
        .ok()
        .and_then(|position| items.get(position).copied())
        .ok_or_else(|| collections::out_of_range(wanted, "vector", items.len(), "item"))
}

fn stretch(
    heap: &mut Heap,
    items: &[Value],
    start: Value,
    end: Value,
) -> Result<Vec<Value>, Error> {
    if let (Value::Int(first), Value::Int(last)) = (start, end)
        && first >= 0
    {
        let from = usize::try_from(first).unwrap_or(usize::MAX);
        let to = usize::try_from(last).unwrap_or(0);
        if from >= to {
            return Ok(Vec::new());
        }
        if to <= items.len() {
            return Ok(items[from..to].to_vec());
        }
    }
    let mut out = Vec::new();
    let mut at = start;
    while less(heap, at, end)? {
        out.push(nth(heap, items, at)?);
        at = plus(heap, at, Value::Int(1))?;
    }
    Ok(out)
}

pub(crate) fn install(isolate: &mut Isolate) {
    errors::install(isolate);
    functions::install(isolate);
    isolate.define_draining("vec", 1, Some(1), Drains::At(0), |heap, arguments| {
        collections::vec(heap, arguments[0])
    });
    sequences::install(isolate);
    maps::install(isolate);
    testing::install(isolate);
    profiling::install(isolate);
    macros::install(isolate);
}
