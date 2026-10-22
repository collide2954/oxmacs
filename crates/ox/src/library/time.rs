use jiff::civil::{Date, DateTime, Weekday};

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::number::to_float;
use crate::value::print;
use crate::vm::build::make_map;

const MODULE: &str = "ox/time";

fn weekday(day: Weekday) -> &'static str {
    match day {
        Weekday::Monday => "monday",
        Weekday::Tuesday => "tuesday",
        Weekday::Wednesday => "wednesday",
        Weekday::Thursday => "thursday",
        Weekday::Friday => "friday",
        Weekday::Saturday => "saturday",
        Weekday::Sunday => "sunday",
    }
}

fn parse(heap: &mut Heap, value: Value) -> Result<Value, Error> {
    let text = heap.text(value).unwrap_or_default();
    let time = text
        .parse::<DateTime>()
        .or_else(|_| text.parse::<Date>().map(DateTime::from))
        .map_err(|_| {
            Error::new(format!(
                "time/parse can't read {} as a date or a time",
                print::write(heap, value)
            ))
        })?;
    fields(heap, time)
}

fn local(heap: &mut Heap, value: Value) -> Result<Value, Error> {
    let Value::Int(millis) = value else {
        return Err(Error::new(format!(
            "time/local expects milliseconds, got {}",
            print::write(heap, value)
        ))
        .of(ErrorKind::Type));
    };
    let moment = jiff::Timestamp::from_millisecond(millis).map_err(|error| {
        Error::new(format!("time/local can't use {millis}: {error}")).of(ErrorKind::Range)
    })?;
    fields(
        heap,
        moment.to_zoned(jiff::tz::TimeZone::system()).datetime(),
    )
}

fn fields(heap: &mut Heap, time: DateTime) -> Result<Value, Error> {
    let fields = [
        ("year", Value::Int(i64::from(time.year()))),
        ("month", Value::Int(i64::from(time.month()))),
        ("day", Value::Int(i64::from(time.day()))),
        ("hour", Value::Int(i64::from(time.hour()))),
        ("minute", Value::Int(i64::from(time.minute()))),
        ("second", Value::Int(i64::from(time.second()))),
        ("weekday", Value::keyword(weekday(time.weekday()))),
    ];
    let items: Vec<Value> = fields
        .into_iter()
        .flat_map(|(name, field)| [Value::keyword(name), field])
        .collect();
    make_map(heap, &items)
}

fn scale(heap: &Heap, name: &str, value: Value, factor: i64) -> Result<Value, Error> {
    match value {
        Value::Int(count) => count
            .checked_mul(factor)
            .map(Value::Int)
            .ok_or_else(|| Error::new(format!("{name} is too long")).of(ErrorKind::Range)),
        Value::Float(count) => Ok(Value::Float(count * to_float(factor))),
        _ => Err(Error::new(format!(
            "{name} expects a number, got {}",
            print::write(heap, value)
        ))
        .of(ErrorKind::Type)),
    }
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native_in(MODULE, "now", 0, Some(0), |_, _| {
        Ok(Value::Int(jiff::Timestamp::now().as_millisecond()))
    });
    isolate.define_native_in(MODULE, "parse", 1, Some(1), |heap, arguments| {
        parse(heap, arguments[0])
    });
    isolate.define_native_in(MODULE, "local", 1, Some(1), |heap, arguments| {
        local(heap, arguments[0])
    });
    for (name, factor) in [
        ("millis", 1),
        ("seconds", 1_000),
        ("minutes", 60_000),
        ("hours", 3_600_000),
        ("days", 86_400_000),
    ] {
        let shown = format!("time/{name}");
        isolate.define_native_in(MODULE, name, 1, Some(1), move |heap, arguments| {
            scale(heap, &shown, arguments[0], factor)
        });
    }
}
