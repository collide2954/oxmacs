use std::sync::Arc;

use regex::Regex;
use unicode_width::UnicodeWidthChar;

use crate::builtins::regex::regex_of;
use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::native::{count, expected, text};
use crate::value::Value;
use crate::value::print;
use crate::vm::build::make_vector;

const MODULE: &str = "ox/string";

enum Pattern {
    Text(String),
    Regex(Arc<Regex>),
}

fn pattern(heap: &Heap, name: &str, value: Value) -> Result<Pattern, Error> {
    if heap.foreign_of::<Regex>(value).is_some() {
        return regex_of(heap, name, value).map(Pattern::Regex);
    }
    heap.text(value)
        .map(|part| Pattern::Text(part.to_owned()))
        .ok_or_else(|| expected(heap, name, "a string or a regex", value))
}

fn strings(heap: &mut Heap, parts: &[String]) -> Result<Value, Error> {
    let values = parts
        .iter()
        .map(|part| heap.string(part).map_err(Error::from))
        .collect::<Result<Vec<_>, _>>()?;
    make_vector(heap, &values)
}

fn unary(isolate: &mut Isolate, name: &'static str, change: fn(&str) -> String) {
    let shown = format!("string/{name}");
    isolate
        .native(MODULE, name)
        .arity(1, Some(1))
        .function(move |heap, arguments| {
            let changed = change(text(heap, &shown, arguments[0])?);
            heap.string(&changed).map_err(Error::from)
        });
}

fn test(isolate: &mut Isolate, name: &'static str, holds: fn(&str, &str) -> bool) {
    let shown = format!("string/{name}");
    isolate
        .native(MODULE, name)
        .arity(2, Some(2))
        .function(move |heap, arguments| {
            let whole = text(heap, &shown, arguments[0])?;
            let part = text(heap, &shown, arguments[1])?;
            Ok(Value::Bool(holds(whole, part)))
        });
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

fn character_at(whole: &str, byte: usize) -> Value {
    Value::Int(i64::try_from(whole[..byte].chars().count()).unwrap_or(i64::MAX))
}

fn columns(c: char) -> usize {
    c.width().unwrap_or(0)
}

fn width(text: &str) -> usize {
    text.chars().map(columns).sum()
}

fn pad(heap: &mut Heap, name: &str, arguments: &[Value], start: bool) -> Result<Value, Error> {
    let original = text(heap, name, arguments[0])?.to_owned();
    let wanted = count(heap, name, arguments[1])?;
    let fill = match arguments.get(2) {
        Some(fill) => text(heap, name, *fill)?.to_owned(),
        None => " ".to_owned(),
    };
    let mut missing = wanted.saturating_sub(width(&original));
    let mut padding = String::new();
    for c in fill.chars().filter(|c| columns(*c) > 0).cycle() {
        if missing == 0 || columns(c) > missing {
            break;
        }
        missing -= columns(c);
        padding.push(c);
    }
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
    let whole = text(heap, "string/slice", arguments[0])?.to_owned();
    let length = whole.chars().count();
    let start = count(heap, "string/slice", arguments[1])?;
    let end = match arguments.get(2) {
        Some(end) => count(heap, "string/slice", *end)?,
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
            let parts: Vec<String> = text(heap, "string/words", arguments[0])?
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
            let whole = text(heap, "string/truncate", arguments[0])?.to_owned();
            let room = count(heap, "string/truncate", arguments[1])?;
            let cut = if width(&whole) <= room {
                whole
            } else {
                let mut used = 0;
                let kept: String = whole
                    .chars()
                    .take_while(|c| {
                        used += columns(*c);
                        used < room
                    })
                    .collect();
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
    unary(isolate, "url-encode", oxmacs_files::uri::encode);
    unary(isolate, "url-decode", oxmacs_files::uri::decode);
    test(isolate, "starts-with?", |whole, part| {
        whole.starts_with(part)
    });
    test(isolate, "ends-with?", |whole, part| whole.ends_with(part));
    isolate
        .native(MODULE, "includes?")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let whole = text(heap, "string/includes?", arguments[0])?;
            Ok(Value::Bool(
                match pattern(heap, "string/includes?", arguments[1])? {
                    Pattern::Text(part) => whole.contains(&part),
                    Pattern::Regex(regex) => regex.is_match(whole),
                },
            ))
        });
    isolate
        .native(MODULE, "blank?")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Ok(Value::Bool(
                text(heap, "string/blank?", arguments[0])?.trim().is_empty(),
            ))
        });
    isolate
        .native(MODULE, "width")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let shown = width(text(heap, "string/width", arguments[0])?);
            Ok(Value::Int(i64::try_from(shown).unwrap_or(i64::MAX)))
        });
    isolate
        .native(MODULE, "split")
        .arity(2, Some(3))
        .function(|heap, arguments| {
            let whole = text(heap, "string/split", arguments[0])?;
            let separator = pattern(heap, "string/split", arguments[1])?;
            let limit = match arguments.get(2) {
                Some(limit) => Some(count(heap, "string/split", *limit)?),
                None => None,
            };
            let parts: Vec<String> = match (separator, limit) {
                (Pattern::Text(separator), Some(limit)) => whole
                    .splitn(limit, separator.as_str())
                    .map(str::to_owned)
                    .collect(),
                (Pattern::Text(separator), None) => {
                    whole.split(separator.as_str()).map(str::to_owned).collect()
                }
                (Pattern::Regex(regex), Some(limit)) => {
                    regex.splitn(whole, limit).map(str::to_owned).collect()
                }
                (Pattern::Regex(regex), None) => regex.split(whole).map(str::to_owned).collect(),
            };
            strings(heap, &parts)
        });
    install_shaping(isolate);
    install_pieces(isolate);
}

