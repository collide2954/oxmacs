use crate::error::Error;
use crate::heap::Heap;
use crate::library::collections;
use crate::native::arguments;
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::map::Map;
use crate::value::print;
use crate::value::record::Record;
use crate::value::vector::Vector;

pub fn full(error: crate::heap::HeapFull) -> Error {
    Error::new(error.to_string())
}

pub fn no_field(owner: &str, name: Sym) -> Error {
    Error::new(format!("{owner} has no field {}", name.name()))
}

pub fn field(heap: &Heap, value: Value, name: Sym) -> Result<Value, Error> {
    if value == Value::Nil {
        return Ok(Value::Nil);
    }
    if let Some(map) = heap.map_of(value) {
        return Ok(map.get(heap, Value::Keyword(name)).unwrap_or(Value::Nil));
    }
    Err(no_field(&print::write(heap, value), name))
}

pub fn call_data(heap: &Heap, callee: Value, given: &[Value]) -> Result<Value, Error> {
    let most = if matches!(callee, Value::Keyword(_)) || heap.map_of(callee).is_some() {
        2
    } else if heap.set_of(callee).is_some() {
        1
    } else {
        return Err(Error::new(format!(
            "{} is not a function",
            print::write(heap, callee)
        )));
    };
    let Some(first) = given.first().filter(|_| given.len() <= most) else {
        let wanted = if most == 1 {
            arguments(1)
        } else {
            "1 to 2 arguments".to_owned()
        };
        return Err(Error::new(format!(
            "{} takes {wanted}, got {}",
            print::write(heap, callee),
            given.len()
        )));
    };
    let fallback = given.get(1).copied().unwrap_or(Value::Nil);
    let lookup = if matches!(callee, Value::Keyword(_)) {
        [*first, callee, fallback]
    } else {
        [callee, *first, fallback]
    };
    Ok(collections::get(heap, &lookup))
}

pub fn record_literal(heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
    let Some((kind, pairs)) = items.split_first() else {
        return Err(Error::new("A record literal lost its type"));
    };
    let Some(record_type) = heap.record_type_of(*kind) else {
        return Err(Error::new(format!(
            "{} is not a record type",
            print::write(heap, *kind)
        )));
    };
    let name = record_type.name.name();
    let names = record_type.fields.clone();
    let mut fields: Vec<Option<Value>> = vec![None; names.len()];
    for pair in pairs.chunks(2) {
        let [key, value] = pair else {
            continue;
        };
        let Value::Keyword(field) = key else {
            return Err(Error::new(format!(
                "{name} has no field {}",
                print::write(heap, *key)
            )));
        };
        let Some(place) = names
            .iter()
            .position(|known| known == field)
            .and_then(|slot| fields.get_mut(slot))
        else {
            return Err(Error::new(format!("{name} has no field {}", field.name())));
        };
        if place.replace(*value).is_some() {
            return Err(Error::new(format!(
                "Duplicate field {} in a {name} literal",
                field.name()
            )));
        }
    }
    let fields = fields
        .into_iter()
        .zip(&names)
        .map(|(value, field)| {
            value.ok_or_else(|| {
                Error::new(format!(
                    "This {name} literal is missing field {}",
                    field.name()
                ))
            })
        })
        .collect::<Result<Box<[Value]>, Error>>()?;
    heap.record(Record {
        kind: *kind,
        fields,
    })
    .map_err(full)
}

pub fn make_vector(heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
    let vector = Vector::from_items(heap, items).map_err(full)?;
    heap.vector(vector).map_err(full)
}

pub fn make_map(heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
    let mut map = Map::EMPTY;
    for pair in items.chunks(2) {
        let [key, value] = pair else {
            continue;
        };
        let grown = map.assoc(heap, *key, *value).map_err(full)?;
        if grown.count == map.count {
            return Err(Error::new(format!(
                "Duplicate key {} in a map literal",
                print::write(heap, *key)
            )));
        }
        map = grown;
    }
    heap.map(map).map_err(full)
}

pub fn make_set(heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
    let mut set = Map::EMPTY;
    for item in items {
        let grown = set.assoc(heap, *item, Value::Bool(true)).map_err(full)?;
        if grown.count == set.count {
            return Err(Error::new(format!(
                "Duplicate item {} in a set literal",
                print::write(heap, *item)
            )));
        }
        set = grown;
    }
    heap.set(set).map_err(full)
}
