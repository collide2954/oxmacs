pub(crate) mod collections;
pub(crate) mod errors;
mod json;
mod math;
pub(crate) mod numbers;
pub(crate) mod ordering;
pub(crate) mod paths;
mod patterns;
mod prelude;
mod regex;
mod rules;
mod strings;
mod tasks;
mod time;
mod types;
mod xml;

use std::io::{self, Write};

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::equal::{equal, hash, identical};
use crate::value::print;

const MODULES: &[(&str, &str)] = &[
    (
        "ox",
        "Numbers, collections, text, records, errors, pattern matching, tasks, channels and promises, there without an import",
    ),
    (
        "ox/string",
        "Text: searching, splitting, joining, trimming, padding and changing case",
    ),
    (
        "ox/regex",
        "Regular expressions: matching, finding, replacing and splitting",
    ),
    ("ox/json", "Reading and writing JSON"),
    (
        "ox/xml",
        "Reading XML into maps, finding elements along paths, and writing XML",
    ),
    (
        "ox/math",
        "Roots, powers, logarithms, angles, clamping and random numbers",
    ),
    (
        "ox/time",
        "Dates and times: the clock, parsing, formatting, time zones and durations",
    ),
    (
        "ox/path",
        "Taking paths apart and joining them, the home folder and ~ expansion",
    ),
];
const OUTSIDE_MODULES: [&str; 4] = ["ox/file", "ox/http", "ox/net", "ox/process"];

const OUTSIDE_NATIVES: [&str; 8] = [
    "print",
    "println",
    "profile-report",
    "worker",
    "image/open",
    "image/probe",
    "image/save",
    "tiled/load",
];

pub(crate) fn reaches_outside(module: &str, shown: &str) -> bool {
    OUTSIDE_MODULES.contains(&module) || OUTSIDE_NATIVES.contains(&shown)
}

pub(crate) fn output(heap: &Heap, arguments: &[Value], newline: bool) -> Result<Value, Error> {
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

pub(crate) fn not_an_atom(heap: &Heap, name: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} expects an atom, got {}",
        print::brief(heap, value)
    ))
    .of(ErrorKind::Type)
}

fn all_equal(heap: &Heap, arguments: &[Value]) -> bool {
    arguments
        .windows(2)
        .all(|pair| equal(heap, pair[0], pair[1]))
}

pub fn install(isolate: &mut Isolate) {
    for (name, summary) in MODULES {
        isolate.add_module(name, summary);
    }
    numbers::install(isolate);
    math::install(isolate);
    errors::install(isolate);
    patterns::install(isolate);
    rules::install(isolate);
    types::install(isolate);
    tasks::install(isolate);
    time::install(isolate);
    json::install(isolate);
    xml::install(isolate);
    strings::install(isolate);
    ordering::install(isolate);
    paths::install(isolate);
    regex::install(isolate);
    isolate
        .native("ox", "=")
        .arity(1, None)
        .function(|heap, arguments| Ok(Value::Bool(all_equal(heap, arguments))));
    isolate
        .native("ox", "not=")
        .arity(1, None)
        .function(|heap, arguments| Ok(Value::Bool(!all_equal(heap, arguments))));
    isolate
        .native("ox", "identical?")
        .arity(2, Some(2))
        .function(|_, arguments| Ok(Value::Bool(identical(arguments[0], arguments[1]))));
    isolate
        .native("ox", "atom")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            heap.atom(arguments[0])
                .map_err(|full| Error::new(full.to_string()))
        });
    isolate
        .native("ox", "atom?")
        .arity(1, Some(1))
        .function(|heap, arguments| Ok(Value::Bool(heap.atom_get(arguments[0]).is_some())));
    isolate
        .native("ox", "deref")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            heap.atom_get(arguments[0])
                .ok_or_else(|| not_an_atom(heap, "deref", arguments[0]))
        });
    isolate
        .native("ox", "reset!")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            if heap.atom_set(arguments[0], arguments[1]) {
                Ok(arguments[1])
            } else {
                Err(not_an_atom(heap, "reset!", arguments[0]))
            }
        });
    isolate
        .native("ox", "compare-and-set!")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let current = heap
                .atom_get(arguments[0])
                .ok_or_else(|| not_an_atom(heap, "compare-and-set!", arguments[0]))?;
            if !identical(current, arguments[1]) {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(heap.atom_set(arguments[0], arguments[2])))
        });
    isolate
        .native("ox", "hash")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Ok(Value::Int(i64::from_ne_bytes(
                hash(heap, arguments[0]).to_ne_bytes(),
            )))
        });
    isolate.native("ox", "str").arity(0, None).function(join);
    isolate
        .native("ox", "pr-str")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let shown = print::write(heap, arguments[0]);
            heap.string(&shown)
                .map_err(|full| Error::new(full.to_string()))
        });
    collections::install(isolate);
    prelude::install(isolate);

    isolate
        .native("ox", "print")
        .arity(0, None)
        .outside()
        .function(|heap, arguments| output(heap, arguments, false));
    isolate
        .native("ox", "println")
        .arity(0, None)
        .outside()
        .function(|heap, arguments| output(heap, arguments, true));
}
