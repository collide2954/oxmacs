use std::fmt::Write as _;

use crate::heap::Heap;
use crate::value::Value;
use crate::value::equal::identical;
use crate::value::promise::Promise;

const BRIEF: usize = 200;

enum Step {
    Value(Value),
    Text(&'static str),
    Owned(String),
    Leave,
}

struct Printer<'a> {
    heap: &'a Heap,
    out: String,
    stack: Vec<Step>,
    path: Vec<Value>,
    limit: Option<usize>,
}

pub fn write(heap: &Heap, value: Value) -> String {
    Printer::new(heap, None).run(value)
}

pub fn brief(heap: &Heap, value: Value) -> String {
    Printer::new(heap, Some(BRIEF)).run(value)
}

pub fn display(heap: &Heap, value: Value) -> String {
    match (value, heap.text(value)) {
        (Value::Char(character), _) => character.to_string(),
        (_, Some(text)) => text.to_owned(),
        _ => write(heap, value),
    }
}

impl<'a> Printer<'a> {
    fn new(heap: &'a Heap, limit: Option<usize>) -> Self {
        Self {
            heap,
            out: String::new(),
            stack: Vec::new(),
            path: Vec::new(),
            limit,
        }
    }

    fn run(mut self, value: Value) -> String {
        self.stack.push(Step::Value(value));
        while let Some(step) = self.stack.pop() {
            if let Some(limit) = self.limit
                && self.out.len() > limit
            {
                let mut end = limit;
                while !self.out.is_char_boundary(end) {
                    end -= 1;
                }
                self.out.truncate(end);
                self.out.push('…');
                return self.out;
            }
            match step {
                Step::Value(value) => self.value(value),
                Step::Text(text) => self.out.push_str(text),
                Step::Owned(text) => self.out.push_str(&text),
                Step::Leave => {
                    self.path.pop();
                }
            }
        }
        if let Some(limit) = self.limit
            && self.out.len() > limit
        {
            let mut end = limit;
            while !self.out.is_char_boundary(end) {
                end -= 1;
            }
            self.out.truncate(end);
            self.out.push('…');
        }
        self.out
    }

    fn sequence(&mut self, open: &str, close: &'static str, parts: Vec<Vec<Step>>) {
        self.out.push_str(open);
        self.stack.push(Step::Text(close));
        for (index, part) in parts.into_iter().enumerate().rev() {
            self.stack.extend(part.into_iter().rev());
            if index > 0 {
                self.stack.push(Step::Text(" "));
            }
        }
    }

    fn holder(&mut self, value: Value, label: &str, held: Value) {
        if self.path.iter().any(|seen| identical(*seen, value)) {
            let _ = write!(self.out, "#<{label} ...>");
            return;
        }
        let _ = write!(self.out, "#<{label} ");
        self.path.push(value);
        self.stack.push(Step::Text(">"));
        self.stack.push(Step::Leave);
        self.stack.push(Step::Value(held));
    }

    fn value(&mut self, value: Value) {
        let heap = self.heap;
        match value {
            Value::Nil => self.out.push_str("nil"),
            Value::Bool(flag) => self.out.push_str(if flag { "true" } else { "false" }),
            Value::Int(number) => {
                let _ = write!(self.out, "{number}");
            }
            Value::Float(number) => float(number, &mut self.out),
            Value::Char(character) => self::character(character, &mut self.out),
            Value::Symbol(name) => self.out.push_str(name.name()),
            Value::Keyword(name) => {
                self.out.push(':');
                self.out.push_str(name.name());
            }
            Value::Ref(_) => self.reference(heap, value),
        }
    }

