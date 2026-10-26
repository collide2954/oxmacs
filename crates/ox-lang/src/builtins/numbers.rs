use std::cmp::Ordering;

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::number::{Number, to_float};

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
        return Err(
            Error::new(format!("{name} takes at least 1 argument, got 0")).of(ErrorKind::Arity),
        );
    };
    let first = Number::of(heap, name, *first)?;
    if rest.is_empty() {
        return single(first)?.store(heap);
    }
    fold(heap, name, first, rest, step)
}

pub(crate) fn quick(
    arguments: &[Value],
    small: fn(i64, i64) -> Option<i64>,
    inexact: fn(f64, f64) -> f64,
) -> Option<Value> {
    let [a, b] = arguments else {
        return None;
    };
    match (*a, *b) {
        (Value::Int(a), Value::Int(b)) => small(a, b).map(Value::Int),
        (Value::Float(a), Value::Float(b)) => Some(Value::Float(inexact(a, b))),
        (Value::Int(a), Value::Float(b)) => Some(Value::Float(inexact(to_float(a), b))),
        (Value::Float(a), Value::Int(b)) => Some(Value::Float(inexact(a, to_float(b)))),
        _ => None,
    }
}

pub(crate) fn quick_holds(a: Value, b: Value, holds: fn(Ordering) -> bool) -> Option<bool> {
    let order = match (a, b) {
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(&b)),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(&b),
        (Value::Int(a), Value::Float(b)) => to_float(a).partial_cmp(&b),
        (Value::Float(a), Value::Int(b)) => a.partial_cmp(&to_float(b)),
        _ => return None,
    };
    Some(order.is_some_and(holds))
}
pub(crate) fn chain(
    heap: &Heap,
    name: &str,
    arguments: &[Value],
    holds: fn(Ordering) -> bool,
) -> Result<Value, Error> {
    if let [a, b] = arguments
        && let Some(held) = quick_holds(*a, *b, holds)
    {
        return Ok(Value::Bool(held));
    }
    let mut holding = true;
    let mut previous: Option<Number> = None;
    for value in arguments {
        let number = Number::of(heap, name, *value)?;
        if let Some(before) = &previous {
            holding = holding && before.compare(&number).is_some_and(holds);
        }
        previous = Some(number);
    }
    Ok(Value::Bool(holding))
}

fn step(heap: &mut Heap, name: &str, value: Value, by: i64) -> Result<Value, Error> {
    if let Some(stepped) = quick(&[value, Value::Int(by)], i64::checked_add, |a, b| a + b) {
        return Ok(stepped);
    }
    one(heap, name, value, |number| Ok(number.add(&Number::Int(by))))
}

pub(crate) fn extreme(
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

pub(crate) fn sum(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    if let Some(sum) = quick(arguments, i64::checked_add, |a, b| a + b) {
        return Ok(sum);
    }
    fold(heap, "+", Number::Int(0), arguments, |a, b| Ok(a.add(&b)))
}

fn one(
    heap: &mut Heap,
    name: &str,
    value: Value,
    change: impl Fn(Number) -> Result<Number, Error>,
) -> Result<Value, Error> {
    change(Number::of(heap, name, value)?)?.store(heap)
}

fn sign(
    heap: &Heap,
    name: &str,
    value: Value,
    holds: fn(Ordering) -> bool,
) -> Result<Value, Error> {
    let number = Number::of(heap, name, value)?;
    Ok(Value::Bool(number.sign().is_some_and(holds)))
}

fn parity(heap: &Heap, name: &str, value: Value, even: bool) -> Result<Value, Error> {
    Number::of(heap, name, value)?
        .even()
        .map(|found| Value::Bool(found == even))
        .ok_or_else(|| {
            Error::new(format!(
                "{name} expects a whole number, got {}",
                crate::value::print::brief(heap, value)
            ))
            .of(ErrorKind::Type)
        })
}

fn install_rounding(isolate: &mut Isolate) {
    for (name, how) in [
        ("round", f64::round as fn(f64) -> f64),
        ("floor", f64::floor),
        ("ceil", f64::ceil),
        ("int", f64::trunc),
    ] {
        isolate
            .native("ox", name)
            .arity(1, Some(1))
            .function(move |heap, arguments| {
                one(heap, name, arguments[0], |number| number.whole(name, how))
            });
    }
    isolate
        .native("ox", "float")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            one(heap, "float", arguments[0], |number| {
                Ok(Number::Float(number.float()))
            })
        });
    isolate
        .native("ox", "abs")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            one(heap, "abs", arguments[0], |number| {
                Ok(if number.sign().is_some_and(Ordering::is_lt) {
                    number.negate()
                } else {
                    number
                })
            })
        });
    isolate
        .native("ox", "mod")
        .arity(2, Some(2))
        .function(|heap, arguments| reduce(heap, "mod", arguments, Ok, |a, b| a.modulo(&b)));
    isolate
        .native("ox", "rem")
        .arity(2, Some(2))
        .function(|heap, arguments| reduce(heap, "rem", arguments, Ok, |a, b| a.remainder(&b)));
    isolate
        .native("ox", "zero?")
        .arity(1, Some(1))
        .function(|heap, arguments| sign(heap, "zero?", arguments[0], Ordering::is_eq));
    isolate
        .native("ox", "pos?")
        .arity(1, Some(1))
        .function(|heap, arguments| sign(heap, "pos?", arguments[0], Ordering::is_gt));
    isolate
        .native("ox", "neg?")
        .arity(1, Some(1))
        .function(|heap, arguments| sign(heap, "neg?", arguments[0], Ordering::is_lt));
    isolate
        .native("ox", "even?")
        .arity(1, Some(1))
        .function(|heap, arguments| parity(heap, "even?", arguments[0], true));
    isolate
        .native("ox", "odd?")
        .arity(1, Some(1))
        .function(|heap, arguments| parity(heap, "odd?", arguments[0], false));
}

