pub(crate) mod patterns;
pub(crate) mod quote;

use std::collections::HashMap;

use crate::error::Error;
use crate::heap::{Heap, HeapFull, Identifier};
use crate::read::{Datum, Marks, Syntax};
use crate::span::Span;
use crate::symbol::Sym;
use crate::value::{Ref, Value, print};
use crate::vm::Runtime;
use crate::vm::build::{make_map, make_set, make_vector};
use crate::vm::modules::{ModuleId, reference};

const MAX_DEPTH: usize = 256;

const SPECIAL: [&str; 22] = [
    "if",
    "do",
    "def",
    "fn",
    "let",
    "loop",
    "again",
    "set!",
    ".",
    "defrecord",
    "defmacro",
    "quote",
    "quasiquote",
    "unquote",
    "unquote-splicing",
    "match",
    "try",
    "catch",
    "finally",
    "deferror",
    "defonce",
    "module",
];

pub fn expand(runtime: &mut Runtime, form: &Syntax, module: ModuleId) -> Result<Syntax, Error> {
    Expander {
        runtime,
        module,
        locals: Vec::new(),
        depth: 0,
    }
    .form(form)
}

struct Expander<'r> {
    runtime: &'r mut Runtime,
    module: ModuleId,
    locals: Vec<(Sym, Marks)>,
    depth: usize,
}

fn symbol(syntax: &Syntax) -> Option<&str> {
    match &syntax.datum {
        Datum::Symbol(name) => Some(name),
        _ => None,
    }
}

fn rebuilt(syntax: &Syntax, datum: Datum) -> Syntax {
    Syntax {
        datum,
        span: syntax.span,
        marks: syntax.marks.clone(),
    }
}

fn full(span: Span) -> impl Fn(HeapFull) -> Error {
    move |error| Error::at(span, error.to_string())
}

type Spans = HashMap<Ref, Span>;

fn quote_syntax(heap: &mut Heap, syntax: &Syntax, spans: &mut Spans) -> Result<Value, Error> {
    let quoted = quote_datum(heap, syntax, spans)?;
    if let Value::Ref(reference) = quoted {
        spans.insert(reference, syntax.span);
    }
    Ok(quoted)
}

fn quote_datum(heap: &mut Heap, syntax: &Syntax, spans: &mut Spans) -> Result<Value, Error> {
    let span = syntax.span;
    let mut values = |heap: &mut Heap, items: &[Syntax]| {
        items
            .iter()
            .map(|item| quote_syntax(heap, item, spans))
            .collect::<Result<Vec<_>, _>>()
    };
    match &syntax.datum {
        Datum::Symbol(name) => heap
            .identifier(Identifier {
                name: Sym::intern(name),
                marks: syntax.marks.clone(),
                span,
            })
            .map_err(full(span)),
        Datum::List(items) => {
            let items = values(heap, items)?;
            heap.list(items.into_iter()).map_err(full(span))
        }
        Datum::Vector(items) => {
            let items = values(heap, items)?;
            make_vector(heap, &items).map_err(|error| error.or_at(span))
        }
        Datum::Map(items) if items.len() % 2 == 1 => {
            let mut pairs = Vec::with_capacity(items.len() + 1);
            for entry in patterns::entries(items)? {
                let key = match entry.key {
                    patterns::Key::Shorthand(name) => Value::keyword(name),
                    patterns::Key::Literal(key) => quote_syntax(heap, key, spans)?,
                };
                pairs.push(key);
                pairs.push(quote_syntax(heap, entry.pattern, spans)?);
            }
            make_map(heap, &pairs).map_err(|error| error.or_at(span))
        }
        Datum::Map(items) => {
            let items = values(heap, items)?;
            make_map(heap, &items).map_err(|error| error.or_at(span))
        }
        Datum::Set(items) => {
            let items = values(heap, items)?;
            make_set(heap, &items).map_err(|error| error.or_at(span))
        }
        Datum::Record(..) => Err(Error::at(
            span,
            "A record literal cannot be passed to a macro yet",
        )),
        _ => quote::datum_value(heap, syntax),
    }
}

