use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::library::collections::{self, expected, is_list};
use crate::native::{Control, Drains};
use crate::value::Value;
use crate::value::equal::equal;
use crate::value::map::Map;
use crate::vm::Runtime;
use crate::vm::build::{full, make_vector};
use crate::vm::routine::{Next, Routine, Started, finished, is_generator};

use super::{int, items, less, plus, smaller, stretch};

fn drop(heap: &mut Heap, count: Value, items: &[Value]) -> Result<Vec<Value>, Error> {
    let total = int(items.len());
    let start = smaller(heap, count, total)?;
    stretch(heap, items, start, total)
}

fn take(heap: &mut Heap, count: Value, items: &[Value]) -> Result<Vec<Value>, Error> {
    let end = smaller(heap, count, int(items.len()))?;
    stretch(heap, items, Value::Int(0), end)
}

fn zero(value: Value) -> bool {
    match value {
        Value::Int(number) => number == 0,
        Value::Float(number) => number.abs() <= 0.0,
        _ => false,
    }
}

fn chunk(heap: &mut Heap, name: &str, size: Value, coll: Value) -> Result<Vec<Value>, Error> {
    let items = items(heap, coll)?;
    let total = int(items.len());
    let mut groups = Vec::new();
    let mut at = Value::Int(0);
    while less(heap, at, total)? {
        let next = plus(heap, at, size)?;
        if zero(size) {
            return Err(expected(heap, name, "a size other than 0", size));
        }
        let group = match (at, size) {
            (Value::Int(start), Value::Int(step)) if start >= 0 && step > 0 => {
                let start = usize::try_from(start).unwrap_or(usize::MAX);
                let step = usize::try_from(step).unwrap_or(usize::MAX);
                items[start..start.saturating_add(step).min(items.len())].to_vec()
            }
            _ => {
                let rest = drop(heap, at, &items)?;
                take(heap, size, &rest)?
            }
        };
        groups.push(make_vector(heap, &group)?);
        at = next;
    }
    Ok(groups)
}

fn flatten(heap: &mut Heap, coll: Value) -> Result<Value, Error> {
    let mut out = Vec::new();
    let mut levels = vec![items(heap, coll)?.into_iter()];
    while let Some(level) = levels.last_mut() {
        let Some(item) = level.next() else {
            levels.pop();
            continue;
        };
        if heap.vector_of(item).is_some() || is_list(heap, item) {
            levels.push(items(heap, item)?.into_iter());
        } else {
            out.push(item);
        }
    }
    make_vector(heap, &out)
}

fn distinct(heap: &mut Heap, coll: Value) -> Result<Value, Error> {
    let mut seen = Map::EMPTY;
    let mut out = Vec::new();
    for item in items(heap, coll)? {
        if seen.get(heap, item).is_none() {
            seen = seen.assoc(heap, item, Value::Bool(true)).map_err(full)?;
            out.push(item);
        }
    }
    make_vector(heap, &out)
}

fn frequencies(heap: &mut Heap, coll: Value) -> Result<Value, Error> {
    let mut counts = Map::EMPTY;
    for item in items(heap, coll)? {
        let seen = counts.get(heap, item).unwrap_or(Value::Int(0));
        let count = plus(heap, Value::Int(1), seen)?;
        counts = counts.assoc(heap, item, count).map_err(full)?;
    }
    heap.map(counts).map_err(full)
}

pub(super) fn zipped(heap: &mut Heap, colls: &[Value]) -> Result<Vec<Vec<Value>>, Error> {
    if colls.is_empty() {
        return Err(Error::new("map takes at least 2 arguments, got 1").of(ErrorKind::Arity));
    }
    let all = colls
        .iter()
        .map(|coll| items(heap, *coll))
        .collect::<Result<Vec<_>, Error>>()?;
    let shortest = all.iter().map(Vec::len).min().unwrap_or(0);
    Ok((0..shortest)
        .map(|at| all.iter().map(|items| items[at]).collect())
        .collect())
}

