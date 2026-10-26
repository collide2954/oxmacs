use jiff::civil::{Date, DateTime, Weekday};
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::number::to_float;
use crate::value::print;
use crate::vm::build::{full, make_map};

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
                print::brief(heap, value)
            ))
        })?;
    fields(heap, time)
}

fn zone_named(name: &str) -> Result<TimeZone, Error> {
    if name.eq_ignore_ascii_case("utc") {
        return Ok(TimeZone::UTC);
    }
    TimeZone::get(name).map_err(|_| Error::new(format!("There is no time zone called {name}")))
}

fn zone_option(heap: &Heap, name: &str, rest: &[Value]) -> Result<TimeZone, Error> {
    match rest {
        [] | [_, Value::Nil] => Ok(TimeZone::system()),
        [Value::Keyword(option), value] if option.name() == "zone" => {
            zone_named(&print::display(heap, *value))
        }
        _ => Err(Error::new(format!("{name} takes :zone"))),
    }
}

fn moment(heap: &Heap, name: &str, value: Value) -> Result<Timestamp, Error> {
    let Value::Int(millis) = value else {
        return Err(Error::new(format!(
            "{name} expects milliseconds, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type));
    };
    Timestamp::from_millisecond(millis).map_err(|error| {
        Error::new(format!("{name} can't use {millis}: {error}")).of(ErrorKind::Range)
    })
}

fn local(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let zone = match arguments.get(1) {
        Some(zone) => zone_named(&print::display(heap, *zone))?,
        None => TimeZone::system(),
    };
    let moment = moment(heap, "time/local", arguments[0])?;
    fields(heap, moment.to_zoned(zone).datetime())
}

fn field(heap: &Heap, fields: &[(Value, Value)], name: &str, default: i64) -> Result<i64, Error> {
    match fields
        .iter()
        .find(|(key, _)| matches!(key, Value::Keyword(key) if key.name() == name))
    {
        None => Ok(default),
        Some((_, Value::Int(number))) => Ok(*number),
        Some((_, value)) => Err(Error::new(format!(
            "time/instant expects a whole number for :{name}, got {}",
            print::brief(heap, *value)
        ))
        .of(ErrorKind::Type)),
    }
}

fn civil(heap: &Heap, fields: &[(Value, Value)]) -> Result<DateTime, Error> {
    let part = |name: &str, default: i64| -> Result<i8, Error> {
        i8::try_from(field(heap, fields, name, default)?).map_err(|_| {
            Error::new(format!("time/instant can't use that :{name}")).of(ErrorKind::Range)
        })
    };
    let year = i16::try_from(field(heap, fields, "year", 1970)?)
        .map_err(|_| Error::new("time/instant can't use that :year").of(ErrorKind::Range))?;
    DateTime::new(
        year,
        part("month", 1)?,
        part("day", 1)?,
        part("hour", 0)?,
        part("minute", 0)?,
        part("second", 0)?,
        0,
    )
    .map_err(|error| {
        Error::new(format!("time/instant can't make that date: {error}")).of(ErrorKind::Range)
    })
}

fn instant(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let zone = zone_option(heap, "time/instant", &arguments[1..])?;
    let given = arguments[0];
    let unreadable = || {
        Error::new(format!(
            "time/instant can't read {} as a moment",
            print::brief(heap, given)
        ))
    };
    let civil = if let Some(fields) = heap.entries(given) {
        civil(heap, &fields)?
    } else {
        let text = heap.text(given).ok_or_else(unreadable)?;
        if let Ok(stamp) = text.parse::<Timestamp>() {
            return Ok(Value::Int(stamp.as_millisecond()));
        }
        text.parse::<DateTime>()
            .or_else(|_| text.parse::<Date>().map(DateTime::from))
            .map_err(|_| unreadable())?
    };
    let zoned = civil.to_zoned(zone).map_err(|error| {
        Error::new(format!("time/instant can't place that time: {error}")).of(ErrorKind::Range)
    })?;
    Ok(Value::Int(zoned.timestamp().as_millisecond()))
}

fn add(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let start = moment(heap, "time/add", arguments[0])?;
    let zone = zone_option(heap, "time/add", &arguments[2..])?;
    let fields = heap.entries(arguments[1]).ok_or_else(|| {
        Error::new(format!(
            "time/add expects a map of amounts, got {}",
            print::brief(heap, arguments[1])
        ))
        .of(ErrorKind::Type)
    })?;
    let mut span = Span::new();
    for (key, value) in fields {
        let unit = match key {
            Value::Keyword(unit) => unit.name().to_owned(),
            other => print::display(heap, other),
        };
        let Value::Int(amount) = value else {
            return Err(Error::new(format!(
                "time/add expects whole numbers, got {}",
                print::brief(heap, value)
            ))
            .of(ErrorKind::Type));
        };
        span = match unit.as_str() {
            "years" => span.try_years(amount),
            "months" => span.try_months(amount),
            "weeks" => span.try_weeks(amount),
            "days" => span.try_days(amount),
            "hours" => span.try_hours(amount),
            "minutes" => span.try_minutes(amount),
            "seconds" => span.try_seconds(amount),
            _ => {
                return Err(Error::new(format!(
                    "time/add takes years, months, weeks, days, hours, minutes and seconds, not :{unit}"
                )));
            }
        }
        .map_err(|error| Error::new(format!("time/add: {error}")).of(ErrorKind::Range))?;
    }
    let moved = start
        .to_zoned(zone)
        .checked_add(span)
        .map_err(|error| Error::new(format!("time/add: {error}")).of(ErrorKind::Range))?;
    Ok(Value::Int(moved.timestamp().as_millisecond()))
}

fn format(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let start = moment(heap, "time/format", arguments[0])?;
    let pattern = print::display(heap, arguments[1]);
    let zone = zone_option(heap, "time/format", &arguments[2..])?;
    let text = jiff::fmt::strtime::format(&pattern, &start.to_zoned(zone))
        .map_err(|error| Error::new(format!("time/format can't use {pattern:?}: {error}")))?;
    heap.string(&text).map_err(full)
}

fn days_in_month(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let (Value::Int(year), Value::Int(month)) = (arguments[0], arguments[1]) else {
        return Err(Error::new(format!(
            "time/days-in-month expects a year and a month, got {} and {}",
            print::brief(heap, arguments[0]),
            print::brief(heap, arguments[1])
        ))
        .of(ErrorKind::Type));
    };
    let date = i16::try_from(year)
        .ok()
        .zip(i8::try_from(month).ok())
        .and_then(|(year, month)| Date::new(year, month, 1).ok())
        .ok_or_else(|| {
            Error::new(format!("time/days-in-month can't use {year}-{month}")).of(ErrorKind::Range)
        })?;
    Ok(Value::Int(i64::from(date.days_in_month())))
}

const WEEKDAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];
const FARTHEST_YEAR: i64 = 1_000_000_000;

fn leap(year: i64) -> bool {
    year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0)
}

fn month_days(year: i64, month: i64) -> i64 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn civil_days(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let shifted = if month > 2 { month - 3 } else { month + 9 };
    let of_year = (153 * shifted + 2) / 5 + day - 1;
    era * 146_097 + of_era * 365 + of_era / 4 - of_era / 100 + of_year - 719_468
}

fn civil_date(days: i64) -> Option<(i64, i64, i64)> {
    let shifted_days = days.checked_add(719_468)?;
    let era = shifted_days.div_euclid(146_097);
    let of_era = shifted_days - era * 146_097;
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * of_year + 2) / 153;
    let day = of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    };
    let year = year_of_era.checked_add(era.checked_mul(400)?)? + i64::from(month <= 2);
    Some((year, month, day))
}

