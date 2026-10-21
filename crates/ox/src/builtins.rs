use std::cmp::Ordering;
use std::io::{self, Write};

use crate::collections;
use crate::equal::{equal, hash, identical};
use crate::error::Error;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::number::Number;
use crate::print;
use crate::value::Value;

type Step = fn(Number, Number) -> Result<Number, Error>;

fn total(
    heap: &Heap,
    name: &str,
    start: Number,
    arguments: &[Value],
    step: Step,
) -> Result<Number, Error> {
    arguments.iter().try_fold(start, |total, value| {
        step(total, Number::of(heap, name, *value)?)
    })
}

fn fold(
    heap: &mut Heap,
    name: &str,
    start: Number,
    arguments: &[Value],
    step: Step,
) -> Result<Value, Error> {
    total(heap, name, start, arguments, step)?.store(heap)
}

fn reduce(
    heap: &mut Heap,
    name: &str,
    arguments: &[Value],
    single: fn(Number) -> Result<Number, Error>,
    step: Step,
) -> Result<Value, Error> {
    let Some((first, rest)) = arguments.split_first() else {
        return Err(Error::new(format!(
            "{name} takes at least 1 argument, got 0"
        )));
    };
    let first = Number::of(heap, name, *first)?;
    if rest.is_empty() {
        return single(first)?.store(heap);
    }
    fold(heap, name, first, rest, step)
}

fn chain(
    heap: &Heap,
    name: &str,
    arguments: &[Value],
    holds: fn(Ordering) -> bool,
) -> Result<Value, Error> {
    let numbers = arguments
        .iter()
        .map(|value| Number::of(heap, name, *value))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Bool(
        numbers
            .windows(2)
            .all(|pair| pair[0].compare(&pair[1]).is_some_and(holds)),
    ))
}

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

fn all_equal(heap: &Heap, arguments: &[Value]) -> bool {
    arguments
        .windows(2)
        .all(|pair| equal(heap, pair[0], pair[1]))
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native("=", 1, None, |heap, arguments| {
        Ok(Value::Bool(all_equal(heap, arguments)))
    });
    isolate.define_native("not=", 1, None, |heap, arguments| {
        Ok(Value::Bool(!all_equal(heap, arguments)))
    });
    isolate.define_native("identical?", 2, Some(2), |_, arguments| {
        Ok(Value::Bool(identical(arguments[0], arguments[1])))
    });
    isolate.define_native("hash", 1, Some(1), |heap, arguments| {
        Ok(Value::Int(i64::from_ne_bytes(
            hash(heap, arguments[0]).to_ne_bytes(),
        )))
    });
    isolate.define_native("==", 1, None, |heap, arguments| {
        chain(heap, "==", arguments, Ordering::is_eq)
    });
    isolate.define_native("str", 0, None, join);
    collections::install(isolate);

    isolate.define_native("print", 0, None, |heap, arguments| {
        output(heap, arguments, false)
    });
    isolate.define_native("println", 0, None, |heap, arguments| {
        output(heap, arguments, true)
    });
    isolate.define_native("+", 0, None, |heap, arguments| {
        fold(heap, "+", Number::Int(0), arguments, |a, b| Ok(a.add(&b)))
    });
    isolate.define_native("*", 0, None, |heap, arguments| {
        fold(heap, "*", Number::Int(1), arguments, |a, b| {
            Ok(a.multiply(&b))
        })
    });
    isolate.define_native("-", 1, None, |heap, arguments| {
        reduce(
            heap,
            "-",
            arguments,
            |only| Ok(only.negate()),
            |a, b| Ok(a.subtract(&b)),
        )
    });
    isolate.define_native("/", 1, None, |heap, arguments| {
        reduce(
            heap,
            "/",
            arguments,
            |only| Number::Int(1).divide(&only),
            |a, b| a.divide(&b),
        )
    });
    isolate.define_native("//", 2, Some(2), |heap, arguments| {
        reduce(heap, "//", arguments, Ok, |a, b| a.floor_divide(&b))
    });
    isolate.define_native("<", 1, None, |heap, arguments| {
        chain(heap, "<", arguments, Ordering::is_lt)
    });
    isolate.define_native("<=", 1, None, |heap, arguments| {
        chain(heap, "<=", arguments, Ordering::is_le)
    });
    isolate.define_native(">", 1, None, |heap, arguments| {
        chain(heap, ">", arguments, Ordering::is_gt)
    });
    isolate.define_native(">=", 1, None, |heap, arguments| {
        chain(heap, ">=", arguments, Ordering::is_ge)
    });
}
