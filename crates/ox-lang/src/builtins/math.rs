use std::cmp::Ordering;
use std::f64::consts;

use num_bigint::BigInt;

use crate::builtins::collections;
use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::number::{Number, shrink, to_float};
use crate::value::print;

const MODULE: &str = "ox/math";

fn random_bits() -> Result<u64, Error> {
    getrandom::u64().map_err(|_| Error::new("The system has no random numbers to give"))
}

fn below(bound: u64) -> Result<u64, Error> {
    let limit = u64::MAX - u64::MAX % bound;
    loop {
        let drawn = random_bits()?;
        if drawn < limit {
            return Ok(drawn % bound);
        }
    }
}

fn float(heap: &Heap, name: &str, value: Value) -> Result<f64, Error> {
    Ok(Number::of(heap, name, value)?.float())
}

fn above_zero(heap: &Heap, name: &str, value: Value) -> Result<f64, Error> {
    let number = float(heap, name, value)?;
    if number > 0.0 {
        return Ok(number);
    }
    Err(Error::new(format!(
        "{name} needs a number above 0, got {}",
        print::brief(heap, value)
    ))
    .of(ErrorKind::Range))
}

fn root(heap: &Heap, value: Value) -> Result<Value, Error> {
    let number = float(heap, "math/sqrt", value)?;
    if number < 0.0 {
        return Err(Error::new(format!(
            "math/sqrt can't take the square root of {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Range));
    }
    Ok(Value::Float(number.sqrt()))
}

fn power(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let base = Number::of(heap, "math/pow", arguments[0])?;
    let exponent = Number::of(heap, "math/pow", arguments[1])?;
    let exact = match (&base, &exponent) {
        (Number::Int(base), Number::Int(power)) => {
            Some((BigInt::from(*base), u32::try_from(*power).ok()))
        }
        (Number::Big(base), Number::Int(power)) => Some((base.clone(), u32::try_from(*power).ok())),
        _ => None,
    };
    if let Some((base, Some(power))) = exact {
        return shrink(base.pow(power)).store(heap);
    }
    let raised = base.float().powf(exponent.float());
    if raised.is_nan() {
        return Err(Error::new(format!(
            "math/pow can't raise {} to {}",
            print::brief(heap, arguments[0]),
            print::brief(heap, arguments[1])
        ))
        .of(ErrorKind::Range));
    }
    Ok(Value::Float(raised))
}

fn clamp(heap: &Heap, arguments: &[Value]) -> Result<Value, Error> {
    let [value, low, high] = [arguments[0], arguments[1], arguments[2]]
        .map(|argument| Number::of(heap, "math/clamp", argument));
    let (value, low, high) = (value?, low?, high?);
    if low.compare(&high) == Some(Ordering::Greater) {
        return Err(Error::new(format!(
            "math/clamp needs low at most high, got {} and {}",
            print::brief(heap, arguments[1]),
            print::brief(heap, arguments[2])
        ))
        .of(ErrorKind::Range));
    }
    Ok(match (value.compare(&low), value.compare(&high)) {
        (Some(Ordering::Less), _) => arguments[1],
        (_, Some(Ordering::Greater)) => arguments[2],
        _ => arguments[0],
    })
}

fn whole(heap: &Heap, value: Value) -> Result<i64, Error> {
    let Value::Int(number) = value else {
        return Err(Error::new(format!(
            "math/random-int expects whole numbers, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type));
    };
    Ok(number)
}

fn random_int(heap: &Heap, arguments: &[Value]) -> Result<Value, Error> {
    let (low, high) = match arguments {
        [bound] => {
            let bound = whole(heap, *bound)?;
            if bound <= 0 {
                return Err(Error::new(format!(
                    "math/random-int needs a bound above 0, got {bound}"
                ))
                .of(ErrorKind::Range));
            }
            (0, bound)
        }
        _ => (whole(heap, arguments[0])?, whole(heap, arguments[1])?),
    };
    if high <= low {
        return Err(Error::new(format!(
            "math/random-int needs high above low, got {low} and {high}"
        ))
        .of(ErrorKind::Range));
    }
    let span = u64::try_from(i128::from(high) - i128::from(low)).unwrap_or(u64::MAX);
    let drawn = i128::from(low) + i128::from(below(span)?);
    Ok(Value::Int(i64::try_from(drawn).unwrap_or(low)))
}

fn random_pick(heap: &mut Heap, value: Value) -> Result<Value, Error> {
    let items = collections::sequence(heap, "math/random-pick", value)?;
    if items.is_empty() {
        return Ok(Value::Nil);
    }
    let count = u64::try_from(items.len()).unwrap_or(u64::MAX);
    let index = usize::try_from(below(count)?).unwrap_or(0);
    Ok(items[index])
}

fn fraction() -> Result<f64, Error> {
    let bits = i64::try_from(random_bits()? >> 11).unwrap_or(0);
    Ok(to_float(bits) / to_float(1 << 53))
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_value_in(
        MODULE,
        "pi",
        Value::Float(consts::PI),
        "The ratio of a circle's circumference to its diameter, 3.141592653589793.",
    );
    isolate.define_value_in(
        MODULE,
        "e",
        Value::Float(consts::E),
        "The base of natural logarithms, 2.718281828459045.",
    );
    isolate
        .native(MODULE, "sqrt")
        .arity(1, Some(1))
        .function(|heap, arguments| root(heap, arguments[0]));
    isolate
        .native(MODULE, "pow")
        .arity(2, Some(2))
        .function(power);
    for (name, apply) in [
        ("exp", f64::exp as fn(f64) -> f64),
        ("sin", f64::sin),
        ("cos", f64::cos),
        ("tan", f64::tan),
    ] {
        let shown = format!("math/{name}");
        isolate
            .native(MODULE, name)
            .arity(1, Some(1))
            .function(move |heap, arguments| {
                Ok(Value::Float(apply(float(heap, &shown, arguments[0])?)))
            });
    }
    for (name, apply) in [("log", f64::ln as fn(f64) -> f64), ("log10", f64::log10)] {
        let shown = format!("math/{name}");
        isolate
            .native(MODULE, name)
            .arity(1, Some(1))
            .function(move |heap, arguments| {
                Ok(Value::Float(apply(above_zero(heap, &shown, arguments[0])?)))
            });
    }
    isolate
        .native(MODULE, "atan2")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let y = float(heap, "math/atan2", arguments[0])?;
            let x = float(heap, "math/atan2", arguments[1])?;
            Ok(Value::Float(y.atan2(x)))
        });
    isolate
        .native(MODULE, "clamp")
        .arity(3, Some(3))
        .function(|heap, arguments| clamp(heap, arguments));
    isolate
        .native(MODULE, "lerp")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let a = float(heap, "math/lerp", arguments[0])?;
            let b = float(heap, "math/lerp", arguments[1])?;
            let t = float(heap, "math/lerp", arguments[2])?;
            Ok(Value::Float((b - a).mul_add(t, a)))
        });
    isolate
        .native(MODULE, "random")
        .arity(0, Some(0))
        .function(|_, _| Ok(Value::Float(fraction()?)));
    isolate
        .native(MODULE, "random-int")
        .arity(1, Some(2))
        .function(|heap, arguments| random_int(heap, arguments));
    isolate
        .native(MODULE, "random-pick")
        .arity(1, Some(1))
        .function(|heap, arguments| random_pick(heap, arguments[0]));
}
