use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::library::collections::{self, expected, is_list};
use crate::library::ordering;
use crate::native::{Control, Drains};
use crate::value::Value;
use crate::value::equal::equal;
use crate::value::map::Map;
use crate::vm::Runtime;
use crate::vm::build::{full, make_vector};
use crate::vm::routine::{Forward, Next, Routine, Started, finished, is_generator};

use super::{int, less, plus, smaller, stretch};
use collections::sequence;

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
    let items = sequence(heap, name, coll)?;
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
    let mut levels = vec![sequence(heap, "flatten", coll)?.into_iter()];
    while let Some(level) = levels.last_mut() {
        let Some(item) = level.next() else {
            levels.pop();
            continue;
        };
        if heap.vector_of(item).is_some() || is_list(heap, item) {
            levels.push(sequence(heap, "flatten", item)?.into_iter());
        } else {
            out.push(item);
        }
    }
    make_vector(heap, &out)
}

fn distinct(heap: &mut Heap, coll: Value) -> Result<Value, Error> {
    let mut seen = Map::EMPTY;
    let mut out = Vec::new();
    for item in sequence(heap, "distinct", coll)? {
        if seen.get(heap, item).is_none() {
            seen = seen.assoc(heap, item, Value::Bool(true)).map_err(full)?;
            out.push(item);
        }
    }
    make_vector(heap, &out)
}

fn frequencies(heap: &mut Heap, coll: Value) -> Result<Value, Error> {
    let mut counts = Map::EMPTY;
    for item in sequence(heap, "frequencies", coll)? {
        let seen = counts.get(heap, item).unwrap_or(Value::Int(0));
        let count = plus(heap, Value::Int(1), seen)?;
        counts = counts.assoc(heap, item, count).map_err(full)?;
    }
    heap.map(counts).map_err(full)
}

pub(super) fn zipped(
    heap: &mut Heap,
    name: &str,
    colls: &[Value],
) -> Result<Vec<Vec<Value>>, Error> {
    if colls.is_empty() {
        return Err(Error::new("map takes at least 2 arguments, got 1").of(ErrorKind::Arity));
    }
    let all = colls
        .iter()
        .map(|coll| sequence(heap, name, *coll))
        .collect::<Result<Vec<_>, Error>>()?;
    let shortest = all.iter().map(Vec::len).min().unwrap_or(0);
    Ok((0..shortest)
        .map(|at| all.iter().map(|items| items[at]).collect())
        .collect())
}

fn zip(heap: &mut Heap, colls: &[Value]) -> Result<Value, Error> {
    let tuples = zipped(heap, "zip", colls)?
        .iter()
        .map(|tuple| make_vector(heap, tuple))
        .collect::<Result<Vec<_>, Error>>()?;
    make_vector(heap, &tuples)
}

fn interleave(heap: &mut Heap, colls: &[Value]) -> Result<Value, Error> {
    let items: Vec<Value> = zipped(heap, "interleave", colls)?
        .into_iter()
        .flatten()
        .collect();
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Map,
    MapCat,
    Filter,
    Remove,
    Keep,
    Some,
    Every,
    Find,
    TakeWhile,
    DropWhile,
    GroupBy,
    SortBy,
}

#[derive(Debug)]
struct Each {
    mode: Mode,
    function: Value,
    columns: Vec<Vec<Value>>,
    length: usize,
    at: usize,
    out: Vec<Value>,
    groups: Map,
    arguments: Vec<Value>,
}

impl Each {
    fn start(
        name: &str,
        mode: Mode,
        runtime: &mut Runtime,
        arguments: &[Value],
    ) -> Result<Started, Error> {
        let columns = arguments[1..]
            .iter()
            .map(|coll| sequence(&mut runtime.heap, name, *coll))
            .collect::<Result<Vec<_>, Error>>()?;
        let length = columns.iter().map(Vec::len).min().unwrap_or(0);
        Ok(Started::Running(Box::new(Self {
            mode,
            function: arguments[0],
            columns,
            length,
            at: 0,
            out: Vec::new(),
            groups: Map::EMPTY,
            arguments: Vec::new(),
        })))
    }

