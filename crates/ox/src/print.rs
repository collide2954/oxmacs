use std::fmt::Write as _;

use crate::heap::Heap;
use crate::map::Map;
use crate::record::Record;
use crate::value::Value;
use crate::vector::Vector;

pub fn write(heap: &Heap, value: Value) -> String {
    let mut out = String::new();
    write_into(heap, value, &mut out);
    out
}

pub fn display(heap: &Heap, value: Value) -> String {
    match (value, heap.text(value)) {
        (Value::Char(character), _) => character.to_string(),
        (_, Some(text)) => text.to_owned(),
        _ => write(heap, value),
    }
}

fn write_into(heap: &Heap, value: Value, out: &mut String) {
    match value {
        Value::Nil => out.push_str("nil"),
        Value::Bool(flag) => out.push_str(if flag { "true" } else { "false" }),
        Value::Int(number) => {
            let _ = write!(out, "{number}");
        }
        Value::Float(number) => float(number, out),
        Value::Char(character) => self::character(character, out),
        Value::Symbol(name) => out.push_str(name.name()),
        Value::Keyword(name) => {
            out.push(':');
            out.push_str(name.name());
        }
        Value::Ref(_) => {
            if let Some(text) = heap.text(value) {
                string(text, out);
            } else if let Some(number) = heap.bigint_of(value) {
                let _ = write!(out, "{number}");
            } else if heap.pair_of(value).is_some() {
                list(heap, value, out);
            } else if let Some(items) = heap.vector_of(value) {
                vector(heap, items, out);
            } else if let Some(entries) = heap.map_of(value) {
                map(heap, entries, out);
            } else if let Some(items) = heap.set_of(value) {
                set(heap, items, out);
            } else if let Some(fields) = heap.record_of(value) {
                record(heap, fields, out);
            } else if let Some(kind) = heap.record_type_of(value) {
                let _ = write!(out, "#<record {}>", kind.name.name());
            } else if let Some(held) = heap.atom_get(value) {
                out.push_str("#<atom ");
                write_into(heap, held, out);
                out.push('>');
            } else if let Some(native) = heap.native_of(value) {
                let _ = write!(out, "#<native {}>", native.name.name());
            } else if let Some(closure) = heap.closure_of(value) {
                match closure.code.name {
                    Some(name) => {
                        let _ = write!(out, "#<fn {}>", name.name());
                    }
                    None => out.push_str("#<fn>"),
                }
            }
        }
    }
}

fn float(number: f64, out: &mut String) {
    if number.is_nan() {
        out.push_str("nan");
    } else if number.is_infinite() {
        out.push_str(if number > 0.0 { "inf" } else { "-inf" });
    } else {
        let _ = write!(out, "{number:?}");
    }
}

fn character(character: char, out: &mut String) {
    let name = match character {
        '\n' => "newline",
        ' ' => "space",
        '\t' => "tab",
        '\r' => "return",
        '\0' => "nul",
        _ if character.is_control() => {
            let _ = write!(out, "\\u{{{:x}}}", u32::from(character));
            return;
        }
        _ => {
            out.push('\\');
            out.push(character);
            return;
        }
    };
    out.push('\\');
    out.push_str(name);
}

fn string(text: &str, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            '{' => out.push_str("{{"),
            '}' => out.push_str("}}"),
            _ if character.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", u32::from(character));
            }
            _ => out.push(character),
        }
    }
    out.push('"');
}

fn list(heap: &Heap, value: Value, out: &mut String) {
    out.push('(');
    let mut rest = value;
    let mut first = true;
    while let Some(pair) = heap.pair_of(rest) {
        if !first {
            out.push(' ');
        }
        first = false;
        write_into(heap, pair.head, out);
        rest = pair.tail;
    }
    if rest != Value::Nil {
        out.push_str(" . ");
        write_into(heap, rest, out);
    }
    out.push(')');
}

fn vector(heap: &Heap, vector: Vector, out: &mut String) {
    out.push('[');
    for (index, item) in vector.items(heap).into_iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        write_into(heap, item, out);
    }
    out.push(']');
}

