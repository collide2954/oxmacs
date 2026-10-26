use std::fmt::{self, Write as _};

use num_traits::ToPrimitive;
use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::print;
use crate::vm::build::{full, make_map, make_vector};

const MODULE: &str = "ox/json";

enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    List(Vec<Json>),
    Object(Vec<(String, Json)>),
}

struct JsonVisitor;

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Json;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Json, E> {
        Ok(Json::Null)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Json, E> {
        Ok(Json::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Json, E> {
        Ok(Json::Int(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Json, E> {
        Ok(i64::try_from(value).map_or_else(
            |_| Json::Float(value.to_f64().unwrap_or(f64::INFINITY)),
            Json::Int,
        ))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Json, E> {
        Ok(Json::Float(value))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Json, E> {
        Ok(Json::Text(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Json, E> {
        Ok(Json::Text(value))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut items: A) -> Result<Json, A::Error> {
        let mut list = Vec::new();
        while let Some(item) = items.next_element()? {
            list.push(item);
        }
        Ok(Json::List(list))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut entries: A) -> Result<Json, A::Error> {
        let mut object = Vec::new();
        while let Some((key, value)) = entries.next_entry()? {
            object.push((key, value));
        }
        Ok(Json::Object(object))
    }
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(JsonVisitor)
    }
}

fn value_of(heap: &mut Heap, json: Json) -> Result<Value, Error> {
    Ok(match json {
        Json::Null => Value::Nil,
        Json::Bool(flag) => Value::Bool(flag),
        Json::Int(number) => Value::Int(number),
        Json::Float(number) => Value::Float(number),
        Json::Text(text) => heap.string(&text).map_err(full)?,
        Json::List(items) => {
            let items = items
                .into_iter()
                .map(|item| value_of(heap, item))
                .collect::<Result<Vec<_>, _>>()?;
            make_vector(heap, &items)?
        }
        Json::Object(entries) => {
            let mut items = Vec::with_capacity(entries.len() * 2);
            for (key, value) in entries {
                items.push(Value::keyword(&key));
                items.push(value_of(heap, value)?);
            }
            make_map(heap, &items)?
        }
    })
}

fn read(heap: &mut Heap, value: Value) -> Result<Value, Error> {
    let Some(text) = heap.text(value) else {
        return Err(Error::new(format!(
            "json/read expects a string, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type));
    };
    let json: Json = serde_json::from_str(text)
        .map_err(|error| Error::new(format!("json/read can't read this: {error}")))?;
    value_of(heap, json)
}

fn quoted(text: &str, out: &mut String) {
    out.push_str(&serde_json::to_string(text).unwrap_or_default());
}

fn key(heap: &Heap, key: Value) -> Result<String, Error> {
    match key {
        Value::Keyword(name) | Value::Symbol(name) => Ok(name.name().to_owned()),
        _ => heap.text(key).map(str::to_owned).ok_or_else(|| {
            Error::new(format!(
                "json/write needs keyword or string keys, got {}",
                print::brief(heap, key)
            ))
        }),
    }
}

fn object(
    heap: &Heap,
    entries: impl IntoIterator<Item = (String, Value)>,
    out: &mut String,
) -> Result<(), Error> {
    out.push('{');
    for (index, (name, value)) in entries.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        quoted(&name, out);
        out.push(':');
        write(heap, value, out)?;
    }
    out.push('}');
    Ok(())
}

fn write(heap: &Heap, value: Value, out: &mut String) -> Result<(), Error> {
    let refused = || {
        Error::new(format!(
            "json/write can't write {}",
            print::brief(heap, value)
        ))
    };
    match value {
        Value::Nil => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if flag { "true" } else { "false" }),
        Value::Int(number) => {
            let _ = write!(out, "{number}");
        }
        Value::Float(number) if number.is_finite() => {
            let _ = write!(out, "{number:?}");
        }
        Value::Keyword(name) | Value::Symbol(name) => quoted(name.name(), out),
        Value::Float(_) | Value::Char(_) => return Err(refused()),
        Value::Ref(_) => {
            if let Some(text) = heap.text(value) {
                quoted(text, out);
            } else if let Some(number) = heap.bigint_of(value) {
                let _ = write!(out, "{number}");
            } else if let Some(entries) = heap.entries(value) {
                let named = entries
                    .into_iter()
                    .map(|(name, item)| Ok((key(heap, name)?, item)))
                    .collect::<Result<Vec<_>, Error>>()?;
                object(heap, named, out)?;
            } else if let Some(record) = heap.record_of(value) {
                let kind = heap.record_type_of(record.kind).ok_or_else(refused)?;
                let named = kind
                    .fields
                    .iter()
                    .map(|field| field.name().to_owned())
                    .zip(record.fields.iter().copied());
                object(heap, named, out)?;
            } else if let Some(items) = heap.elements(value) {
                out.push('[');
                for (index, item) in items.into_iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write(heap, item, out)?;
                }
                out.push(']');
            } else {
                return Err(refused());
            }
        }
    }
    Ok(())
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "read")
        .arity(1, Some(1))
        .function(|heap, arguments| read(heap, arguments[0]));
    isolate
        .native(MODULE, "write")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let mut out = String::new();
            write(heap, arguments[0], &mut out)?;
            heap.string(&out).map_err(full)
        });
}
