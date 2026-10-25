use crate::error::{Error, ErrorKind};
use crate::heap::{Heap, HeapFull};
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::map::Map;
use crate::value::print;
use crate::value::record::Record;
use crate::value::vector::Vector;

fn full(error: HeapFull) -> Error {
    Error::new(error.to_string())
}

fn int(number: usize) -> Value {
    Value::Int(i64::try_from(number).unwrap_or(i64::MAX))
}

pub(crate) fn expected(heap: &Heap, name: &str, what: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} expects {what}, got {}",
        print::brief(heap, value)
    ))
    .of(ErrorKind::Type)
}

pub(crate) fn out_of_range(index: i64, kind: &str, count: usize, unit: &str) -> Error {
    let plural = if count == 1 { "" } else { "s" };
    Error::new(format!(
        "Index {index} is out of range for a {kind} of {count} {unit}{plural}"
    ))
    .of(ErrorKind::Range)
}

pub(crate) fn index(heap: &Heap, name: &str, value: Value) -> Result<i64, Error> {
    match value {
        Value::Int(number) => Ok(number),
        _ => Err(expected(heap, name, "an integer index", value)),
    }
}

pub(crate) fn is_list(heap: &Heap, value: Value) -> bool {
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

pub(crate) fn map_or_nil(heap: &Heap, name: &str, collection: Value) -> Result<Map, Error> {
    if collection == Value::Nil {
        return Ok(Map::EMPTY);
    }
    heap.map_of(collection)
        .ok_or_else(|| expected(heap, name, "a map", collection))
}

fn pairs(heap: &mut Heap, map: Map) -> Result<Vec<Value>, Error> {
    let mut pairs = Vec::with_capacity(map.count);
    for (key, value) in map.entries(heap) {
        pairs.push(vector_of(heap, &[key, value])?);
    }
    Ok(pairs)
}

fn entries(heap: &mut Heap, map: Map) -> Result<Value, Error> {
    let pairs = pairs(heap, map)?;
    vector_of(heap, &pairs)
}

fn members(heap: &Heap, set: Map) -> Vec<Value> {
    set.entries(heap)
        .into_iter()
        .map(|(item, _)| item)
        .collect()
}

pub fn sequence(heap: &mut Heap, name: &str, collection: Value) -> Result<Vec<Value>, Error> {
    if let Some(vector) = heap.vector_of(collection) {
        return Ok(vector.items(heap));
    }
    if let Some(set) = heap.set_of(collection) {
        return Ok(members(heap, set));
    }
    if let Some(map) = heap.map_of(collection) {
        return pairs(heap, map);
    }
    if let Some(text) = heap.text(collection) {
        return Ok(text.chars().map(Value::Char).collect());
    }
    if is_list(heap, collection) {
        return Ok(heap.items(collection));
    }
    Err(expected(heap, name, "a collection", collection))
}

fn store_set(heap: &mut Heap, set: Map) -> Result<Value, Error> {
    heap.set(set).map_err(full)
}

fn add(heap: &mut Heap, set: Map, items: &[Value]) -> Result<Map, Error> {
    items
        .iter()
        .try_fold(set, |set, item| set.assoc(heap, *item, Value::Bool(true)))
        .map_err(full)
}

fn set(heap: &mut Heap, collection: Value) -> Result<Value, Error> {
    if heap.set_of(collection).is_some() {
        return Ok(collection);
    }
    let items = sequence(heap, "set", collection)?;
    let set = add(heap, Map::EMPTY, &items)?;
    store_set(heap, set)
}

fn disj(heap: &mut Heap, collection: Value, items: &[Value]) -> Result<Value, Error> {
    if collection == Value::Nil {
        return Ok(Value::Nil);
    }
    let Some(mut set) = heap.set_of(collection) else {
        return Err(expected(heap, "disj", "a set", collection));
    };
    for item in items {
        set = set.dissoc(heap, *item).map_err(full)?;
    }
    store_set(heap, set)
}

pub(crate) fn contains(heap: &Heap, collection: Value, key: Value) -> Result<Value, Error> {
    if collection == Value::Nil {
        return Ok(Value::Bool(false));
    }
    let Some(map) = heap.map_of(collection).or_else(|| heap.set_of(collection)) else {
        return Err(expected(heap, "contains?", "a map or set", collection));
    };
    Ok(Value::Bool(map.get(heap, key).is_some()))
}

fn with(heap: &mut Heap, value: Value, pairs: &[Value]) -> Result<Value, Error> {
    let Some(record) = heap.record_of(value).cloned() else {
        return Err(expected(heap, "with", "a record", value));
    };
    if pairs.len() % 2 == 1 {
        return Err(Error::new("with takes a record and field value pairs"));
    }
    let mut fields = record.fields.to_vec();
    for pair in pairs.chunks(2) {
        let [key, field] = pair else {
            continue;
        };
        let slot = match key {
            Value::Keyword(name) => record.slot(heap, *name),
            _ => None,
        };
        let Some(place) = slot.and_then(|slot| fields.get_mut(slot)) else {
            let name = match key {
                Value::Keyword(name) => name.name().to_owned(),
                other => print::brief(heap, *other),
            };
            return Err(Error::new(format!(
                "{} has no field {name}",
                record.type_name(heap)
            )));
        };
        *place = *field;
    }
    heap.record(Record {
        kind: record.kind,
        fields: fields.into(),
    })
    .map_err(full)
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

pub(crate) fn count(heap: &Heap, value: Value) -> Result<Value, Error> {
    counted(heap, "count", value)
}

fn counted(heap: &Heap, native: &str, value: Value) -> Result<Value, Error> {
    if let Some(length) = heap.length(value) {
        return Ok(int(length));
    }
    if let Some(vector) = heap.vector_of(value) {
        return Ok(int(vector.count));
    }
    if let Some(map) = heap.map_of(value).or_else(|| heap.set_of(value)) {
        return Ok(int(map.count));
    }
    if is_list(heap, value) {
        return Ok(int(heap.items(value).len()));
    }
    Err(expected(heap, native, "a collection", value))
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

pub fn get(heap: &Heap, arguments: &[Value]) -> Value {
    let fallback = arguments.get(2).copied().unwrap_or(Value::Nil);
    let (Some(collection), Some(key)) = (arguments.first(), arguments.get(1)) else {
        return fallback;
    };
    if let Some(record) = heap.record_of(*collection) {
        let Value::Keyword(name) = key else {
            return fallback;
        };
        return record.get(heap, *name).unwrap_or(fallback);
    }
    if let Some(map) = heap.map_of(*collection) {
        return map.get(heap, *key).unwrap_or(fallback);
    }
    if let Some(set) = heap.set_of(*collection) {
        return set.entry(heap, *key).map_or(fallback, |(item, _)| item);
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

pub(crate) fn vec(heap: &mut Heap, collection: Value) -> Result<Value, Error> {
    if heap.vector_of(collection).is_some() {
        return Ok(collection);
    }
    let items = sequence(heap, "vec", collection)?;
    vector_of(heap, &items)
}

pub(crate) fn conj(heap: &mut Heap, collection: Value, items: &[Value]) -> Result<Value, Error> {
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
    if let Some(set) = heap.set_of(collection) {
        let grown = add(heap, set, items)?;
        return store_set(heap, grown);
    }
    if is_list(heap, collection) {
        return items
            .iter()
            .try_fold(collection, |list, item| heap.pair(*item, list))
            .map_err(full);
    }
    Err(expected(heap, "conj", "a collection", collection))
}

pub(crate) fn assoc(heap: &mut Heap, collection: Value, pairs: &[Value]) -> Result<Value, Error> {
    if pairs.len() % 2 == 1 {
        return Err(Error::new("assoc takes a collection and key value pairs"));
    }
    if heap.record_of(collection).is_some() {
        return with(heap, collection, pairs);
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

pub(crate) fn first(heap: &mut Heap, collection: Value) -> Result<Value, Error> {
    if let Some(pair) = heap.pair_of(collection) {
        return Ok(pair.head);
    }
    if let Some(vector) = heap.vector_of(collection) {
        return Ok(vector.get(heap, 0).unwrap_or(Value::Nil));
    }
    let items = sequence(heap, "first", collection)?;
    Ok(items.first().copied().unwrap_or(Value::Nil))
}

pub(crate) fn rest(heap: &mut Heap, collection: Value) -> Result<Value, Error> {
    if is_list(heap, collection) {
        return Ok(heap
            .pair_of(collection)
            .map_or(Value::Nil, |pair| pair.tail));
    }
    let items = sequence(heap, "rest", collection)?;
    vector_of(heap, items.get(1..).unwrap_or_default())
}

pub(crate) fn range(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let mut numbers = Vec::with_capacity(arguments.len());
    for value in arguments {
        let Value::Int(number) = value else {
            return Err(expected(heap, "range", "integers", *value));
        };
        numbers.push(*number);
    }
    let (start, end, step) = match numbers.as_slice() {
        [end] => (0, *end, 1),
        [start, end] => (*start, *end, 1),
        [start, end, step] => (*start, *end, *step),
        _ => (0, 0, 1),
    };
    if step == 0 {
        return Err(Error::new("range needs a step that is not zero"));
    }
    let mut items = Vec::new();
    let mut at = start;
    while (step > 0 && at < end) || (step < 0 && at > end) {
        items.push(Value::Int(at));
        let Some(next) = at.checked_add(step) else {
            break;
        };
        at = next;
    }
    vector_of(heap, &items)
}

fn cons(heap: &mut Heap, item: Value, collection: Value) -> Result<Value, Error> {
    if is_list(heap, collection) {
        return heap.pair(item, collection).map_err(full);
    }
    let mut items = vec![item];
    items.extend(sequence(heap, "cons", collection)?);
    heap.list(items.into_iter()).map_err(full)
}

fn install_sequences(isolate: &mut Isolate) {
    isolate.define_native("first", 1, Some(1), |heap, arguments| {
        first(heap, arguments[0])
    });
    isolate.define_native("second", 1, Some(1), |heap, arguments| {
        let items = sequence(heap, "second", arguments[0])?;
        Ok(items.get(1).copied().unwrap_or(Value::Nil))
    });
    isolate.define_native("last", 1, Some(1), |heap, arguments| {
        let items = sequence(heap, "last", arguments[0])?;
        Ok(items.last().copied().unwrap_or(Value::Nil))
    });
    isolate.define_native("rest", 1, Some(1), |heap, arguments| {
        rest(heap, arguments[0])
    });
    isolate.define_native("empty?", 1, Some(1), |heap, arguments| {
        Ok(Value::Bool(
            counted(heap, "empty?", arguments[0])? == Value::Int(0),
        ))
    });
    isolate.define_native("range", 1, Some(3), range);
    isolate.define_native("concat", 0, None, |heap, arguments| {
        let mut items = Vec::new();
        for value in arguments {
            items.extend(sequence(heap, "concat", *value)?);
        }
        vector_of(heap, &items)
    });
    isolate.define_native("reverse", 1, Some(1), |heap, arguments| {
        let mut items = sequence(heap, "reverse", arguments[0])?;
        items.reverse();
        vector_of(heap, &items)
    });
    isolate.define_native("cons", 2, Some(2), |heap, arguments| {
        cons(heap, arguments[0], arguments[1])
    });
    isolate.define_native("into", 2, Some(2), |heap, arguments| {
        let items = sequence(heap, "into", arguments[1])?;
        conj(heap, arguments[0], &items)
    });
}

pub fn install(isolate: &mut Isolate) {
    install_sequences(isolate);
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
        contains(heap, arguments[0], arguments[1])
    });
    isolate.define_native("set", 1, Some(1), |heap, arguments| set(heap, arguments[0]));
    isolate.define_native("apply", 2, None, |_, _| {
        Err(Error::new("apply is handled by the VM"))
    });
    isolate.define_native("#list", 0, None, |heap, arguments| {
        heap.list(arguments.iter().copied()).map_err(full)
    });
    isolate.define_native("#vec", 1, Some(1), |heap, arguments| {
        vec(heap, arguments[0])
    });
    isolate.define_native("#concat", 0, None, |heap, arguments| {
        let mut items = Vec::new();
        for value in arguments {
            items.extend(sequence(heap, "~@", *value)?);
        }
        heap.list(items.into_iter()).map_err(full)
    });
    isolate.define_native("with", 1, None, |heap, arguments| {
        with(heap, arguments[0], &arguments[1..])
    });
    isolate.define_native("record?", 1, Some(1), |heap, arguments| {
        Ok(Value::Bool(heap.record_of(arguments[0]).is_some()))
    });
    isolate.define_native("set?", 1, Some(1), |heap, arguments| {
        Ok(Value::Bool(heap.set_of(arguments[0]).is_some()))
    });
    isolate.define_native("disj", 1, None, |heap, arguments| {
        disj(heap, arguments[0], &arguments[1..])
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