pub fn install(isolate: &mut Isolate) {
    install_rounding(isolate);
    isolate
        .native("ox", "==")
        .arity(1, None)
        .function(|heap, arguments| chain(heap, "==", arguments, Ordering::is_eq));
    isolate.native("ox", "+").arity(0, None).function(sum);
    isolate
        .native("ox", "*")
        .arity(0, None)
        .function(|heap, arguments| {
            if let Some(product) = quick(arguments, i64::checked_mul, |a, b| a * b) {
                return Ok(product);
            }
            fold(heap, "*", Number::Int(1), arguments, |a, b| {
                Ok(a.multiply(&b))
            })
        });
    isolate
        .native("ox", "inc")
        .arity(1, Some(1))
        .function(|heap, arguments| step(heap, "inc", arguments[0], 1));
    isolate
        .native("ox", "dec")
        .arity(1, Some(1))
        .function(|heap, arguments| step(heap, "dec", arguments[0], -1));
    isolate
        .native("ox", "-")
        .arity(1, None)
        .function(|heap, arguments| {
            if let Some(difference) = quick(arguments, i64::checked_sub, |a, b| a - b) {
                return Ok(difference);
            }
            reduce(
                heap,
                "-",
                arguments,
                |only| Ok(only.negate()),
                |a, b| Ok(a.subtract(&b)),
            )
        });
    isolate
        .native("ox", "/")
        .arity(1, None)
        .function(|heap, arguments| {
            reduce(
                heap,
                "/",
                arguments,
                |only| Number::Int(1).divide(&only),
                |a, b| a.divide(&b),
            )
        });
    isolate
        .native("ox", "//")
        .arity(2, Some(2))
        .function(|heap, arguments| reduce(heap, "//", arguments, Ok, |a, b| a.floor_divide(&b)));
    isolate
        .native("ox", "min")
        .arity(1, None)
        .function(|heap, arguments| extreme(heap, "min", arguments, Ordering::is_lt));
    isolate
        .native("ox", "max")
        .arity(1, None)
        .function(|heap, arguments| extreme(heap, "max", arguments, Ordering::is_gt));
    isolate
        .native("ox", "<")
        .arity(1, None)
        .function(|heap, arguments| chain(heap, "<", arguments, Ordering::is_lt));
    isolate
        .native("ox", "<=")
        .arity(1, None)
        .function(|heap, arguments| chain(heap, "<=", arguments, Ordering::is_le));
    isolate
        .native("ox", ">")
        .arity(1, None)
        .function(|heap, arguments| chain(heap, ">", arguments, Ordering::is_gt));
    isolate
        .native("ox", ">=")
        .arity(1, None)
        .function(|heap, arguments| chain(heap, ">=", arguments, Ordering::is_ge));
}
