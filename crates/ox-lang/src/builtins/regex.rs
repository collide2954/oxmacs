use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use regex::{Captures, Regex};

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::{Value, print};

const MODULE: &str = "ox/regex";

thread_local! {
    static COMPILED: RefCell<HashMap<String, Arc<Regex>>> = RefCell::new(HashMap::new());
}

pub(crate) fn compile(pattern: &str) -> Result<Arc<Regex>, Error> {
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

pub fn regex_of(heap: &Heap, name: &str, value: Value) -> Result<Arc<Regex>, Error> {
    if let Some(found) = heap.foreign_of::<Regex>(value) {
        return compile(found.as_str());
    }
    let pattern = heap.text(value).ok_or_else(|| {
        Error::new(format!(
            "{name} expects a regex or a pattern string, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })?;
    compile(pattern)
}

fn text<'a>(heap: &'a Heap, name: &str, value: Value) -> Result<&'a str, Error> {
    heap.text(value).ok_or_else(|| {
        let order = if heap.foreign_of::<Regex>(value).is_some() {
            "; it takes the text first, then the pattern"
        } else {
            ""
        };
        Error::new(format!(
            "regex/{name} expects text to search, got {}{order}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

#[derive(Default)]
struct Counted {
    byte: usize,
    chars: usize,
}

impl Counted {
    fn position(&mut self, whole: &str, byte: usize) -> Value {
        if byte < self.byte {
            *self = Self::default();
        }
        self.chars += whole[self.byte..byte].chars().count();
        self.byte = byte;
        Value::Int(i64::try_from(self.chars).unwrap_or(i64::MAX))
    }
}

fn found(
    heap: &mut Heap,
    regex: &Regex,
    whole: &str,
    captures: &Captures<'_>,
    counted: &mut Counted,
) -> Result<Value, Error> {
    let all = captures.get(0).map_or(0..0, |matched| matched.range());
    let mut groups = Vec::new();
    for group in captures.iter().skip(1) {
        groups.push(match group {
            Some(matched) => heap.string(matched.as_str())?,
            None => Value::Nil,
        });
    }
    let mut named = Vec::new();
    for name in regex.capture_names().flatten() {
        named.push(Value::keyword(name));
        named.push(match captures.name(name) {
            Some(matched) => heap.string(matched.as_str())?,
            None => Value::Nil,
        });
    }
    let matched = heap.string(&whole[all.clone()])?;
    let groups = heap.vector_from(&groups)?;
    let named = crate::vm::build::make_map(heap, &named)?;
    let start = counted.position(whole, all.start);
    let end = counted.position(whole, all.end);
    crate::vm::build::make_map(
        heap,
        &[
            Value::keyword("text"),
            matched,
            Value::keyword("start"),
            start,
            Value::keyword("end"),
            end,
            Value::keyword("groups"),
            groups,
            Value::keyword("named"),
            named,
        ],
    )
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "regex")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let regex = regex_of(heap, "regex", arguments[0])?;
            heap.shared("regex", Arc::new((*regex).clone()))
                .map_err(Error::from)
        });
    isolate
        .native(MODULE, "find")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let whole = text(heap, "find", arguments[0])?.to_owned();
            let regex = regex_of(heap, "regex/find", arguments[1])?;
            match regex.captures(&whole) {
                Some(captures) => found(heap, &regex, &whole, &captures, &mut Counted::default()),
                None => Ok(Value::Nil),
            }
        });
    isolate
        .native(MODULE, "find-all")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let whole = text(heap, "find-all", arguments[0])?.to_owned();
            let regex = regex_of(heap, "regex/find-all", arguments[1])?;
            let mut all = Vec::new();
            let mut counted = Counted::default();
            for captures in regex.captures_iter(&whole) {
                all.push(found(heap, &regex, &whole, &captures, &mut counted)?);
            }
            heap.vector_from(&all).map_err(Error::from)
        });
}