fn zip(heap: &mut Heap, colls: &[Value]) -> Result<Value, Error> {
    let tuples = zipped(heap, colls)?
        .iter()
        .map(|tuple| make_vector(heap, tuple))
        .collect::<Result<Vec<_>, Error>>()?;
    make_vector(heap, &tuples)
}

fn interleave(heap: &mut Heap, colls: &[Value]) -> Result<Value, Error> {
    let items: Vec<Value> = zipped(heap, colls)?.into_iter().flatten().collect();
    make_vector(heap, &items)
}

fn repeat(heap: &mut Heap, count: Value, item: Value) -> Result<Value, Error> {
    let range = collections::range(heap, &[count])?;
    let times = heap.vector_of(range).map_or(0, |range| range.count);
    make_vector(heap, &vec![item; times])
}

#[derive(Debug)]
struct Pull {
    generator: Value,
    count: Value,
    at: Value,
    out: Vec<Value>,
    pulled: bool,
}

impl Routine for Pull {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        if self.pulled {
            if finished(&runtime.heap, self.generator) {
                return make_vector(&mut runtime.heap, &self.out).map(Next::Return);
            }
            self.out.push(value);
            self.at = plus(&mut runtime.heap, self.at, Value::Int(1))?;
        }
        if !less(&runtime.heap, self.at, self.count)? {
            return make_vector(&mut runtime.heap, &self.out).map(Next::Return);
        }
        self.pulled = true;
        let next = runtime.control_value(Control::Next);
        Ok(Next::Call(next, std::slice::from_ref(&self.generator)))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.extend([self.generator, self.count, self.at]);
        values.extend(&self.out);
    }
}

pub(super) fn install(isolate: &mut Isolate) {
    isolate.define_draining("distinct", 1, Some(1), Drains::At(0), |heap, arguments| {
        distinct(heap, arguments[0])
    });
    isolate.define_routine("take", 2, Some(2), None, |runtime, _, arguments| {
        let (count, coll) = (arguments[0], arguments[1]);
        if is_generator(&runtime.heap, coll) {
            return Ok(Started::Running(Box::new(Pull {
                generator: coll,
                count,
                at: Value::Int(0),
                out: Vec::new(),
                pulled: false,
            })));
        }
        let heap = &mut runtime.heap;
        let items = items(heap, coll)?;
        let taken = take(heap, count, &items)?;
        make_vector(heap, &taken).map(Started::Done)
    });
    isolate.define_draining("drop", 2, Some(2), Drains::At(1), |heap, arguments| {
        let items = items(heap, arguments[1])?;
        let kept = drop(heap, arguments[0], &items)?;
        make_vector(heap, &kept)
    });
    isolate.define_draining("chunk", 2, Some(2), Drains::At(1), |heap, arguments| {
        let groups = chunk(heap, "chunk", arguments[0], arguments[1])?;
        make_vector(heap, &groups)
    });
    isolate.define_draining("partition", 2, Some(2), Drains::At(1), |heap, arguments| {
        let size = arguments[0];
        let groups = chunk(heap, "partition", size, arguments[1])?;
        let whole: Vec<Value> = groups
            .into_iter()
            .filter(|group| {
                let length = heap.vector_of(*group).map_or(0, |group| group.count);
                equal(heap, int(length), size)
            })
            .collect();
        make_vector(heap, &whole)
    });
    isolate.define_draining("flatten", 1, Some(1), Drains::At(0), |heap, arguments| {
        flatten(heap, arguments[0])
    });
    isolate.define_draining(
        "frequencies",
        1,
        Some(1),
        Drains::At(0),
        |heap, arguments| frequencies(heap, arguments[0]),
    );
    isolate.define_draining("zip", 0, None, Drains::From(0), zip);
    isolate.define_draining("interleave", 0, None, Drains::From(0), interleave);
    isolate.define_native("repeat", 2, Some(2), |heap, arguments| {
        repeat(heap, arguments[0], arguments[1])
    });
}
