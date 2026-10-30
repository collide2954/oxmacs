use std::fmt;

use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum Datum {
    Nil,
    Bool(bool),
    Int(i64),
    BigInt(String),
    Float(f64),
    Char(char),
    Str(String),
    Symbol(String),
    Keyword(String),
    List(Vec<Syntax>),
    Vector(Vec<Syntax>),
    Map(Vec<Syntax>),
    Set(Vec<Syntax>),
    Record(String, Vec<Syntax>),
    Meta(Vec<Syntax>),
}

pub const PRELUDE_MARK: u32 = 0;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Marks(Vec<u32>);

impl Marks {
    pub fn prelude() -> Self {
        Self(vec![PRELUDE_MARK])
    }

    pub fn last(&self) -> Option<u32> {
        self.0.last().copied()
    }

    #[must_use]
    pub fn flipped(&self, mark: u32) -> Self {
        let mut marks = self.0.clone();
        match marks.binary_search(&mark) {
            Ok(at) => {
                marks.remove(at);
            }
            Err(at) => marks.insert(at, mark),
        }
        Self(marks)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Syntax {
    pub datum: Datum,
    pub span: Span,
    pub marks: Marks,
}

impl Syntax {
    pub fn new(datum: Datum, span: Span) -> Self {
        Self {
            datum,
            span,
            marks: Marks::default(),
        }
    }
}

pub fn split_meta(parts: &[Syntax]) -> (Option<&[Syntax]>, &[Syntax]) {
    match parts.split_first() {
        Some((
            Syntax {
                datum: Datum::Meta(items),
                ..
            },
            rest,
        )) => (Some(items), rest),
        _ => (None, parts),
    }
}

pub fn meta_flag(meta: Option<&[Syntax]>, name: &str) -> bool {
    meta.is_some_and(|items| {
        items.chunks(2).any(|pair| {
            matches!(pair, [Syntax { datum: Datum::Keyword(key), .. }, value]
                if key == name && !matches!(value.datum, Datum::Bool(false) | Datum::Nil))
        })
    })
}

fn joined(formatter: &mut fmt::Formatter<'_>, items: &[Syntax]) -> fmt::Result {
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            formatter.write_str(" ")?;
        }
        write!(formatter, "{item}")?;
    }
    Ok(())
}

impl fmt::Display for Syntax {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (open, items, close) = match &self.datum {
            Datum::Nil => return formatter.write_str("nil"),
            Datum::Bool(value) => return write!(formatter, "{value}"),
            Datum::Int(number) => return write!(formatter, "{number}"),
            Datum::BigInt(digits) => return formatter.write_str(digits),
            Datum::Float(number) => return write!(formatter, "{number:?}"),
            Datum::Char(character) => return write!(formatter, "\\{character}"),
            Datum::Str(text) => return write!(formatter, "{text:?}"),
            Datum::Symbol(name) => return formatter.write_str(name),
            Datum::Keyword(name) => return write!(formatter, ":{name}"),
            Datum::List(items) => ("(".to_owned(), items, ")"),
            Datum::Vector(items) => ("[".to_owned(), items, "]"),
            Datum::Map(items) => ("{".to_owned(), items, "}"),
            Datum::Set(items) => ("#{".to_owned(), items, "}"),
            Datum::Record(name, items) => (format!("#{name}{{"), items, "}"),
            Datum::Meta(items) => ("^{".to_owned(), items, "}"),
        };
        formatter.write_str(&open)?;
        joined(formatter, items)?;
        formatter.write_str(close)
    }
}

#[cfg(test)]
mod tests {
    use super::Marks;

    #[test]
    fn flipping_a_mark_twice_removes_it() {
        let one = Marks::default().flipped(3);
        let two = one.flipped(1);

        assert_eq!(two, Marks::default().flipped(1).flipped(3));
        assert_eq!(two.flipped(3), Marks::default().flipped(1));
        assert_eq!(one.flipped(3), Marks::default());
    }
}
