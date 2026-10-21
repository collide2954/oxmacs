use std::cmp::Ordering;
use std::io::{self, Write};

use crate::error::Error;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::number::Number;
use crate::print;
use crate::value::Value;

type Step = fn(Number, Number) -> Result<Number, Error>;

fn fold(
    heap: &Heap,
    name: &str,
    start: Number,
    arguments: &[Value],
    step: Step,
) -> Result<Value, Error> {
    arguments
        .iter()
        .try_fold(start, |total, value| {
            step(total, Number::of(heap, name, *value)?)
        })
        .map(Number::value)
}

fn reduce(
    heap: &Heap,
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
        return single(first).map(Number::value);
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
            .all(|pair| pair[0].compare(pair[1]).is_some_and(holds)),
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

pub fn install(isolate: &mut Isolate) {
    isolate.define_native("print", 0, None, |heap, arguments| {
        output(heap, arguments, false)
    });
    isolate.define_native("println", 0, None, |heap, arguments| {
        output(heap, arguments, true)
    });
    isolate.define_native("+", 0, None, |heap, arguments| {
        fold(heap, "+", Number::Int(0), arguments, Number::add)
    });
    isolate.define_native("*", 0, None, |heap, arguments| {
        fold(heap, "*", Number::Int(1), arguments, Number::multiply)
    });
    isolate.define_native("-", 1, None, |heap, arguments| {
        reduce(heap, "-", arguments, Number::negate, Number::subtract)
    });
    isolate.define_native("/", 1, None, |heap, arguments| {
        reduce(
            heap,
            "/",
            arguments,
            |only| Number::Int(1).divide(only),
            Number::divide,
        )
    });
    isolate.define_native("//", 2, Some(2), |heap, arguments| {
        reduce(heap, "//", arguments, Ok, Number::floor_divide)
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
