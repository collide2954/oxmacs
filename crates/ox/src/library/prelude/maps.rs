use crate::error::Error;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::library::collections::{self, map_or_nil};
use crate::native::Drains;
use crate::value::Value;
use crate::value::map::Map;
use crate::vm::build::full;

use super::items;

pub(super) fn get_in(heap: &mut Heap, map: Value, path: Value) -> Result<Value, Error> {
    let mut found = map;
    for key in items(heap, path)? {
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
    for key in items(heap, wanted)? {
        if collections::contains(heap, map, key)?.truthy() {
            let value = collections::get(heap, &[map, key]);
            selected = selected.assoc(heap, key, value).map_err(full)?;
        }
    }
    heap.map(selected).map_err(full)
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
}
