use std::collections::HashMap;

use crate::read::{self, Datum, Syntax, split_meta};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Form {
    pub indent: Option<usize>,
    pub defines: Option<usize>,
}

impl Form {
    const fn new(indent: Option<usize>, defines: Option<usize>) -> Self {
        Self { indent, defines }
    }

    pub(crate) fn of_meta(items: &[Syntax]) -> Self {
        let number = |name: &str| {
            items.chunks(2).find_map(|pair| match pair {
                [
                    Syntax {
                        datum: Datum::Keyword(key),
                        ..
                    },
                    Syntax {
                        datum: Datum::Int(number),
                        ..
                    },
                ] if key == name => usize::try_from(*number).ok(),
                _ => None,
            })
        };
        Self::new(number("indent"), number("defines"))
    }

    pub fn body(&self) -> bool {
        self.indent.is_some()
    }
}

const fn body(indent: usize) -> Form {
    Form::new(Some(indent), None)
}

const fn definition(indent: usize) -> Form {
    Form::new(Some(indent), Some(0))
}

const NONE: Form = Form::new(None, None);

pub(crate) const SPECIAL: &[(&str, Form)] = &[
    ("def", definition(1)),
    ("defonce", definition(1)),
    ("defmacro", definition(2)),
    ("defrecord", definition(1)),
    ("deferror", definition(1)),
    ("fn", body(1)),
    ("let", body(1)),
    ("loop", body(1)),
    ("again", NONE),
    ("if", body(1)),
    ("do", body(0)),
    ("try", body(0)),
    ("catch", body(2)),
    ("finally", body(0)),
    ("match", body(1)),
    ("module", body(0)),
    ("set!", NONE),
    ("quote", NONE),
];

pub(crate) const BUILT_IN: &[(&str, Form)] = &[
    ("when", body(1)),
    ("unless", body(1)),
    ("cond", body(0)),
    ("case", body(1)),
    ("each", body(1)),
    ("for", body(1)),
    ("while", body(1)),
    ("until", body(1)),
    ("if-let", body(1)),
    ("when-let", body(1)),
    ("test", body(1)),
    ("gen", body(0)),
    ("spawn", body(0)),
    ("atomically", body(0)),
    ("maybe", body(0)),
    ("defrule", definition(1)),
    ("defrules", definition(2)),
];

const BODY_PREFIXES: [&str; 4] = ["def", "with-", "when-", "if-"];

pub fn special(name: &str) -> bool {
    SPECIAL.iter().any(|(known, _)| *known == name)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Forms {
    known: HashMap<String, Form>,
}

impl Forms {
    pub fn special() -> Self {
        let mut forms = Self::default();
        for (name, form) in SPECIAL {
            forms.insert(*name, *form);
        }
        forms
    }

    pub fn built_in() -> Self {
        let mut forms = Self::special();
        for (name, form) in BUILT_IN {
            forms.insert(*name, *form);
        }
        forms
    }

    pub fn insert(&mut self, name: impl Into<String>, form: Form) {
        self.known.insert(name.into(), form);
    }

    pub fn get(&self, head: &str) -> Option<Form> {
        self.known.get(head).copied()
    }

    pub fn knows(&self, head: &str) -> bool {
        self.known.contains_key(head)
    }

    pub fn body(&self, head: &str) -> bool {
        self.get(head).is_some_and(|form| form.body())
            || BODY_PREFIXES.iter().any(|prefix| head.starts_with(prefix))
    }

    pub fn defines(&self, head: &str) -> Option<usize> {
        self.get(head)?.defines
    }

    pub fn scan(&mut self, source: &str) {
        for form in read::read(source).0 {
            let Datum::List(items) = &form.datum else {
                continue;
            };
            let Some((head, rest)) = items.split_first() else {
                continue;
            };
            if !matches!(&head.datum, Datum::Symbol(name) if name == "defmacro") {
                continue;
            }
            let (meta, rest) = split_meta(rest);
            if let Some(Syntax {
                datum: Datum::Symbol(name),
                ..
            }) = rest.first()
            {
                self.insert(name, meta.map(Form::of_meta).unwrap_or_default());
            }
        }
    }

    #[must_use]
    pub fn scanned(mut self, source: &str) -> Self {
        self.scan(source);
        self
    }
}
