use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::print;
use crate::vm::build::make_vector;

const MODULE: &str = "ox/string";

fn text<'a>(heap: &'a Heap, name: &str, value: Value) -> Result<&'a str, Error> {
    heap.text(value).ok_or_else(|| {
        Error::new(format!(
            "string/{name} expects a string, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)
    })
}

fn count(heap: &Heap, name: &str, value: Value) -> Result<usize, Error> {
    match value {
        Value::Int(number) => usize::try_from(number).map_err(|_| {
            Error::new(format!(
                "string/{name} expects a count of 0 or more, got {number}"
            ))
            .of(ErrorKind::Range)
        }),
        _ => Err(Error::new(format!(
            "string/{name} expects a whole number, got {}",
            print::brief(heap, value)
        ))
        .of(ErrorKind::Type)),
    }
}

fn strings(heap: &mut Heap, parts: &[String]) -> Result<Value, Error> {
    let values = parts
        .iter()
        .map(|part| heap.string(part).map_err(Error::from))
        .collect::<Result<Vec<_>, _>>()?;
    make_vector(heap, &values)
}

fn unary(isolate: &mut Isolate, name: &'static str, change: fn(&str) -> String) {
    isolate
        .native(MODULE, name)
        .arity(1, Some(1))
        .function(move |heap, arguments| {
            let changed = change(text(heap, name, arguments[0])?);
            heap.string(&changed).map_err(Error::from)
        });
}

fn test(isolate: &mut Isolate, name: &'static str, holds: fn(&str, &str) -> bool) {
    isolate
        .native(MODULE, name)
        .arity(2, Some(2))
        .function(move |heap, arguments| {
            let whole = text(heap, name, arguments[0])?;
            let part = text(heap, name, arguments[1])?;
            Ok(Value::Bool(holds(whole, part)))
        });
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

fn pad(heap: &mut Heap, name: &str, arguments: &[Value], start: bool) -> Result<Value, Error> {
    let original = text(heap, name, arguments[0])?.to_owned();
    let width = count(heap, name, arguments[1])?;
    let fill = match arguments.get(2) {
        Some(fill) => text(heap, name, *fill)?.to_owned(),
        None => " ".to_owned(),
    };
    let missing = width.saturating_sub(original.chars().count());
    let padding: String = fill.chars().cycle().take(missing).collect();
    let padded = if start {
        padding + &original
    } else {
        original + &padding
    };
    heap.string(&padded).map_err(Error::from)
}

fn number(text: &str) -> Value {
    let text = text.trim();
    if let Ok(whole) = text.parse::<i64>() {
        return Value::Int(whole);
    }
    let digits = text.chars().any(|character| character.is_ascii_digit());
    text.parse::<f64>()
        .ok()
        .filter(|number| digits && number.is_finite())
        .map_or(Value::Nil, Value::Float)
}

fn slice(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let whole = text(heap, "slice", arguments[0])?.to_owned();
    let length = whole.chars().count();
    let start = count(heap, "slice", arguments[1])?;
    let end = match arguments.get(2) {
        Some(end) => count(heap, "slice", *end)?,
        None => length,
    };
    if start > end || end > length {
        return Err(Error::new(format!(
            "string/slice goes past the end of a {length}-character string"
        ))
        .of(ErrorKind::Range));
    }
    let part: String = whole.chars().skip(start).take(end - start).collect();
    heap.string(&part).map_err(Error::from)
}

fn install_shaping(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "words")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let parts: Vec<String> = text(heap, "words", arguments[0])?
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            strings(heap, &parts)
        });
    unary(isolate, "reverse", |text| text.chars().rev().collect());
    isolate
        .native(MODULE, "truncate")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let whole = text(heap, "truncate", arguments[0])?.to_owned();
            let width = count(heap, "truncate", arguments[1])?;
            let cut = if whole.chars().count() <= width {
                whole
            } else {
                let kept: String = whole.chars().take(width.saturating_sub(1)).collect();
                format!("{kept}…")
            };
            heap.string(&cut).map_err(Error::from)
        });
}