fn map(heap: &Heap, map: Map, out: &mut String) {
    out.push('{');
    for (index, (key, value)) in map.entries(heap).into_iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        if let Value::Keyword(name) = key {
            out.push_str(name.name());
            out.push(':');
        } else {
            write_into(heap, key, out);
        }
        out.push(' ');
        write_into(heap, value, out);
    }
    out.push('}');
}

fn set(heap: &Heap, set: Map, out: &mut String) {
    out.push_str("#{");
    for (index, (item, _)) in set.entries(heap).into_iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        write_into(heap, item, out);
    }
    out.push('}');
}

fn record(heap: &Heap, record: &Record, out: &mut String) {
    out.push('#');
    out.push_str(record.type_name(heap));
    out.push('{');
    let names: &[_] = heap
        .record_type_of(record.kind)
        .map_or(&[], |kind| &kind.fields);
    for (index, (name, value)) in names.iter().zip(&record.fields).enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(name.name());
        out.push_str(": ");
        write_into(heap, *value, out);
    }
    out.push('}');
}

#[cfg(test)]
mod tests {
    use super::{display, write};
    use crate::heap::Heap;
    use crate::value::Value;

    fn written(value: Value) -> String {
        write(&Heap::default(), value)
    }

    #[test]
    fn scalars_print_as_they_read() {
        let cases = [
            (Value::Nil, "nil"),
            (Value::Bool(true), "true"),
            (Value::Bool(false), "false"),
            (Value::Int(-7), "-7"),
            (Value::Float(3.0), "3.0"),
            (Value::Float(0.25), "0.25"),
            (Value::Float(1e21), "1e21"),
            (Value::Float(f64::INFINITY), "inf"),
            (Value::Float(f64::NEG_INFINITY), "-inf"),
            (Value::Float(f64::NAN), "nan"),
            (Value::symbol("forecast"), "forecast"),
            (Value::keyword("name"), ":name"),
        ];

        for (value, text) in cases {
            assert_eq!(written(value), text, "{value:?}");
        }
    }

    #[test]
    fn characters_print_by_themselves_or_by_name() {
        let cases = [
            ('a', r"\a"),
            ('λ', r"\λ"),
            ('\n', r"\newline"),
            (' ', r"\space"),
            ('\t', r"\tab"),
            ('\r', r"\return"),
            ('\0', r"\nul"),
            ('\u{1}', r"\u{1}"),
        ];

        for (character, text) in cases {
            assert_eq!(written(Value::Char(character)), text, "{character:?}");
        }
    }

    #[test]
    fn strings_print_quoted_with_escapes_and_doubled_braces() {
        let mut heap = Heap::default();
        let text = heap.string("a\"b\\c\nd\t{e}\u{1}").unwrap();

        assert_eq!(write(&heap, text), r#""a\"b\\c\nd\t{{e}}\u{1}""#);
    }

    #[test]
    fn lists_print_in_parentheses() {
        let mut heap = Heap::default();
        let inner = heap.list([Value::Int(3)].into_iter()).unwrap();
        let two = heap.string("two").unwrap();
        let list = heap.list([Value::Int(1), two, inner].into_iter()).unwrap();
        let dotted = heap.pair(Value::Int(1), Value::Int(2)).unwrap();

        assert_eq!(write(&heap, list), r#"(1 "two" (3))"#);
        assert_eq!(write(&heap, dotted), "(1 . 2)");
    }

    #[test]
    fn display_shows_strings_and_characters_as_they_are() {
        let mut heap = Heap::default();
        let text = heap.string("hi {x}").unwrap();
        let list = heap.list([text].into_iter()).unwrap();

        assert_eq!(display(&heap, text), "hi {x}");
        assert_eq!(display(&heap, Value::Char('a')), "a");
        assert_eq!(display(&heap, list), r#"("hi {{x}}")"#);
        assert_eq!(display(&heap, Value::Int(2)), "2");
    }
}
