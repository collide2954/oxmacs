use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use regex::{Captures, Regex};

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::{Value, print};
use crate::vm::build::full;

const MODULE: &str = "ox/regex";

thread_local! {
    static COMPILED: RefCell<HashMap<String, Arc<Regex>>> = RefCell::new(HashMap::new());
}

fn compile(pattern: &str) -> Result<Arc<Regex>, Error> {
    if let Some(found) = COMPILED.with_borrow(|compiled| compiled.get(pattern).cloned()) {
        return Ok(found);
    }
    let made = Regex::new(pattern).map_err(|error| {
        let shown = error.to_string();
        let reason = shown
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .trim_start_matches("error: ")
            .to_owned();
        Error::new(format!("Invalid regex {pattern}: {reason}")).of(ErrorKind::Range)
    })?;
    let made = Arc::new(made);
    COMPILED.with_borrow_mut(|compiled| compiled.insert(pattern.to_owned(), made.clone()));
    Ok(made)
}

fn regex_of(heap: &Heap, name: &str, value: Value) -> Result<Arc<Regex>, Error> {
    if let Some(found) = heap.foreign_of::<Regex>(value) {
        return compile(found.as_str());
    }
    let pattern = heap.text(value).ok_or_else(|| {
        Error::new(format!(
            "regex/{name} expects a regex or a pattern string, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })?;
    compile(pattern)
}

fn text<'a>(heap: &'a Heap, name: &str, value: Value) -> Result<&'a str, Error> {
    heap.text(value).ok_or_else(|| {
        Error::new(format!(
            "regex/{name} expects text to search, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

fn position(whole: &str, byte: usize) -> Value {
    Value::Int(i64::try_from(whole[..byte].chars().count()).unwrap_or(i64::MAX))
}

fn found(
    heap: &mut Heap,
    regex: &Regex,
    whole: &str,
    captures: &Captures<'_>,
) -> Result<Value, Error> {
    let all = captures.get(0).map_or(0..0, |matched| matched.range());
    let mut groups = Vec::new();
    for group in captures.iter().skip(1) {
        groups.push(match group {
            Some(matched) => heap.string(matched.as_str()).map_err(full)?,
            None => Value::Nil,
        });
    }
    let mut named = Vec::new();
    for name in regex.capture_names().flatten() {
        named.push(Value::keyword(name));
        named.push(match captures.name(name) {
            Some(matched) => heap.string(matched.as_str()).map_err(full)?,
            None => Value::Nil,
        });
    }
    let matched = heap.string(&whole[all.clone()]).map_err(full)?;
    let groups = heap.vector_from(&groups).map_err(full)?;
    let named = crate::vm::build::make_map(heap, &named)?;
    crate::vm::build::make_map(
        heap,
        &[
            Value::keyword("text"),
            matched,
            Value::keyword("start"),
            position(whole, all.start),
            Value::keyword("end"),
            position(whole, all.end),
            Value::keyword("groups"),
            groups,
            Value::keyword("named"),
            named,
        ],
    )
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native("regex", 1, Some(1), |heap, arguments| {
        let regex = regex_of(heap, "compile", arguments[0])?;
        heap.shared("regex", Arc::new((*regex).clone()))
            .map_err(full)
    });
    isolate.define_native_in(MODULE, "matches?", 2, Some(2), |heap, arguments| {
        let regex = regex_of(heap, "matches?", arguments[0])?;
        Ok(Value::Bool(regex.is_match(text(
            heap,
            "matches?",
            arguments[1],
        )?)))
    });
    isolate.define_native_in(MODULE, "find", 2, Some(2), |heap, arguments| {
        let regex = regex_of(heap, "find", arguments[0])?;
        let whole = text(heap, "find", arguments[1])?.to_owned();
        match regex.captures(&whole) {
            Some(captures) => found(heap, &regex, &whole, &captures),
            None => Ok(Value::Nil),
        }
    });
    isolate.define_native_in(MODULE, "find-all", 2, Some(2), |heap, arguments| {
        let regex = regex_of(heap, "find-all", arguments[0])?;
        let whole = text(heap, "find-all", arguments[1])?.to_owned();
        let mut all = Vec::new();
        for captures in regex.captures_iter(&whole) {
            all.push(found(heap, &regex, &whole, &captures)?);
        }
        heap.vector_from(&all).map_err(full)
    });
    isolate.define_native_in(MODULE, "replace", 3, Some(3), |heap, arguments| {
        let regex = regex_of(heap, "replace", arguments[0])?;
        let whole = text(heap, "replace", arguments[1])?;
        let with = text(heap, "replace", arguments[2])?;
        let replaced = regex.replace_all(whole, with).into_owned();
        heap.string(&replaced).map_err(full)
    });
    isolate.define_native_in(MODULE, "split", 2, Some(2), |heap, arguments| {
        let regex = regex_of(heap, "split", arguments[0])?;
        let whole = text(heap, "split", arguments[1])?.to_owned();
        let parts = regex
            .split(&whole)
            .map(|part| heap.string(part).map_err(full))
            .collect::<Result<Vec<_>, _>>()?;
        heap.vector_from(&parts).map_err(full)
    });
}