fn weekday_number(days: i64) -> i64 {
    (days.rem_euclid(7) + 3) % 7
}

fn day_number(heap: &Heap, value: Value) -> Result<Value, Error> {
    let fields = heap.entries(value).unwrap_or_default();
    let part = |name: &str| {
        fields.iter().find_map(|(key, field)| match (key, field) {
            (Value::Keyword(key), Value::Int(number)) if key.name() == name => Some(*number),
            _ => None,
        })
    };
    let (Some(year), Some(month), Some(day)) = (part("year"), part("month"), part("day")) else {
        return Err(Error::new(format!(
            "time/day-number expects a map with :year, :month and :day, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type));
    };
    let real = year.abs() <= FARTHEST_YEAR
        && (1..=12).contains(&month)
        && (1..=month_days(year, month)).contains(&day);
    if !real {
        return Err(
            Error::new(format!("time/day-number can't use {year}-{month}-{day}"))
                .of(ErrorKind::Range),
        );
    }
    Ok(Value::Int(civil_days(year, month, day)))
}

fn days_of(heap: &Heap, name: &str, value: Value) -> Result<i64, Error> {
    let Value::Int(days) = value else {
        return Err(Error::new(format!(
            "{name} expects a whole number of days, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type));
    };
    Ok(days)
}

fn date_of(heap: &mut Heap, value: Value) -> Result<Value, Error> {
    let days = days_of(heap, "time/date-of", value)?;
    let (year, month, day) = civil_date(days)
        .ok_or_else(|| Error::new(format!("time/date-of can't use {days}")).of(ErrorKind::Range))?;
    let weekday = WEEKDAYS[usize::try_from(weekday_number(days)).unwrap_or(0)];
    make_map(
        heap,
        &[
            Value::keyword("year"),
            Value::Int(year),
            Value::keyword("month"),
            Value::Int(month),
            Value::keyword("day"),
            Value::Int(day),
            Value::keyword("weekday"),
            Value::keyword(weekday),
        ],
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
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)),
    }
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "now")
        .arity(0, Some(0))
        .function(|_, _| Ok(Value::Int(Timestamp::now().as_millisecond())));
    isolate
        .native(MODULE, "parse")
        .arity(1, Some(1))
        .function(|heap, arguments| parse(heap, arguments[0]));
    isolate
        .native(MODULE, "local")
        .arity(1, Some(2))
        .function(local);
    isolate
        .native(MODULE, "instant")
        .arity(1, Some(3))
        .function(instant);
    isolate
        .native(MODULE, "add")
        .arity(2, Some(4))
        .function(add);
    isolate
        .native(MODULE, "format")
        .arity(2, Some(4))
        .function(format);
    isolate
        .native(MODULE, "days-in-month")
        .arity(2, Some(2))
        .function(days_in_month);
    isolate
        .native(MODULE, "day-number")
        .arity(1, Some(1))
        .function(|heap, arguments| day_number(heap, arguments[0]));
    isolate
        .native(MODULE, "date-of")
        .arity(1, Some(1))
        .function(|heap, arguments| date_of(heap, arguments[0]));
    isolate
        .native(MODULE, "weekday-number")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Ok(Value::Int(weekday_number(days_of(
                heap,
                "time/weekday-number",
                arguments[0],
            )?)))
        });
    isolate
        .native(MODULE, "zone")
        .arity(0, Some(0))
        .function(|heap, _| {
            let zone = TimeZone::system();
            heap.string(zone.iana_name().unwrap_or("UTC")).map_err(full)
        });
    for (name, factor) in [
        ("millis", 1),
        ("seconds", 1_000),
        ("minutes", 60_000),
        ("hours", 3_600_000),
        ("days", 86_400_000),
    ] {
        let shown = format!("time/{name}");
        isolate
            .native(MODULE, name)
            .arity(1, Some(1))
            .function(move |heap, arguments| scale(heap, &shown, arguments[0], factor));
    }
}
