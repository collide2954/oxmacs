use crate::error::Error;
use crate::heap::{Heap, HeapFull};
use crate::isolate::Isolate;
use crate::map::Map;
use crate::print;
use crate::value::Value;
use crate::vector::Vector;

fn full(error: HeapFull) -> Error {
    Error::new(error.to_string())
}

fn int(number: usize) -> Value {
    Value::Int(i64::try_from(number).unwrap_or(i64::MAX))
}

fn expected(heap: &Heap, name: &str, what: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} expects {what}, got {}",
        print::write(heap, value)
    ))
}

fn out_of_range(index: i64, kind: &str, count: usize, unit: &str) -> Error {
    let plural = if count == 1 { "" } else { "s" };
    Error::new(format!(
        "Index {index} is out of range for a {kind} of {count} {unit}{plural}"
    ))
}

fn index(heap: &Heap, name: &str, value: Value) -> Result<i64, Error> {
    match value {
        Value::Int(number) => Ok(number),
        _ => Err(expected(heap, name, "an integer index", value)),
    }
}

fn is_list(heap: &Heap, value: Value) -> bool {
    value == Value::Nil || heap.pair_of(value).is_some()
}

fn store(heap: &mut Heap, vector: Vector) -> Result<Value, Error> {
    heap.vector(vector).map_err(full)
}

fn store_map(heap: &mut Heap, map: Map) -> Result<Value, Error> {
    heap.map(map).map_err(full)
}

fn vector_of(heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
    let vector = Vector::from_items(heap, items).map_err(full)?;
    store(heap, vector)
}

fn map_or_nil(heap: &Heap, name: &str, collection: Value) -> Result<Map, Error> {
    if collection == Value::Nil {
        return Ok(Map::EMPTY);
    }
    heap.map_of(collection)
        .ok_or_else(|| expected(heap, name, "a map", collection))
}

fn entries(heap: &mut Heap, map: Map) -> Result<Value, Error> {
    let mut pairs = Vec::with_capacity(map.count);
    for (key, value) in map.entries(heap) {
        pairs.push(vector_of(heap, &[key, value])?);
    }
    vector_of(heap, &pairs)
}

fn dissoc(heap: &mut Heap, collection: Value, keys: &[Value]) -> Result<Value, Error> {
    if collection == Value::Nil {
        return Ok(Value::Nil);
    }
    let mut map = map_or_nil(heap, "dissoc", collection)?;
    for key in keys {
        map = map.dissoc(heap, *key).map_err(full)?;
    }
    store_map(heap, map)
}

fn conj_entries(heap: &mut Heap, map: Map, items: &[Value]) -> Result<Value, Error> {
    let mut map = map;
    for item in items {
        let pairs = if let Some(other) = heap.map_of(*item) {
            other.entries(heap)
        } else if let Some(pair) = heap.vector_of(*item).filter(|pair| pair.count == 2)
            && let (Some(key), Some(value)) = (pair.get(heap, 0), pair.get(heap, 1))
        {
            vec![(key, value)]
        } else {
            return Err(expected(
                heap,
                "conj on a map",
                "[key value] pairs or maps",
                *item,
            ));
        };
        for (key, value) in pairs {
            map = map.assoc(heap, key, value).map_err(full)?;
        }
    }
    store_map(heap, map)
}

fn count(heap: &Heap, value: Value) -> Result<Value, Error> {
    if let Some(length) = heap.length(value) {
        return Ok(int(length));
    }
    if let Some(vector) = heap.vector_of(value) {
        return Ok(int(vector.count));
    }
    if let Some(map) = heap.map_of(value) {
        return Ok(int(map.count));
    }
    if is_list(heap, value) {
        return Ok(int(heap.items(value).len()));
    }
    Err(expected(heap, "count", "a collection", value))
}

