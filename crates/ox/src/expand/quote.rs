use std::mem;

use num_bigint::BigInt;

use crate::error::Error;
use crate::heap::{Heap, HeapFull};
use crate::read::{Datum, Marks, Syntax};
use crate::span::Span;
use crate::value::Value;
use crate::vm::build::{make_map, make_set, make_vector};

#[derive(Clone, Copy)]
enum Shape {
    List,
    Vector,
}

fn full(span: Span) -> impl Fn(HeapFull) -> Error {
    move |error| Error::at(span, error.to_string())
}

fn values(heap: &mut Heap, items: &[Syntax]) -> Result<Vec<Value>, Error> {
    items.iter().map(|item| datum_value(heap, item)).collect()
}

pub fn datum_value(heap: &mut Heap, syntax: &Syntax) -> Result<Value, Error> {
    let span = syntax.span;
    match &syntax.datum {
        Datum::Nil => Ok(Value::Nil),
        Datum::Bool(flag) => Ok(Value::Bool(*flag)),
        Datum::Int(number) => Ok(Value::Int(*number)),
        Datum::Float(number) => Ok(Value::Float(*number)),
        Datum::Char(character) => Ok(Value::Char(*character)),
        Datum::Keyword(name) => Ok(Value::keyword(name)),
        Datum::Symbol(name) => Ok(Value::symbol(name)),
        Datum::Str(text) => heap.string(text).map_err(full(span)),
        Datum::BigInt(digits) => {
            let number = digits
                .parse::<BigInt>()
                .map_err(|_| Error::at(span, format!("{digits} is not a number")))?;
            heap.bigint(number).map_err(full(span))
        }
        Datum::List(items) => {
            let items = values(heap, items)?;
            heap.list(items.into_iter()).map_err(full(span))
        }
        Datum::Vector(items) => {
            let items = values(heap, items)?;
            make_vector(heap, &items).map_err(|error| error.or_at(span))
        }
        Datum::Map(items) => {
            let items = values(heap, items)?;
            make_map(heap, &items).map_err(|error| error.or_at(span))
        }
        Datum::Set(items) => {
            let items = values(heap, items)?;
            make_set(heap, &items).map_err(|error| error.or_at(span))
        }
        Datum::Record(..) => Err(Error::at(span, "A record literal cannot be quoted")),
        Datum::Meta(_) => Err(Error::at(span, MISPLACED_META)),
    }
}

pub const MISPLACED_META: &str = "^ metadata only goes before the name in def, defonce or defmacro";

fn symbol(name: &str, span: Span) -> Syntax {
    Syntax {
        datum: Datum::Symbol(name.to_owned()),
        span,
        marks: Marks::default(),
    }
}

fn call(name: &str, arguments: Vec<Syntax>, span: Span) -> Syntax {
    let mut items = vec![symbol(name, span)];
    items.extend(arguments);
    Syntax {
        datum: Datum::List(items),
        span,
        marks: Marks::default(),
    }
}

fn quoted(syntax: Syntax) -> Syntax {
    let span = syntax.span;
    Syntax {
        datum: Datum::List(vec![symbol("quote", span), syntax]),
        span,
        marks: Marks::default(),
    }
}

fn tagged<'s>(syntax: &'s Syntax, name: &str) -> Option<&'s Syntax> {
    let Datum::List(items) = &syntax.datum else {
        return None;
    };
    match items.as_slice() {
        [
            Syntax {
                datum: Datum::Symbol(head),
                ..
            },
            inner,
        ] if head == name => Some(inner),
        _ => None,
    }
}

fn unquoted(syntax: &Syntax, level: usize) -> bool {
    if let Some(inner) = tagged(syntax, "unquote").or_else(|| tagged(syntax, "unquote-splicing")) {
        return level == 1 || unquoted(inner, level - 1);
    }
    if let Some(inner) = tagged(syntax, "quasiquote") {
        return unquoted(inner, level + 1);
    }
    match &syntax.datum {
        Datum::List(items)
        | Datum::Vector(items)
        | Datum::Map(items)
        | Datum::Set(items)
        | Datum::Record(_, items) => items.iter().any(|item| unquoted(item, level)),
        _ => false,
    }
}

fn nested(name: &str, inner: &Syntax, level: usize, span: Span) -> Result<Syntax, Error> {
    Ok(call(
        "#list",
        vec![quoted(symbol(name, span)), quasiquote(inner, level)?],
        span,
    ))
}

fn expand_all(items: &[Syntax], level: usize) -> Result<Vec<Syntax>, Error> {
    items.iter().map(|item| quasiquote(item, level)).collect()
}

fn sequence(items: &[Syntax], level: usize, span: Span, shape: Shape) -> Result<Syntax, Error> {
    let splice = |item: &Syntax| {
        tagged(item, "unquote-splicing")
            .filter(|_| level == 1)
            .cloned()
    };
    if !items.iter().any(|item| splice(item).is_some()) {
        let expanded = expand_all(items, level)?;
        return Ok(match shape {
            Shape::List => call("#list", expanded, span),
            Shape::Vector => Syntax {
                datum: Datum::Vector(expanded),
                span,
                marks: Marks::default(),
            },
        });
    }
    let mut segments = Vec::new();
    let mut run = Vec::new();
    for item in items {
        if let Some(inner) = splice(item) {
            if !run.is_empty() {
                segments.push(Syntax {
                    datum: Datum::Vector(mem::take(&mut run)),
                    span,
                    marks: Marks::default(),
                });
            }
            segments.push(inner);
        } else {
            run.push(quasiquote(item, level)?);
        }
    }
    if !run.is_empty() {
        segments.push(Syntax {
            datum: Datum::Vector(run),
            span,
            marks: Marks::default(),
        });
    }
    let joined = call("#concat", segments, span);
    Ok(match shape {
        Shape::List => joined,
        Shape::Vector => call("#vec", vec![joined], span),
    })
}

pub fn quasiquote(template: &Syntax, level: usize) -> Result<Syntax, Error> {
    let span = template.span;
    if !unquoted(template, level) {
        return Ok(quoted(template.clone()));
    }
    if let Some(inner) = tagged(template, "unquote") {
        if level == 1 {
            return Ok(inner.clone());
        }
        return nested("unquote", inner, level - 1, span);
    }
    if let Some(inner) = tagged(template, "unquote-splicing") {
        if level == 1 {
            return Err(Error::at(span, "~@ splices into a list or vector"));
        }
        return nested("unquote-splicing", inner, level - 1, span);
    }
    if let Some(inner) = tagged(template, "quasiquote") {
        return nested("quasiquote", inner, level + 1, span);
    }
    match &template.datum {
        Datum::List(items) => sequence(items, level, span, Shape::List),
        Datum::Vector(items) => sequence(items, level, span, Shape::Vector),
        Datum::Map(items) => Ok(Syntax {
            datum: Datum::Map(expand_all(items, level)?),
            span,
            marks: Marks::default(),
        }),
        Datum::Set(items) => Ok(Syntax {
            datum: Datum::Set(expand_all(items, level)?),
            span,
            marks: Marks::default(),
        }),
        _ => Err(Error::at(span, "A record literal cannot be quoted")),
    }
}