fn install_searching(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "index-of")
        .arity(2, Some(3))
        .function(|heap, arguments| {
            let from = match arguments.get(2) {
                Some(start) => count(heap, "string/index-of", *start)?,
                None => 0,
            };
            let whole = text(heap, "string/index-of", arguments[0])?;
            let part = text(heap, "string/index-of", arguments[1])?;
            let Some((skipped, _)) = whole
                .char_indices()
                .nth(from)
                .or_else(|| (from == whole.chars().count()).then_some((whole.len(), ' ')))
            else {
                return Ok(Value::Nil);
            };
            Ok(whole[skipped..]
                .find(part)
                .map_or(Value::Nil, |at| character_at(whole, skipped + at)))
        });
    isolate
        .native(MODULE, "last-index-of")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let whole = text(heap, "string/last-index-of", arguments[0])?;
            let part = text(heap, "string/last-index-of", arguments[1])?;
            Ok(whole
                .rfind(part)
                .map_or(Value::Nil, |at| character_at(whole, at)))
        });
}

fn install_pieces(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "lines")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let parts: Vec<String> = text(heap, "string/lines", arguments[0])?
                .lines()
                .map(str::to_owned)
                .collect();
            strings(heap, &parts)
        });
    isolate
        .native(MODULE, "chars")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let chars: Vec<Value> = text(heap, "string/chars", arguments[0])?
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
                [separator, items, ..] => {
                    (text(heap, "string/join", *separator)?.to_owned(), *items)
                }
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
                .filter(|item| *item != Value::Nil)
                .map(|item| print::display(heap, item))
                .collect();
            heap.string(&parts.join(&separator)).map_err(Error::from)
        });
    isolate
        .native(MODULE, "replace")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let whole = text(heap, "string/replace", arguments[0])?;
            let to = text(heap, "string/replace", arguments[2])?;
            let replaced = match pattern(heap, "string/replace", arguments[1])? {
                Pattern::Text(from) => whole.replace(&from, to),
                Pattern::Regex(regex) => regex.replace_all(whole, to).into_owned(),
            };
            heap.string(&replaced).map_err(Error::from)
        });
    install_searching(isolate);
    isolate
        .native(MODULE, "repeat")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let part = text(heap, "string/repeat", arguments[0])?;
            let times = count(heap, "string/repeat", arguments[1])?;
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
        .function(|heap, arguments| Ok(number(text(heap, "string/to-number", arguments[0])?)));
    isolate
        .native(MODULE, "pad-start")
        .arity(2, Some(3))
        .function(|heap, arguments| pad(heap, "string/pad-start", arguments, true));
    isolate
        .native(MODULE, "pad-end")
        .arity(2, Some(3))
        .function(|heap, arguments| pad(heap, "string/pad-end", arguments, false));
}