fn syntax_of(
    heap: &Heap,
    value: Value,
    mark: u32,
    span: Span,
    spans: &Spans,
) -> Result<Syntax, Value> {
    syntax_at(heap, value, mark, span, spans, 0)
}

fn syntax_at(
    heap: &Heap,
    value: Value,
    mark: u32,
    span: Span,
    spans: &Spans,
    depth: usize,
) -> Result<Syntax, Value> {
    if depth > crate::vm::convert::DEEPEST {
        return Err(value);
    }
    let own = match value {
        Value::Ref(reference) => spans.get(&reference).copied().unwrap_or(span),
        _ => span,
    };
    let plain = |datum| Syntax::new(datum, own);
    if let Some(identifier) = heap.identifier_of(value) {
        return Ok(Syntax {
            datum: Datum::Symbol(identifier.name.name().to_owned()),
            span: identifier.span,
            marks: identifier.marks.clone(),
        });
    }
    let all = |items: Vec<Value>| {
        items
            .into_iter()
            .map(|item| syntax_at(heap, item, mark, span, spans, depth + 1))
            .collect::<Result<Vec<_>, _>>()
    };
    let datum = match value {
        Value::Nil => Datum::Nil,
        Value::Bool(flag) => Datum::Bool(flag),
        Value::Int(number) => Datum::Int(number),
        Value::Float(number) => Datum::Float(number),
        Value::Char(character) => Datum::Char(character),
        Value::Keyword(name) => Datum::Keyword(name.name().to_owned()),
        Value::Symbol(name) => {
            return Ok(Syntax {
                datum: Datum::Symbol(name.name().to_owned()),
                span,
                marks: Marks::default().flipped(mark),
            });
        }
        Value::Ref(_) => {
            if let Some(text) = heap.text(value) {
                Datum::Str(text.to_owned())
            } else if let Some(number) = heap.bigint_of(value) {
                Datum::BigInt(number.to_string())
            } else if heap.pair_of(value).is_some() {
                let mut rest = value;
                while let Some(pair) = heap.pair_of(rest) {
                    rest = pair.tail;
                }
                if rest != Value::Nil {
                    return Err(value);
                }
                Datum::List(all(heap.items(value))?)
            } else if let Some(vector) = heap.vector_of(value) {
                Datum::Vector(all(vector.items(heap))?)
            } else if let Some(map) = heap.map_of(value) {
                let items = map
                    .entries(heap)
                    .into_iter()
                    .flat_map(|(key, item)| [key, item])
                    .collect();
                Datum::Map(all(items)?)
            } else if let Some(set) = heap.set_of(value) {
                let items = set
                    .entries(heap)
                    .into_iter()
                    .map(|(item, _)| item)
                    .collect();
                Datum::Set(all(items)?)
            } else {
                return Err(value);
            }
        }
    };
    Ok(plain(datum))
}