fn nth(heap: &Heap, collection: Value, wanted: Value) -> Result<Value, Error> {
    let wanted = index(heap, "nth", wanted)?;
    let position = usize::try_from(wanted).ok();
    if let Some(text) = heap.text(collection) {
        return position
            .and_then(|position| text.chars().nth(position))
            .map(Value::Char)
            .ok_or_else(|| {
                let count = heap.length(collection).unwrap_or_default();
                out_of_range(wanted, "string", count, "character")
            });
    }
    if let Some(vector) = heap.vector_of(collection) {
        return position
            .and_then(|position| vector.get(heap, position))
            .ok_or_else(|| out_of_range(wanted, "vector", vector.count, "item"));
    }
    if !is_list(heap, collection) {
        return Err(expected(heap, "nth", "a collection", collection));
    }
    let items = heap.items(collection);
    position
        .and_then(|position| items.get(position).copied())
        .ok_or_else(|| out_of_range(wanted, "list", items.len(), "item"))
}

fn get(heap: &Heap, arguments: &[Value]) -> Value {
    let fallback = arguments.get(2).copied().unwrap_or(Value::Nil);
    let (Some(collection), Some(key)) = (arguments.first(), arguments.get(1)) else {
        return fallback;
    };
    if let Some(map) = heap.map_of(*collection) {
        return map.get(heap, *key).unwrap_or(fallback);
    }
    let Value::Int(wanted) = key else {
        return fallback;
    };
    let Ok(position) = usize::try_from(*wanted) else {
        return fallback;
    };
    let found = if let Some(vector) = heap.vector_of(*collection) {
        vector.get(heap, position)
    } else {
        heap.text(*collection)
            .and_then(|text| text.chars().nth(position))
            .map(Value::Char)
    };
    found.unwrap_or(fallback)
}

fn vec(heap: &mut Heap, collection: Value) -> Result<Value, Error> {
    if heap.vector_of(collection).is_some() {
        return Ok(collection);
    }
    if let Some(map) = heap.map_of(collection) {
        return entries(heap, map);
    }
    let items: Vec<Value> = if let Some(text) = heap.text(collection) {
        text.chars().map(Value::Char).collect()
    } else if is_list(heap, collection) {
        heap.items(collection)
    } else {
        return Err(expected(heap, "vec", "a collection", collection));
    };
    let vector = Vector::from_items(heap, &items).map_err(full)?;
    store(heap, vector)
}

fn conj(heap: &mut Heap, collection: Value, items: &[Value]) -> Result<Value, Error> {
    if let Some(vector) = heap.vector_of(collection) {
        let grown = items
            .iter()
            .try_fold(vector, |vector, item| vector.conj(heap, *item))
            .map_err(full)?;
        return store(heap, grown);
    }
    if let Some(map) = heap.map_of(collection) {
        return conj_entries(heap, map, items);
    }
    if is_list(heap, collection) {
        return items
            .iter()
            .try_fold(collection, |list, item| heap.pair(*item, list))
            .map_err(full);
    }
    Err(expected(heap, "conj", "a collection", collection))
}

fn assoc(heap: &mut Heap, collection: Value, pairs: &[Value]) -> Result<Value, Error> {
    if pairs.len() % 2 == 1 {
        return Err(Error::new("assoc takes a collection and key value pairs"));
    }
    let map = if collection == Value::Nil {
        Some(Map::EMPTY)
    } else {
        heap.map_of(collection)
    };
    if let Some(mut map) = map {
        for pair in pairs.chunks(2) {
            if let [key, value] = pair {
                map = map.assoc(heap, *key, *value).map_err(full)?;
            }
        }
        return store_map(heap, map);
    }
    let Some(mut vector) = heap.vector_of(collection) else {
        return Err(expected(heap, "assoc", "a map or vector", collection));
    };
    for pair in pairs.chunks(2) {
        let [key, value] = pair else {
            continue;
        };
        let wanted = index(heap, "assoc", *key)?;
        let changed = match usize::try_from(wanted) {
            Ok(position) => vector.assoc(heap, position, *value).map_err(full)?,
            Err(_) => None,
        };
        vector = changed.ok_or_else(|| out_of_range(wanted, "vector", vector.count, "item"))?;
    }
    store(heap, vector)
}

