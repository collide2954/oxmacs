pub(crate) mod collections;
mod errors;
mod files;
mod http;
mod json;
mod numbers;
mod ordering;
pub(crate) mod paths;
mod patterns;
mod process;
mod rules;
mod strings;
mod tasks;
mod time;
mod types;

use std::io::{self, Write};

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::equal::{equal, hash, identical};
use crate::value::print;

fn output(heap: &Heap, arguments: &[Value], newline: bool) -> Result<Value, Error> {
    let mut text = arguments
        .iter()
        .map(|value| print::display(heap, *value))
        .collect::<Vec<_>>()
        .join(" ");
    if newline {
        text.push('\n');
    }
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|error| Error::new(error.to_string()))?;
    Ok(Value::Nil)
}

fn join(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let text: String = arguments
        .iter()
        .filter(|value| **value != Value::Nil)
        .map(|value| print::display(heap, *value))
        .collect();
    heap.string(&text)
        .map_err(|full| Error::new(full.to_string()))
}

fn not_an_atom(heap: &Heap, name: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} expects an atom, got {}",
        print::write(heap, value)
    ))
    .of(ErrorKind::Type)
}

fn all_equal(heap: &Heap, arguments: &[Value]) -> bool {
    arguments
        .windows(2)
        .all(|pair| equal(heap, pair[0], pair[1]))
}

pub fn install(isolate: &mut Isolate) {
    numbers::install(isolate);
    errors::install(isolate);
    patterns::install(isolate);
    rules::install(isolate);
    types::install(isolate);
    tasks::install(isolate);
    time::install(isolate);
    json::install(isolate);
    http::install(isolate);
    strings::install(isolate);
    ordering::install(isolate);
    paths::install(isolate);
    files::install(isolate);
    process::install(isolate);
    isolate.define_native("=", 1, None, |heap, arguments| {
        Ok(Value::Bool(all_equal(heap, arguments)))
    });
    isolate.define_native("not=", 1, None, |heap, arguments| {
        Ok(Value::Bool(!all_equal(heap, arguments)))
    });
    isolate.define_native("identical?", 2, Some(2), |_, arguments| {
        Ok(Value::Bool(identical(arguments[0], arguments[1])))
    });
    isolate.define_native("atom", 1, Some(1), |heap, arguments| {
        heap.atom(arguments[0])
            .map_err(|full| Error::new(full.to_string()))
    });
    isolate.define_native("atom?", 1, Some(1), |heap, arguments| {
        Ok(Value::Bool(heap.atom_get(arguments[0]).is_some()))
    });
    isolate.define_native("deref", 1, Some(1), |heap, arguments| {
        heap.atom_get(arguments[0])
            .ok_or_else(|| not_an_atom(heap, "deref", arguments[0]))
    });
    isolate.define_native("reset!", 2, Some(2), |heap, arguments| {
        if heap.atom_set(arguments[0], arguments[1]) {
            Ok(arguments[1])
        } else {
            Err(not_an_atom(heap, "reset!", arguments[0]))
        }
    });
    isolate.define_native("hash", 1, Some(1), |heap, arguments| {
        Ok(Value::Int(i64::from_ne_bytes(
            hash(heap, arguments[0]).to_ne_bytes(),
        )))
    });
    isolate.define_native("str", 0, None, join);
    collections::install(isolate);

    isolate.define_native("print", 0, None, |heap, arguments| {
        output(heap, arguments, false)
    });
    isolate.define_native("println", 0, None, |heap, arguments| {
        output(heap, arguments, true)
    });
}