impl Expander<'_> {
    fn module_for(&self, marks: &Marks) -> ModuleId {
        marks
            .last()
            .and_then(|mark| self.runtime.module_of_mark(mark))
            .unwrap_or(self.module)
    }

    fn reference(&self, form: &Syntax, name: &str) -> Syntax {
        let home = self.module_for(&form.marks);
        if home == self.module || self.bound(name, &form.marks) {
            return form.clone();
        }
        match self.runtime.modules.get(home) {
            Some(module) => rebuilt(
                form,
                Datum::Symbol(reference(&module.name, name).name().to_owned()),
            ),
            None => form.clone(),
        }
    }

    fn form(&mut self, form: &Syntax) -> Result<Syntax, Error> {
        match &form.datum {
            Datum::Symbol(name) => Ok(self.reference(form, name)),
            Datum::List(items) => self.list(form, items),
            Datum::Vector(items) => Ok(rebuilt(form, Datum::Vector(self.all(items)?))),
            Datum::Map(items) => Ok(rebuilt(form, Datum::Map(self.all(items)?))),
            Datum::Set(items) => Ok(rebuilt(form, Datum::Set(self.all(items)?))),
            Datum::Record(name, items) => {
                Ok(rebuilt(form, Datum::Record(name.clone(), self.all(items)?)))
            }
            _ => Ok(form.clone()),
        }
    }

    fn all(&mut self, items: &[Syntax]) -> Result<Vec<Syntax>, Error> {
        items.iter().map(|item| self.form(item)).collect()
    }

    fn bind(&mut self, name: &Syntax) {
        if let Some(text) = symbol(name) {
            self.locals.push((Sym::intern(text), name.marks.clone()));
        }
    }

    fn bound(&self, name: &str, marks: &Marks) -> bool {
        let name = Sym::intern(name);
        self.locals
            .iter()
            .any(|(local, local_marks)| *local == name && local_marks == marks)
    }

    fn list(&mut self, form: &Syntax, items: &[Syntax]) -> Result<Syntax, Error> {
        let Some((head, rest)) = items.split_first() else {
            return Ok(form.clone());
        };
        let Some(name) = symbol(head) else {
            return Ok(rebuilt(form, Datum::List(self.all(items)?)));
        };
        let rebound = matches!(name, "loop" | "again") && self.bound(name, &head.marks);
        if SPECIAL.contains(&name) && !rebound {
            return self.special(form, name, items);
        }
        if !self.bound(name, &head.marks)
            && let Some((transformer, home)) = self
                .runtime
                .lookup_macro(self.module_for(&head.marks), Sym::intern(name))
        {
            return self.call_macro(form, name, transformer, home, rest);
        }
        Ok(rebuilt(form, Datum::List(self.all(items)?)))
    }

    fn special(&mut self, form: &Syntax, name: &str, items: &[Syntax]) -> Result<Syntax, Error> {
        match name {
            "quote" | "unquote" | "unquote-splicing" | "module" => Ok(form.clone()),
            "quasiquote" => {
                let [_, template] = items else {
                    return Err(Error::at(form.span, "quasiquote takes one form"));
                };
                let rewritten = quote::quasiquote(template, 1)?;
                self.form(&rewritten)
            }
            "fn" => self.function(form, items),
            "def" | "defmacro" => self.definition(form, items),
            "defrecord" => self.record(form, items),
            "let" | "loop" => self.binding(form, items),
            "match" => self.match_form(form, items),
            "try" => self.try_form(form, items),
            "deferror" => self.error_form(form, items),
            "." => {
                let mut out = items.to_vec();
                if let Some(object) = items.get(1) {
                    out[1] = self.form(object)?;
                }
                Ok(rebuilt(form, Datum::List(out)))
            }
            _ => {
                let mut out = vec![items[0].clone()];
                out.extend(self.all(&items[1..])?);
                Ok(rebuilt(form, Datum::List(out)))
            }
        }
    }

    fn parameters(&mut self, items: &[Syntax]) -> Result<Vec<Syntax>, Error> {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            match &item.datum {
                Datum::Symbol(name) if name == "&" => out.push(item.clone()),
                Datum::Symbol(_) => {
                    self.bind(item);
                    out.push(item.clone());
                }
                Datum::List(parts) => match parts.as_slice() {
                    [name, default] => {
                        let default = self.form(default)?;
                        self.bind(name);
                        out.push(rebuilt(item, Datum::List(vec![name.clone(), default])));
                    }
                    [name] => {
                        self.bind(name);
                        out.push(item.clone());
                    }
                    _ => out.push(item.clone()),
                },
                Datum::Vector(_) | Datum::Record(..) => {
                    for binder in patterns::names(item) {
                        self.bind(binder);
                    }
                    out.push(item.clone());
                }
                Datum::Map(pairs) => {
                    let mut expanded = Vec::with_capacity(pairs.len());
                    for pair in pairs.chunks(2) {
                        if let [name, default] = pair {
                            let default = self.form(default)?;
                            self.bind(name);
                            expanded.extend([name.clone(), default]);
                        }
                    }
                    out.push(rebuilt(item, Datum::Map(expanded)));
                }
                _ => out.push(item.clone()),
            }
        }
        Ok(out)
    }

    fn vector_parameters(&mut self, parameters: &Syntax) -> Result<Syntax, Error> {
        match &parameters.datum {
            Datum::Vector(items) => Ok(rebuilt(parameters, Datum::Vector(self.parameters(items)?))),
            _ => Ok(parameters.clone()),
        }
    }

    fn function(&mut self, form: &Syntax, items: &[Syntax]) -> Result<Syntax, Error> {
        let saved = self.locals.len();
        let mut out = vec![items[0].clone()];
        let mut index = 1;
        if let Some(name) = items.get(1).filter(|name| symbol(name).is_some()) {
            self.bind(name);
            out.push(name.clone());
            index = 2;
        }
        if let Some(parameters) = items.get(index) {
            out.push(self.vector_parameters(parameters)?);
            out.extend(self.all(&items[index + 1..])?);
        }
        self.locals.truncate(saved);
        Ok(rebuilt(form, Datum::List(out)))
    }

    fn definition(&mut self, form: &Syntax, items: &[Syntax]) -> Result<Syntax, Error> {
        if let [head, meta, rest @ ..] = items
            && matches!(meta.datum, Datum::Meta(_))
        {
            let mut plain = vec![head.clone()];
            plain.extend_from_slice(rest);
            let expanded = self.definition(form, &plain)?;
            let Datum::List(mut out) = expanded.datum else {
                return Ok(expanded);
            };
            out.insert(1, meta.clone());
            return Ok(rebuilt(form, Datum::List(out)));
        }
        let saved = self.locals.len();
        let out = match items {
            [head, signature, body @ ..] if matches!(signature.datum, Datum::List(_)) => {
                let Datum::List(parts) = &signature.datum else {
                    return Ok(form.clone());
                };
                let mut signature_out = parts.first().cloned().into_iter().collect::<Vec<_>>();
                signature_out.extend(self.parameters(parts.get(1..).unwrap_or_default())?);
                let mut out = vec![head.clone(), rebuilt(signature, Datum::List(signature_out))];
                out.extend(self.all(body)?);
                out
            }
            [head, name, parameters, body @ ..] if symbol(head) == Some("defmacro") => {
                let mut out = vec![
                    head.clone(),
                    name.clone(),
                    self.vector_parameters(parameters)?,
                ];
                out.extend(self.all(body)?);
                out
            }
            [head, rest @ ..] => {
                let mut out = vec![head.clone()];
                if let Some((name, values)) = rest.split_first() {
                    out.push(name.clone());
                    out.extend(self.all(values)?);
                }
                out
            }
            [] => Vec::new(),
        };
        self.locals.truncate(saved);
        Ok(rebuilt(form, Datum::List(out)))
    }

    fn record(&mut self, form: &Syntax, items: &[Syntax]) -> Result<Syntax, Error> {
        let saved = self.locals.len();
        let mut out = items.to_vec();
        if let Some(fields) = items.get(2) {
            out[2] = self.vector_parameters(fields)?;
        }
        self.locals.truncate(saved);
        Ok(rebuilt(form, Datum::List(out)))
    }

    fn binding(&mut self, form: &Syntax, items: &[Syntax]) -> Result<Syntax, Error> {
        let Some(Datum::Vector(bindings)) = items.get(1).map(|vector| &vector.datum) else {
            return Ok(rebuilt(form, Datum::List(self.all(items)?)));
        };
        let saved = self.locals.len();
        let mut expanded = Vec::with_capacity(bindings.len());
        for pair in bindings.chunks(2) {
            if let [name, value] = pair {
                let value = self.form(value)?;
                for binder in patterns::names(name) {
                    self.bind(binder);
                }
                expanded.extend([name.clone(), value]);
            } else {
                expanded.extend(pair.iter().cloned());
            }
        }
        let mut out = vec![
            items[0].clone(),
            rebuilt(&items[1], Datum::Vector(expanded)),
        ];
        out.extend(self.all(&items[2..])?);
        self.locals.truncate(saved);
        Ok(rebuilt(form, Datum::List(out)))
    }

    fn try_form(&mut self, form: &Syntax, items: &[Syntax]) -> Result<Syntax, Error> {
        let mut out = vec![items[0].clone()];
        for item in &items[1..] {
            let expanded = match &item.datum {
                Datum::List(parts)
                    if parts.len() >= 3 && parts.first().and_then(symbol) == Some("catch") =>
                {
                    let saved = self.locals.len();
                    self.bind(&parts[2]);
                    let mut clause = parts[..3].to_vec();
                    clause.extend(self.all(&parts[3..])?);
                    self.locals.truncate(saved);
                    rebuilt(item, Datum::List(clause))
                }
                _ => self.form(item)?,
            };
            out.push(expanded);
        }
        Ok(rebuilt(form, Datum::List(out)))
    }

    fn error_form(&mut self, form: &Syntax, items: &[Syntax]) -> Result<Syntax, Error> {
        let saved = self.locals.len();
        let mut out = items.to_vec();
        if let Some(fields) = items.get(2) {
            out[2] = self.vector_parameters(fields)?;
        }
        if let Some(template) = items.get(3) {
            out[3] = self.form(template)?;
        }
        self.locals.truncate(saved);
        Ok(rebuilt(form, Datum::List(out)))
    }

    fn match_form(&mut self, form: &Syntax, items: &[Syntax]) -> Result<Syntax, Error> {
        let mut out = vec![items[0].clone()];
        if let Some(scrutinee) = items.get(1) {
            out.push(self.form(scrutinee)?);
        }
        for clause in items.get(2..).unwrap_or_default() {
            let Datum::Vector(parts) = &clause.datum else {
                out.push(clause.clone());
                continue;
            };
            let saved = self.locals.len();
            let mut expanded = Vec::with_capacity(parts.len());
            if let Some((pattern, rest)) = parts.split_first() {
                for binder in patterns::names(pattern) {
                    self.bind(binder);
                }
                expanded.push(pattern.clone());
                expanded.extend(self.all(rest)?);
            }
            self.locals.truncate(saved);
            out.push(rebuilt(clause, Datum::Vector(expanded)));
        }
        Ok(rebuilt(form, Datum::List(out)))
    }

    fn call_macro(
        &mut self,
        form: &Syntax,
        name: &str,
        transformer: Value,
        home: ModuleId,
        arguments: &[Syntax],
    ) -> Result<Syntax, Error> {
        if self.depth >= MAX_DEPTH {
            return Err(Error::at(
                form.span,
                format!("Macro expansion went too deep in {name}"),
            ));
        }
        let mut spans = Spans::new();
        let values = arguments
            .iter()
            .map(|argument| quote_syntax(&mut self.runtime.heap, argument, &mut spans))
            .collect::<Result<Vec<_>, _>>()?;
        let produced = self
            .runtime
            .invoke(transformer, &values, form.span, &mut ())
            .map_err(|error| error.or_at(form.span))?;
        let mark = self.runtime.fresh_mark(home);
        let expansion =
            syntax_of(&self.runtime.heap, produced, mark, form.span, &spans).map_err(|bad| {
                Error::at(
                    form.span,
                    format!(
                        "{name} produced {}, which is not code",
                        print::brief(&self.runtime.heap, bad)
                    ),
                )
            })?;
        self.depth += 1;
        let result = self.form(&expansion);
        self.depth -= 1;
        result
    }
}