    fn took(&mut self, heap: &mut Heap, value: Value) -> Result<Option<Value>, Error> {
        let item = self.columns[0][self.at - 1];
        match self.mode {
            Mode::Map | Mode::MapCat | Mode::SortBy => self.out.push(value),
            Mode::Filter if value.truthy() => self.out.push(item),
            Mode::Remove if !value.truthy() => self.out.push(item),
            Mode::Keep if value != Value::Nil => self.out.push(value),
            Mode::Some if value.truthy() => return Ok(Some(value)),
            Mode::Every if !value.truthy() => return Ok(Some(Value::Bool(false))),
            Mode::Find if value.truthy() => return Ok(Some(item)),
            Mode::TakeWhile if !value.truthy() => return make_vector(heap, &self.out).map(Some),
            Mode::TakeWhile => self.out.push(item),
            Mode::DropWhile if !value.truthy() => {
                return make_vector(heap, &self.columns[0][self.at - 1..]).map(Some);
            }
            Mode::GroupBy => {
                let group = match self.groups.get(heap, value) {
                    Some(group) => group,
                    None => make_vector(heap, &[])?,
                };
                let grown = collections::conj(heap, group, &[item])?;
                self.groups = self.groups.assoc(heap, value, grown).map_err(full)?;
            }
            Mode::Filter
            | Mode::Remove
            | Mode::Keep
            | Mode::Some
            | Mode::Every
            | Mode::Find
            | Mode::DropWhile => {}
        }
        Ok(None)
    }

    fn finish(&self, heap: &mut Heap) -> Result<Value, Error> {
        match self.mode {
            Mode::Map | Mode::Filter | Mode::Remove | Mode::Keep | Mode::TakeWhile => {
                make_vector(heap, &self.out)
            }
            Mode::MapCat => {
                let mut items = Vec::new();
                for part in &self.out {
                    items.extend(sequence(heap, "concat", *part)?);
                }
                make_vector(heap, &items)
            }
            Mode::Some | Mode::Find => Ok(Value::Nil),
            Mode::Every => Ok(Value::Bool(true)),
            Mode::DropWhile => make_vector(heap, &[]),
            Mode::GroupBy => heap.map(self.groups).map_err(full),
            Mode::SortBy => sorted(heap, &self.columns[0], &self.out),
        }
    }
}

impl Routine for Each {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        if self.at > 0
            && let Some(done) = self.took(&mut runtime.heap, value)?
        {
            return Ok(Next::Return(done));
        }
        if self.at >= self.length {
            return self.finish(&mut runtime.heap).map(Next::Return);
        }
        let at = self.at;
        self.at += 1;
        self.arguments.clear();
        self.arguments
            .extend(self.columns.iter().map(|column| column[at]));
        Ok(Next::Call(self.function, &self.arguments))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.push(self.function);
        values.extend(self.columns.iter().flatten());
        values.extend(&self.out);
        values.extend(&self.arguments);
        values.extend(self.groups.children());
    }
}

fn sorted(heap: &mut Heap, items: &[Value], keys: &[Value]) -> Result<Value, Error> {
    let pairs = keys
        .iter()
        .enumerate()
        .map(|(at, key)| make_vector(heap, &[*key, int(at)]))
        .collect::<Result<Vec<_>, Error>>()?;
    let pairs = make_vector(heap, &pairs)?;
    let order = ordering::sort(heap, &[pairs])?;
    let mut out = Vec::with_capacity(items.len());
    for pair in sequence(heap, "sort", order)? {
        let at = heap.vector_of(pair).and_then(|pair| pair.get(heap, 1));
        if let Some(Value::Int(at)) = at
            && let Some(item) = usize::try_from(at).ok().and_then(|at| items.get(at))
        {
            out.push(*item);
        }
    }
    make_vector(heap, &out)
}

#[derive(Debug)]
struct Reduce {
    function: Value,
    items: Vec<Value>,
    at: usize,
    pair: [Value; 2],
}

impl Routine for Reduce {
    fn resume(&mut self, _: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        if self.at > 0 {
            self.pair[0] = value;
        }
        let Some(item) = self.items.get(self.at) else {
            return Ok(Next::Return(self.pair[0]));
        };
        self.pair[1] = *item;
        self.at += 1;
        Ok(Next::Call(self.function, &self.pair))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.push(self.function);
        values.extend(&self.items);
        values.extend(self.pair);
    }
}