fn pop(heap: &mut Heap, collection: Value) -> Result<Value, Error> {
    if let Some(vector) = heap.vector_of(collection) {
        let popped = vector
            .pop(heap)
            .map_err(full)?
            .ok_or_else(|| Error::new("Cannot pop an empty vector"))?;
        return store(heap, popped);
    }
    if is_list(heap, collection) {
        return Ok(heap
            .pair_of(collection)
            .map_or(Value::Nil, |pair| pair.tail));
    }
    Err(expected(heap, "pop", "a vector or list", collection))
}

fn peek(heap: &Heap, collection: Value) -> Result<Value, Error> {
    if let Some(vector) = heap.vector_of(collection) {
        return Ok(vector
            .count
            .checked_sub(1)
            .and_then(|last| vector.get(heap, last))
            .unwrap_or(Value::Nil));
    }
    if is_list(heap, collection) {
        return Ok(heap
            .pair_of(collection)
            .map_or(Value::Nil, |pair| pair.head));
    }
    Err(expected(heap, "peek", "a vector or list", collection))
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native("count", 1, Some(1), |heap, arguments| {
        count(heap, arguments[0])
    });
    isolate.define_native("nth", 2, Some(2), |heap, arguments| {
        nth(heap, arguments[0], arguments[1])
    });
    isolate.define_native("get", 2, Some(3), |heap, arguments| {
        Ok(get(heap, arguments))
    });
    isolate.define_native("vector", 0, None, vector_of);
    isolate.define_native("map?", 1, Some(1), |heap, arguments| {
        Ok(Value::Bool(heap.map_of(arguments[0]).is_some()))
    });
    isolate.define_native("dissoc", 1, None, |heap, arguments| {
        dissoc(heap, arguments[0], &arguments[1..])
    });
    isolate.define_native("contains?", 2, Some(2), |heap, arguments| {
        let map = map_or_nil(heap, "contains?", arguments[0])?;
        Ok(Value::Bool(map.get(heap, arguments[1]).is_some()))
    });
    isolate.define_native("keys", 1, Some(1), |heap, arguments| {
        let map = map_or_nil(heap, "keys", arguments[0])?;
        let keys: Vec<Value> = map.entries(heap).into_iter().map(|(key, _)| key).collect();
        vector_of(heap, &keys)
    });
    isolate.define_native("vals", 1, Some(1), |heap, arguments| {
        let map = map_or_nil(heap, "vals", arguments[0])?;
        let values: Vec<Value> = map
            .entries(heap)
            .into_iter()
            .map(|(_, value)| value)
            .collect();
        vector_of(heap, &values)
    });
    isolate.define_native("entries", 1, Some(1), |heap, arguments| {
        let map = map_or_nil(heap, "entries", arguments[0])?;
        entries(heap, map)
    });
    isolate.define_native("vector?", 1, Some(1), |heap, arguments| {
        Ok(Value::Bool(heap.vector_of(arguments[0]).is_some()))
    });
    isolate.define_native("vec", 1, Some(1), |heap, arguments| vec(heap, arguments[0]));
    isolate.define_native("list", 0, None, |heap, arguments| {
        heap.list(arguments.iter().copied()).map_err(full)
    });
    isolate.define_native("conj", 1, None, |heap, arguments| {
        conj(heap, arguments[0], &arguments[1..])
    });
    isolate.define_native("assoc", 3, None, |heap, arguments| {
        assoc(heap, arguments[0], &arguments[1..])
    });
    isolate.define_native("pop", 1, Some(1), |heap, arguments| pop(heap, arguments[0]));
    isolate.define_native("peek", 1, Some(1), |heap, arguments| {
        peek(heap, arguments[0])
    });
}
