use std::cmp::Ordering;

use crate::error::Error;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::number::Number;

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

fn extreme(
    heap: &Heap,
    name: &str,
    arguments: &[Value],
    better: fn(Ordering) -> bool,
) -> Result<Value, Error> {
    let mut best = Value::Nil;
    let mut best_number: Option<Number> = None;
    for value in arguments {
        let number = Number::of(heap, name, *value)?;
        let wins = best_number
            .as_ref()
            .is_none_or(|current| number.compare(current).is_some_and(better));
        if wins {
            best = *value;
            best_number = Some(number);
        }
    }
    Ok(best)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native("==", 1, None, |heap, arguments| {
        chain(heap, "==", arguments, Ordering::is_eq)
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
    isolate.define_native("min", 1, None, |heap, arguments| {
        extreme(heap, "min", arguments, Ordering::is_lt)
    });
    isolate.define_native("max", 1, None, |heap, arguments| {
        extreme(heap, "max", arguments, Ordering::is_gt)
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
