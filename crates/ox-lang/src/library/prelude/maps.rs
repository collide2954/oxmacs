use crate::error::Error;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::library::collections::{self, map_or_nil};
use crate::native::Drains;
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::map::Map;
use crate::vm::Runtime;
use crate::vm::build::full;
use crate::vm::modules::PRELUDE;
use crate::vm::routine::{Next, Routine, Started, is_generator};

pub(super) fn get_in(heap: &mut Heap, map: Value, path: Value) -> Result<Value, Error> {
    let mut found = map;
    for key in collections::sequence(heap, "get-in", path)? {
        if found != Value::Nil {
            found = collections::get(heap, &[found, key]);
        }
    }
    Ok(found)
}

pub(super) fn assoc_in(
    heap: &mut Heap,
    map: Value,
    path: Value,
    value: Value,
) -> Result<Value, Error> {
    let mut levels = Vec::new();
    let mut inside = map;
    let mut path = path;
    loop {
        let key = collections::first(heap, path)?;
        let more = collections::rest(heap, path)?;
        levels.push((inside, key));
        if collections::count(heap, more)? == Value::Int(0) {
            break;
        }
        let empty = heap.map(Map::EMPTY).map_err(full)?;
        inside = collections::get(heap, &[inside, key, empty]);
        path = more;
    }
    let mut changed = value;
    for (inside, key) in levels.into_iter().rev() {
        changed = collections::assoc(heap, inside, &[key, changed])?;
    }
    Ok(changed)
}

fn merge(heap: &mut Heap, maps: &[Value]) -> Result<Value, Error> {
    let mut merged: Option<Map> = None;
    for map in maps {
        if *map == Value::Nil {
            continue;
        }
        let mut into = merged.unwrap_or(Map::EMPTY);
        for (key, _) in map_or_nil(heap, "keys", *map)?.entries(heap) {
            let value = collections::get(heap, &[*map, key]);
            into = into.assoc(heap, key, value).map_err(full)?;
        }
        merged = Some(into);
    }
    merged.map_or(Ok(Value::Nil), |map| heap.map(map).map_err(full))
}

fn select_keys(heap: &mut Heap, map: Value, wanted: Value) -> Result<Value, Error> {
    let mut selected = Map::EMPTY;
    for key in collections::sequence(heap, "select-keys", wanted)? {
        if collections::contains(heap, map, key)?.truthy() {
            let value = collections::get(heap, &[map, key]);
            selected = selected.assoc(heap, key, value).map_err(full)?;
        }
    }
    heap.map(selected).map_err(full)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    Find,
    Apply,
    Ready,
    Place,
}

#[derive(Debug)]
struct UpdateIn {
    map: Value,
    path: Value,
    function: Value,
    arguments: Vec<Value>,
    reach: Reach,
    found: [Value; 2],
}

impl Routine for UpdateIn {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        match self.reach {
            Reach::Find => {
                self.reach = Reach::Apply;
                let get_in = runtime
                    .lookup(PRELUDE, Sym::intern("get-in"))
                    .unwrap_or(Value::Nil);
                Ok(Next::Call(get_in, &self.found))
            }
            Reach::Apply | Reach::Ready => {
                if self.reach == Reach::Apply {
                    self.arguments[0] = value;
                }
                self.reach = Reach::Place;
                Ok(Next::Call(self.function, &self.arguments))
            }
            Reach::Place => {
                assoc_in(&mut runtime.heap, self.map, self.path, value).map(Next::Return)
            }
        }
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.extend([self.map, self.path, self.function]);
        values.extend(&self.arguments);
        values.extend(self.found);
    }
}

fn update_in(runtime: &mut Runtime, arguments: &[Value]) -> Result<Started, Error> {
    let (map, path, function) = (arguments[0], arguments[1], arguments[2]);
    let mut given = vec![Value::Nil];
    given.extend_from_slice(&arguments[3..]);
    let reach = if is_generator(&runtime.heap, path) {
        Reach::Find
    } else {
        given[0] = get_in(&mut runtime.heap, map, path)?;
        Reach::Ready
    };
    Ok(Started::Running(Box::new(UpdateIn {
        map,
        path,
        function,
        arguments: given,
        reach,
        found: [map, path],
    })))
}

#[derive(Debug)]
struct MergeWith {
    function: Value,
    maps: Vec<Value>,
    next_map: usize,
    current: Value,
    keys: Vec<Value>,
    next_key: usize,
    merged: Option<Map>,
    pair: [Value; 2],
    waiting: Option<Value>,
}

impl Routine for MergeWith {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        let heap = &mut runtime.heap;
        if let Some(key) = self.waiting.take() {
            let into = self.merged.unwrap_or(Map::EMPTY);
            self.merged = Some(into.assoc(heap, key, value).map_err(full)?);
        }
        loop {
            if let Some(key) = self.keys.get(self.next_key).copied() {
                self.next_key += 1;
                let into = self.merged.unwrap_or(Map::EMPTY);
                let incoming = collections::get(heap, &[self.current, key]);
                if let Some(existing) = into.get(heap, key) {
                    self.waiting = Some(key);
                    self.pair = [existing, incoming];
                    return Ok(Next::Call(self.function, &self.pair));
                }
                self.merged = Some(into.assoc(heap, key, incoming).map_err(full)?);
                continue;
            }
            let Some(map) = self.maps.get(self.next_map).copied() else {
                return match self.merged {
                    Some(merged) => heap.map(merged).map(Next::Return).map_err(full),
                    None => Ok(Next::Return(Value::Nil)),
                };
            };
            self.next_map += 1;
            if map == Value::Nil {
                continue;
            }
            self.keys = map_or_nil(heap, "keys", map)?
                .entries(heap)
                .into_iter()
                .map(|(key, _)| key)
                .collect();
            self.next_key = 0;
            self.current = map;
            self.merged = Some(self.merged.unwrap_or(Map::EMPTY));
        }
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.push(self.function);
        values.extend(&self.maps);
        values.push(self.current);
        values.extend(&self.keys);
        values.extend(self.pair);
        values.extend(self.waiting);
        if let Some(merged) = self.merged {
            values.extend(merged.children());
        }
    }
}

pub(super) fn install(isolate: &mut Isolate) {
    isolate.define_draining("get-in", 2, None, Drains::At(1), |heap, arguments| {
        let found = get_in(heap, arguments[0], arguments[1])?;
        Ok(match arguments.get(2) {
            Some(default) if found == Value::Nil => *default,
            _ => found,
        })
    });
    isolate.define_native("assoc-in", 3, Some(3), |heap, arguments| {
        assoc_in(heap, arguments[0], arguments[1], arguments[2])
    });
    isolate.define_native("merge", 0, None, merge);
    isolate.define_draining(
        "select-keys",
        2,
        Some(2),
        Drains::At(1),
        |heap, arguments| select_keys(heap, arguments[0], arguments[1]),
    );
    isolate.define_routine("update-in", 3, None, None, |runtime, _, arguments| {
        update_in(runtime, arguments)
    });
    isolate.define_routine("merge-with", 1, None, None, |_, _, arguments| {
        Ok(Started::Running(Box::new(MergeWith {
            function: arguments[0],
            maps: arguments[1..].to_vec(),
            next_map: 0,
            current: Value::Nil,
            keys: Vec::new(),
            next_key: 0,
            merged: None,
            pair: [Value::Nil; 2],
            waiting: None,
        })))
    });
}
