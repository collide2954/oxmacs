use std::collections::HashMap;
use std::rc::Rc;

use crate::compile::parameters::parse_parameters;
use crate::read::{self, Datum, Syntax};
use crate::span::{Source, Span, line_column};
use crate::symbol::Sym;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Function,
    Macro,
    Native,
    Record,
    Special,
    Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub source: Rc<Source>,
    pub span: Span,
}

impl Place {
    pub fn file(&self) -> &str {
        &self.source.name
    }

    pub fn line_column(&self) -> (usize, usize) {
        line_column(&self.source.text, self.span.start)
    }
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Macro => "macro",
            Self::Native => "native",
            Self::Record => "record",
            Self::Special => "special",
            Self::Value => "value",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Description {
    pub name: String,
    pub kind: Kind,
    pub signature: String,
    pub doc: Option<String>,
    pub place: Option<Place>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDocs {
    pub module: String,
    pub entries: Vec<Description>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub parameters: String,
    pub doc: String,
    pub special: bool,
    pub least: usize,
    pub most: Option<usize>,
    pub positional: Option<usize>,
    pub keywords: usize,
}

impl Entry {
    pub fn fits(&self, least: usize, most: Option<usize>) -> bool {
        let pairs = self.positional.map(|most| most + 2 * self.keywords);
        least == self.least && (most == self.most || (self.keywords > 0 && most == pairs))
    }
}

pub(crate) fn signature(name: &str, parameters: &str) -> String {
    if parameters.is_empty() {
        format!("({name})")
    } else {
        format!("({name} {parameters})")
    }
}

fn entry(signature: &Syntax, doc: &Syntax, special: bool) -> Option<(String, Entry)> {
    let (Datum::List(parts), Datum::Str(doc)) = (&signature.datum, &doc.datum) else {
        return None;
    };
    let (Datum::Symbol(name), parameters) = (&parts.first()?.datum, &parts[1..]) else {
        return None;
    };
    let (least, most, keywords) = match parse_parameters(parameters) {
        Ok(parsed) => {
            let least = parsed.required.len();
            let most = parsed
                .rest
                .is_none()
                .then_some(least + parsed.optional.len());
            (least, most, parsed.keywords.len())
        }
        Err(_) if special => (0, None, 0),
        Err(_) => return None,
    };
    let shown: Vec<String> = parameters.iter().map(ToString::to_string).collect();
    Some((
        name.clone(),
        Entry {
            parameters: shown.join(" "),
            doc: doc.clone(),
            special,
            least,
            most: most.filter(|_| keywords == 0),
            positional: most,
            keywords,
        },
    ))
}

fn pairs(
    items: &[Syntax],
    special: bool,
    found: &mut HashMap<String, Entry>,
    broken: &mut Vec<String>,
) {
    for pair in items.chunks(2) {
        match pair {
            [signature, doc] => match entry(signature, doc, special) {
                Some((name, entry)) => {
                    found.insert(name, entry);
                }
                None => broken.push(format!("Unreadable doc entry {signature}")),
            },
            [lone] => broken.push(format!("Doc entry {lone} has no doc string")),
            _ => {}
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Docs {
    pub entries: HashMap<String, Entry>,
    pub broken: Vec<String>,
}

pub(crate) fn parse(tables: &[&str]) -> Docs {
    let mut found = HashMap::new();
    let mut broken = Vec::new();
    for table in tables {
        let (forms, errors) = read::read(table);
        broken.extend(errors.into_iter().map(|error| error.message));
        let mut plain = Vec::new();
        for form in forms {
            match &form.datum {
                Datum::List(items) if matches!(items.first().map(|item| &item.datum), Some(Datum::Symbol(head)) if head == "special") =>
                {
                    pairs(&items[1..], true, &mut found, &mut broken);
                }
                _ => plain.push(form),
            }
        }
        pairs(&plain, false, &mut found, &mut broken);
    }
    Docs {
        entries: found,
        broken,
    }
}

pub(crate) fn fallback_with(least: usize, most: Option<usize>, keywords: &[Sym]) -> String {
    if keywords.is_empty() {
        return fallback(least, most);
    }
    let positional = most.map(|most| most.saturating_sub(2 * keywords.len()));
    let names: Vec<&str> = keywords.iter().map(|keyword| keyword.name()).collect();
    let shown = format!("{} {{{}}}", fallback(least, positional), names.join(" "));
    shown.trim_start().to_owned()
}

pub(crate) fn fallback(least: usize, most: Option<usize>) -> String {
    let mut parts: Vec<String> = (1..=least).map(|index| format!("arg{index}")).collect();
    match most {
        None => parts.push("& more".to_owned()),
        Some(most) => parts.extend((least + 1..=most).map(|index| format!("(arg{index})"))),
    }
    parts.join(" ")
}