pub fn install(isolate: &mut Isolate) {
    unary(isolate, "trim", |text| text.trim().to_owned());
    unary(isolate, "trim-start", |text| text.trim_start().to_owned());
    unary(isolate, "trim-end", |text| text.trim_end().to_owned());
    unary(isolate, "upper", str::to_uppercase);
    unary(isolate, "lower", str::to_lowercase);
    unary(isolate, "capitalize", capitalize);
    test(isolate, "starts-with?", |whole, part| {
        whole.starts_with(part)
    });
    test(isolate, "ends-with?", |whole, part| whole.ends_with(part));
    test(isolate, "includes?", |whole, part| whole.contains(part));
    isolate
        .native(MODULE, "blank?")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Ok(Value::Bool(
                text(heap, "blank?", arguments[0])?.trim().is_empty(),
            ))
        });
    isolate
        .native(MODULE, "length")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let length = text(heap, "length", arguments[0])?.chars().count();
            Ok(Value::Int(i64::try_from(length).unwrap_or(i64::MAX)))
        });
    isolate
        .native(MODULE, "split")
        .arity(2, Some(3))
        .function(|heap, arguments| {
            let whole = text(heap, "split", arguments[0])?;
            let separator = text(heap, "split", arguments[1])?;
            let parts: Vec<String> = match arguments.get(2) {
                Some(limit) => whole
                    .splitn(count(heap, "split", *limit)?, separator)
                    .map(str::to_owned)
                    .collect(),
                None => whole.split(separator).map(str::to_owned).collect(),
            };
            strings(heap, &parts)
        });
    install_shaping(isolate);
    install_pieces(isolate);
}

fn install_pieces(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "lines")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let parts: Vec<String> = text(heap, "lines", arguments[0])?
                .lines()
                .map(str::to_owned)
                .collect();
            strings(heap, &parts)
        });
    isolate
        .native(MODULE, "chars")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let chars: Vec<Value> = text(heap, "chars", arguments[0])?
                .chars()
                .map(Value::Char)
                .collect();
            make_vector(heap, &chars)
        });
    isolate
        .native(MODULE, "join")
        .arity(1, Some(2))
        .function(|heap, arguments| {
            let (separator, items) = match arguments {
                [items] => (String::new(), *items),
                [separator, items, ..] => (text(heap, "join", *separator)?.to_owned(), *items),
                [] => return Ok(Value::Nil),
            };
            let parts: Vec<String> = heap
                .elements(items)
                .ok_or_else(|| {
                    Error::new(format!(
                        "string/join expects a sequence, got {}",
                        print::brief(heap, items)
                    ))
                    .of(ErrorKind::Type)
                })?
                .into_iter()
                .map(|item| print::display(heap, item))
                .collect();
            heap.string(&parts.join(&separator)).map_err(Error::from)
        });
    isolate
        .native(MODULE, "replace")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let whole = text(heap, "replace", arguments[0])?;
            let from = text(heap, "replace", arguments[1])?;
            let to = text(heap, "replace", arguments[2])?;
            let replaced = whole.replace(from, to);
            heap.string(&replaced).map_err(Error::from)
        });
    isolate
        .native(MODULE, "index-of")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let whole = text(heap, "index-of", arguments[0])?;
            let part = text(heap, "index-of", arguments[1])?;
            Ok(whole.find(part).map_or(Value::Nil, |at| {
                Value::Int(i64::try_from(whole[..at].chars().count()).unwrap_or(i64::MAX))
            }))
        });
    isolate
        .native(MODULE, "repeat")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let part = text(heap, "repeat", arguments[0])?;
            let times = count(heap, "repeat", arguments[1])?;
            let repeated = part.repeat(times);
            heap.string(&repeated).map_err(Error::from)
        });
    isolate
        .native(MODULE, "slice")
        .arity(2, Some(3))
        .function(slice);
    isolate
        .native(MODULE, "to-number")
        .arity(1, Some(1))
        .function(|heap, arguments| Ok(number(text(heap, "to-number", arguments[0])?)));
    isolate
        .native(MODULE, "pad-start")
        .arity(2, Some(3))
        .function(|heap, arguments| pad(heap, "pad-start", arguments, true));
    isolate
        .native(MODULE, "pad-end")
        .arity(2, Some(3))
        .function(|heap, arguments| pad(heap, "pad-end", arguments, false));
}
