use crate::error::Error;
use crate::read::{Datum, Syntax};

pub enum Key<'s> {
    Shorthand(&'s str),
    Literal(&'s Syntax),
}

pub struct Entry<'s> {
    pub key: Key<'s>,
    pub pattern: &'s Syntax,
}

pub fn entries(items: &[Syntax]) -> Result<Vec<Entry<'_>>, Error> {
    let mut found = Vec::new();
    let mut items = items.iter();
    while let Some(item) = items.next() {
        if let Datum::Symbol(name) = &item.datum {
            found.push(Entry {
                key: Key::Shorthand(name),
                pattern: item,
            });
            continue;
        }
        let Some(pattern) = items.next() else {
            return Err(Error::at(
                item.span,
                "This map pattern has a key without a pattern",
            ));
        };
        found.push(Entry {
            key: Key::Literal(item),
            pattern,
        });
    }
    Ok(found)
}

pub fn names(pattern: &Syntax) -> Vec<&Syntax> {
    match &pattern.datum {
        Datum::Symbol(name) if name == "_" || name == "&" => Vec::new(),
        Datum::Symbol(_) => vec![pattern],
        Datum::Vector(items) => items.iter().flat_map(names).collect(),
        Datum::Map(items) | Datum::Record(_, items) => entries(items)
            .map(|found| {
                found
                    .iter()
                    .flat_map(|entry| names(entry.pattern))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

pub fn is_pattern(syntax: &Syntax) -> bool {
    matches!(
        syntax.datum,
        Datum::Vector(_) | Datum::Map(_) | Datum::Record(..)
    )
}