fn reduce(runtime: &mut Runtime, arguments: &[Value]) -> Result<Started, Error> {
    let function = arguments[0];
    let given = &arguments[1..];
    let mut items = sequence(
        &mut runtime.heap,
        "reduce",
        given.last().copied().unwrap_or(Value::Nil),
    )?;
    let start = if given.len() == 1 {
        if items.is_empty() {
            return Ok(Started::Running(Box::new(Forward::new(
                function,
                Vec::new(),
            ))));
        }
        items.remove(0)
    } else {
        given.first().copied().unwrap_or(Value::Nil)
    };
    if items.is_empty() {
        return Ok(Started::Done(start));
    }
    Ok(Started::Running(Box::new(Reduce {
        function,
        items,
        at: 0,
        pair: [start, Value::Nil],
    })))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pulling {
    Next,
    Kept,
}

#[derive(Debug)]
struct TakeWhile {
    generator: Value,
    keep: Value,
    out: Vec<Value>,
    item: [Value; 1],
    pulling: Option<Pulling>,
}

impl Routine for TakeWhile {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        match self.pulling {
            Some(Pulling::Next) => {
                if finished(&runtime.heap, self.generator) {
                    return make_vector(&mut runtime.heap, &self.out).map(Next::Return);
                }
                self.item = [value];
                self.pulling = Some(Pulling::Kept);
                return Ok(Next::Call(self.keep, &self.item));
            }
            Some(Pulling::Kept) => {
                if !value.truthy() {
                    return make_vector(&mut runtime.heap, &self.out).map(Next::Return);
                }
                self.out.push(self.item[0]);
            }
            None => {}
        }
        self.pulling = Some(Pulling::Next);
        let next = runtime.control_value(Control::Next);
        Ok(Next::Call(next, std::slice::from_ref(&self.generator)))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.extend([self.generator, self.keep]);
        values.extend(&self.out);
        values.extend(self.item);
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
        let items = sequence(heap, "take", coll)?;
        let taken = take(heap, count, &items)?;
        make_vector(heap, &taken).map(Started::Done)
    });
    isolate.define_draining("drop", 2, Some(2), Drains::At(1), |heap, arguments| {
        let items = sequence(heap, "drop", arguments[1])?;
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
    install_callers(isolate);
}

fn install_callers(isolate: &mut Isolate) {
    let each: [(&str, Mode, Option<usize>, Drains); 11] = [
        ("map", Mode::Map, None, Drains::From(1)),
        ("mapcat", Mode::MapCat, None, Drains::From(1)),
        ("filter", Mode::Filter, Some(2), Drains::At(1)),
        ("remove", Mode::Remove, Some(2), Drains::At(1)),
        ("keep", Mode::Keep, Some(2), Drains::At(1)),
        ("some", Mode::Some, Some(2), Drains::At(1)),
        ("every?", Mode::Every, Some(2), Drains::At(1)),
        ("find", Mode::Find, Some(2), Drains::At(1)),
        ("drop-while", Mode::DropWhile, Some(2), Drains::At(1)),
        ("group-by", Mode::GroupBy, Some(2), Drains::At(1)),
        ("sort-by", Mode::SortBy, Some(2), Drains::At(1)),
    ];
    for (name, mode, most, drains) in each {
        isolate.define_routine(name, 2, most, Some(drains), move |runtime, _, arguments| {
            Each::start(name, mode, runtime, arguments)
        });
    }
    isolate.define_routine("take-while", 2, Some(2), None, |runtime, _, arguments| {
        if !is_generator(&runtime.heap, arguments[1]) {
            return Each::start("take-while", Mode::TakeWhile, runtime, arguments);
        }
        Ok(Started::Running(Box::new(TakeWhile {
            generator: arguments[1],
            keep: arguments[0],
            out: Vec::new(),
            item: [Value::Nil],
            pulling: None,
        })))
    });
    isolate.define_routine(
        "reduce",
        1,
        None,
        Some(Drains::Last),
        |runtime, _, arguments| reduce(runtime, arguments),
    );
}