    fn reference(&mut self, heap: &Heap, value: Value) {
        if let Some(text) = heap.text(value) {
            string(text, &mut self.out);
        } else if let Some(number) = heap.bigint_of(value) {
            let _ = write!(self.out, "{number}");
        } else if heap.pair_of(value).is_some() {
            self.list(value);
        } else if let Some(items) = heap.vector_of(value) {
            let parts = items
                .items(heap)
                .into_iter()
                .map(|item| vec![Step::Value(item)])
                .collect();
            self.sequence("[", "]", parts);
        } else if let Some(entries) = heap.map_of(value) {
            let parts = entries
                .entries(heap)
                .into_iter()
                .map(|(key, item)| {
                    let key = match key {
                        Value::Keyword(name) => Step::Owned(format!("{}:", name.name())),
                        other => Step::Value(other),
                    };
                    vec![key, Step::Text(" "), Step::Value(item)]
                })
                .collect();
            self.sequence("{", "}", parts);
        } else if let Some(items) = heap.set_of(value) {
            let parts = items
                .entries(heap)
                .into_iter()
                .map(|(item, _)| vec![Step::Value(item)])
                .collect();
            self.sequence("#{", "}", parts);
        } else if let Some(record) = heap.record_of(value) {
            let names: &[_] = heap
                .record_type_of(record.kind)
                .map_or(&[], |kind| &kind.fields);
            let parts = names
                .iter()
                .zip(&record.fields)
                .map(|(name, field)| {
                    vec![
                        Step::Owned(format!("{}: ", name.name())),
                        Step::Value(*field),
                    ]
                })
                .collect();
            let open = format!("#{}{{", record.type_name(heap));
            self.sequence(&open, "}", parts);
        } else if let Some(kind) = heap.record_type_of(value) {
            let _ = write!(self.out, "#<record {}>", kind.name.name());
        } else if let Some(identifier) = heap.identifier_of(value) {
            self.out.push_str(identifier.name.name());
        } else if let Some(held) = heap.atom_get(value) {
            self.holder(value, "atom", held);
        } else if heap.channel_of(value).is_some() {
            self.out.push_str("#<chan>");
        } else if let Some(foreign) = heap.foreign_entry(value) {
            let _ = write!(self.out, "#<{}>", foreign.name.name());
        } else if heap.generator_of(value).is_some() {
            self.out.push_str("#<generator>");
        } else if let Some(promise) = heap.promise_of(value) {
            match promise {
                Promise::Pending { .. } => self.out.push_str("#<promise>"),
                Promise::Delivered(held) => self.holder(value, "promise", held),
                Promise::Failed(_) => self.out.push_str("#<promise failed>"),
            }
        } else if let Some(native) = heap.native_of(value) {
            let _ = write!(self.out, "#<native {}>", native.name.name());
        } else if let Some(closure) = heap.closure_of(value) {
            match closure.code.name {
                Some(name) => {
                    let _ = write!(self.out, "#<fn {}>", name.name());
                }
                None => self.out.push_str("#<fn>"),
            }
        }
    }

    fn list(&mut self, value: Value) {
        let mut parts = Vec::new();
        let mut rest = value;
        while let Some(pair) = self.heap.pair_of(rest) {
            parts.push(vec![Step::Value(pair.head)]);
            rest = pair.tail;
        }
        if rest != Value::Nil {
            parts.push(vec![Step::Text("."), Step::Text(" "), Step::Value(rest)]);
        }
        self.sequence("(", ")", parts);
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
#[cfg(test)]
mod tests {
    use super::{brief, display, write};
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
    fn atoms_and_promises_that_hold_themselves_print_without_looping() {
        let mut heap = Heap::default();
        let atom = heap.atom(Value::Nil).unwrap();
        let inside = heap.vector_from(&[atom, Value::Int(1)]).unwrap();
        heap.atom_set(atom, inside);

        assert_eq!(write(&heap, atom), "#<atom [#<atom ...> 1]>");
    }

    #[test]
    fn deeply_nested_values_print_without_running_out_of_stack() {
        let mut heap = Heap::default();
        let mut nested = Value::Nil;
        for _ in 0..200_000 {
            nested = heap.vector_from(&[nested]).unwrap();
        }

        let shown = write(&heap, nested);

        assert_eq!(shown.len(), 200_000 * 2 + 3);
        assert!(shown.starts_with("[[[") && shown.ends_with("]]]"));
        assert_eq!(shown.find("nil"), Some(200_000));
    }

    #[test]
    fn brief_stops_after_a_few_hundred_characters() {
        let mut heap = Heap::default();
        let items: Vec<Value> = (0..100_000).map(Value::Int).collect();
        let big = heap.vector_from(&items).unwrap();

        let shown = brief(&heap, big);

        assert!(shown.chars().count() <= 201, "{}", shown.len());
        assert!(shown.starts_with("[0 1 2 ") && shown.ends_with('…'));
        assert_eq!(brief(&heap, Value::Int(5)), "5");
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
