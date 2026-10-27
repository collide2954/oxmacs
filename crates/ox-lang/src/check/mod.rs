pub mod report;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::compile::parameters::parse_parameters;
use crate::expand::patterns;
use crate::native::arguments;
use crate::read::{Datum, Marks, Syntax, meta_flag, split_meta};
use crate::span::Span;
use crate::symbol::Sym;
use crate::value::Value;
use crate::vm::Runtime;
use crate::vm::modules::{self, ModuleId, PRELUDE};

pub const BUDGET: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub(crate) fn error(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            span,
        }
    }

    pub(crate) fn warning(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Warning,
            ..Self::error(code, message, span)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Binding {
    Local(Span),
    Global {
        module: Option<String>,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Occurrence {
    pub span: Span,
    pub binding: Binding,
    pub defines: bool,
}

pub fn binding_at(occurrences: &[Occurrence], offset: usize) -> Option<&Occurrence> {
    occurrences.iter().find(|occurrence| {
        (occurrence.span.start as usize..=occurrence.span.end as usize).contains(&offset)
    })
}

fn symbol(syntax: &Syntax) -> Option<&str> {
    match &syntax.datum {
        Datum::Symbol(name) => Some(name),
        _ => None,
    }
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, left) in a.chars().enumerate() {
        let mut current = vec![i + 1];
        for (j, right) in b.iter().enumerate() {
            let cost = usize::from(left != *right);
            current.push(
                (previous[j] + cost)
                    .min(previous[j + 1] + 1)
                    .min(current[j] + 1),
            );
        }
        previous = current;
    }
    previous[b.len()]
}

#[derive(Debug, Default)]
pub(crate) struct Unknown {
    aliases: HashSet<String>,
    names: HashSet<String>,
}

#[derive(Debug)]
pub(crate) struct Import {
    module: String,
    alias: Option<String>,
    only: Vec<String>,
    exclude: Vec<String>,
    span: Span,
}

impl Import {
    pub(crate) fn of(spec: &Syntax) -> Option<Self> {
        let Datum::List(items) = &spec.datum else {
            return None;
        };
        let segments: Vec<&str> = items.iter().map_while(symbol).collect();
        let mut import = Self {
            module: segments.join(" "),
            alias: segments.last().map(|last| (*last).to_owned()),
            only: Vec::new(),
            exclude: Vec::new(),
            span: spec.span,
        };
        let mut options = items[segments.len()..].iter();
        while let (Some(option), Some(value)) = (options.next(), options.next()) {
            match (&option.datum, &value.datum) {
                (Datum::Keyword(word), Datum::Symbol(name)) if word == "as" => {
                    import.alias = Some(name.clone());
                }
                (Datum::Keyword(word), Datum::Vector(names)) if word == "only" => {
                    import
                        .only
                        .extend(names.iter().filter_map(symbol).map(str::to_owned));
                }
                (Datum::Keyword(word), Datum::Vector(names)) if word == "exclude" => {
                    import
                        .exclude
                        .extend(names.iter().filter_map(symbol).map(str::to_owned));
                }
                _ => {}
            }
        }
        Some(import)
    }

    fn used(&self, names: &HashSet<&str>) -> bool {
        !self.exclude.is_empty()
            || self.only.iter().any(|name| names.contains(name.as_str()))
            || self.alias.as_ref().is_some_and(|alias| {
                let prefix = format!("{alias}/");
                names.iter().any(|name| name.starts_with(&prefix))
            })
    }
}

fn symbols<'s>(form: &'s Syntax, into: &mut HashSet<&'s str>) {
    match &form.datum {
        Datum::Symbol(name) => {
            into.insert(name);
        }
        Datum::List(items)
        | Datum::Vector(items)
        | Datum::Map(items)
        | Datum::Set(items)
        | Datum::Record(_, items) => {
            for item in items {
                symbols(item, into);
            }
        }
        _ => {}
    }
}

pub(crate) fn unused_imports(imports: &[Import], forms: &[&Syntax]) -> Vec<Diagnostic> {
    let mut names = HashSet::new();
    for form in forms {
        symbols(form, &mut names);
    }
    imports
        .iter()
        .filter(|import| !import.used(&names))
        .map(|import| {
            Diagnostic::warning(
                "unused",
                format!("({}) is imported but never used", import.module),
                import.span,
            )
        })
        .collect()
}

const REGEX_FUNCTIONS: [&str; 5] = ["find", "find-all", "matches?", "replace", "split"];

fn literal<'s>(source: &'s str, syntax: &Syntax) -> Option<&'s str> {
    if !matches!(syntax.datum, Datum::Str(_)) {
        return None;
    }
    let text = source.get(syntax.span.start as usize..syntax.span.end as usize)?;
    text.starts_with('"').then_some(text)
}

fn regex_call(regex: &[&Import], head: &Syntax) -> bool {
    let Some(name) = symbol(head) else {
        return false;
    };
    regex.iter().any(|import| match name.split_once('/') {
        Some((alias, short)) => {
            import.alias.as_deref() == Some(alias) && REGEX_FUNCTIONS.contains(&short)
        }
        None => import.only.iter().any(|only| only == name) && REGEX_FUNCTIONS.contains(&name),
    })
}

fn strings_in(source: &str, regex: &[&Import], form: &Syntax, found: &mut Vec<Diagnostic>) {
    let items = match &form.datum {
        Datum::List(items) => {
            let pattern = items
                .first()
                .filter(|head| regex_call(regex, head))
                .and_then(|_| items.get(1))
                .filter(|pattern| {
                    literal(source, pattern).is_some_and(|text| {
                        text.replace("\\\"", "").contains('\\') || text.contains("{{")
                    })
                });
            if let Some(pattern) = pattern {
                found.push(Diagnostic::warning(
                    "regex-string",
                    "this pattern escapes backslashes or braces; a regex literal, #\"…\", needs no escapes",
                    pattern.span,
                ));
            }
            items
                .iter()
                .filter(|item| pattern.is_none_or(|pattern| !std::ptr::eq(*item, pattern)))
                .for_each(|item| strings_in(source, regex, item, found));
            return;
        }
        Datum::Vector(items) | Datum::Map(items) | Datum::Set(items) | Datum::Record(_, items) => {
            items
        }
        Datum::Str(_) => {
            let Some(text) = literal(source, form) else {
                return;
            };
            let escapes = text.matches("{{").count();
            if escapes >= 2 && !text.replace("\\\"", "").contains('\\') {
                found.push(Diagnostic::warning(
                    "braces",
                    format!(
                        "this string escapes braces {escapes} times; a raw string, #r\"…\", needs no escapes"
                    ),
                    form.span,
                ));
            }
            return;
        }
        _ => return,
    };
    for item in items {
        strings_in(source, regex, item, found);
    }
}

pub(crate) fn strings(source: &str, imports: &[Import], forms: &[&Syntax]) -> Vec<Diagnostic> {
    let regex: Vec<&Import> = imports
        .iter()
        .filter(|import| import.module == "ox regex")
        .collect();
    let mut found = Vec::new();
    for form in forms {
        strings_in(source, &regex, form, &mut found);
    }
    found
}

impl Unknown {
    pub(crate) fn add(&mut self, spec: &Syntax) {
        let Some(import) = Import::of(spec) else {
            return;
        };
        self.names.extend(import.only);
        self.aliases.extend(import.alias);
    }

    fn covers(&self, name: &str) -> bool {
        self.names.contains(name)
            || name
                .split_once('/')
                .is_some_and(|(alias, _)| self.aliases.contains(alias))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Shape {
    required: usize,
    optional: usize,
    rest: bool,
    keywords: Vec<String>,
}

fn keyword(syntax: &Syntax) -> Option<&str> {
    match &syntax.datum {
        Datum::Keyword(word) => Some(word),
        _ => None,
    }
}

impl Shape {
    fn of_parameters(items: &[Syntax]) -> Option<Self> {
        let parameters = parse_parameters(items).ok()?;
        Some(Self {
            required: parameters.required.len(),
            optional: parameters.optional.len(),
            rest: parameters.rest.is_some(),
            keywords: parameters
                .keywords
                .iter()
                .filter_map(|(name, _)| symbol(name).map(str::to_owned))
                .collect(),
        })
    }

    fn of_value(runtime: &Runtime, value: Value) -> Option<Self> {
        let heap = &runtime.heap;
        let callee = heap
            .record_type_of(value)
            .map_or(value, |kind| kind.constructor);
        if let Some(native) = heap.native_of(callee) {
            let pairs = 2 * native.keywords.len();
            return Some(Self {
                required: native.least,
                optional: native
                    .most
                    .map_or(0, |most| most.saturating_sub(native.least + pairs)),
                rest: native.most.is_none(),
                keywords: native
                    .keywords
                    .iter()
                    .map(|name| name.name().to_owned())
                    .collect(),
            });
        }
        let arity = &heap.closure_of(callee)?.code.arity;
        Some(Self {
            required: usize::from(arity.required),
            optional: usize::from(arity.optional),
            rest: arity.rest,
            keywords: arity
                .keywords
                .iter()
                .map(|name| name.name().to_owned())
                .collect(),
        })
    }

    fn wanted(&self) -> String {
        if self.rest {
            format!("at least {}", arguments(self.required))
        } else if self.optional > 0 {
            format!(
                "{} to {} arguments",
                self.required,
                self.required + self.optional
            )
        } else {
            arguments(self.required)
        }
    }

    fn knows(&self, word: &str) -> bool {
        self.keywords.iter().any(|known| known == word)
    }

    fn problem(&self, name: &str, given: &[Syntax]) -> Option<String> {
        let count = given.len();
        let wrong_count = || format!("{name} takes {}, got {count}", self.wanted());
        if count < self.required {
            return Some(wrong_count());
        }
        let named = (self.required..count)
            .find(|index| keyword(&given[*index]).is_some_and(|word| self.knows(word)))
            .unwrap_or(count);
        if named - self.required > self.optional && !self.rest {
            let first = &given[self.required + self.optional];
            return Some(match keyword(first) {
                Some(word) if !self.keywords.is_empty() => {
                    format!("{name} has no keyword parameter :{word}")
                }
                _ => wrong_count(),
            });
        }
        for pair in given[named..].chunks(2) {
            match (pair, keyword(&pair[0])) {
                ([_, _], Some(word)) if !self.knows(word) => {
                    return Some(format!("{name} has no keyword parameter :{word}"));
                }
                ([_], Some(word)) => {
                    return Some(format!("{name} was given :{word} without a value"));
                }
                ([_, _], Some(_)) => {}
                _ => return None,
            }
        }
        None
    }
}

pub(crate) fn defines_macro(form: &Syntax) -> bool {
    matches!(&form.datum, Datum::List(items) if items.first().and_then(symbol) == Some("defmacro"))
}

pub(crate) fn names(
    runtime: &Runtime,
    module: ModuleId,
    source: &str,
    forms: &[Syntax],
    unknown: &Unknown,
) -> (Vec<Diagnostic>, Vec<Occurrence>) {
    let mut walker = Walker {
        runtime,
        module,
        source,
        unknown,
        defined: HashSet::new(),
        shapes: HashMap::new(),
        locals: Vec::new(),
        found: Vec::new(),
        occurrences: Vec::new(),
    };
    for form in forms {
        walker.collect(form);
    }
    for form in forms {
        walker.form(form);
    }
    let mut occurrences = walker.occurrences;
    occurrences.sort_by_key(|occurrence| (occurrence.span.start, occurrence.span.end));
    occurrences.dedup();
    (walker.found, occurrences)
}

fn lambda_shape(value: &[Syntax]) -> Option<Shape> {
    if value.first().and_then(symbol) != Some("fn") {
        return None;
    }
    let parameters = value.iter().skip(1).find_map(|item| match &item.datum {
        Datum::Vector(parameters) => Some(parameters),
        _ => None,
    })?;
    Shape::of_parameters(parameters)
}

struct Local {
    name: Sym,
    marks: Marks,
    span: Span,
    used: bool,
    quiet: bool,
}

struct Walker<'a> {
    runtime: &'a Runtime,
    module: ModuleId,
    source: &'a str,
    unknown: &'a Unknown,
    defined: HashSet<Sym>,
    shapes: HashMap<Sym, Shape>,
    locals: Vec<Local>,
    found: Vec<Diagnostic>,
    occurrences: Vec<Occurrence>,
}

impl Walker<'_> {
    fn note(&mut self, syntax: &Syntax, binding: Binding, defines: bool) {
        let Datum::Symbol(name) = &syntax.datum else {
            return;
        };
        let written = self
            .source
            .get(syntax.span.start as usize..syntax.span.end as usize);
        if syntax.marks == Marks::default() && written == Some(name.as_str()) {
            self.occurrences.push(Occurrence {
                span: syntax.span,
                binding,
                defines,
            });
        }
    }

    fn global(&self, sym: Sym) -> Option<Binding> {
        if self.defined.contains(&sym) {
            return Some(Binding::Global {
                module: None,
                name: sym.name().to_owned(),
            });
        }
        let (_, target, short) = self
            .runtime
            .resolve(self.module, sym, false)
            .or_else(|| self.runtime.resolve(self.module, sym, true))?;
        let module = (target != self.module)
            .then(|| {
                self.runtime
                    .modules
                    .get(target)
                    .map(|found| found.name.clone())
            })
            .flatten();
        Some(Binding::Global {
            module,
            name: short.name().to_owned(),
        })
    }

    fn collect(&mut self, form: &Syntax) {
        let Datum::List(items) = &form.datum else {
            return;
        };
        match items.first().and_then(symbol) {
            Some("do") => {
                for item in &items[1..] {
                    self.collect(item);
                }
            }
            Some(head @ ("def" | "defonce" | "defrecord" | "deferror" | "defmacro")) => {
                let (meta, parts) = split_meta(&items[1..]);
                let (target, shape) = match parts.first() {
                    Some(
                        target @ Syntax {
                            datum: Datum::Symbol(_),
                            ..
                        },
                    ) => {
                        let shape = match (head, parts.get(1).map(|item| &item.datum)) {
                            ("defrecord", Some(Datum::Vector(fields))) => {
                                Shape::of_parameters(fields)
                            }
                            ("def", Some(Datum::List(value))) => lambda_shape(value),
                            _ => None,
                        };
                        (Some(target), shape)
                    }
                    Some(Syntax {
                        datum: Datum::List(signature),
                        ..
                    }) => (
                        signature.first(),
                        Shape::of_parameters(signature.get(1..).unwrap_or_default()),
                    ),
                    _ => (None, None),
                };
                let Some((target, name)) =
                    target.and_then(|target| Some((target, symbol(target)?)))
                else {
                    return;
                };
                let name = Sym::intern(name);
                self.defined.insert(name);
                let binding = Binding::Global {
                    module: None,
                    name: name.name().to_owned(),
                };
                self.note(target, binding, true);
                match shape.filter(|_| head != "defmacro") {
                    Some(shape) => self.shapes.insert(name, shape),
                    None => self.shapes.remove(&name),
                };
                if !meta_flag(meta, "shadow") && self.shadows(name) {
                    let name = name.name();
                    self.found.push(Diagnostic::warning(
                        "shadow",
                        format!(
                            "{name} shadows the built-in {name}; mark it ^:shadow, or leave the built-in out with (import (ox :exclude [{name}]))"
                        ),
                        target.span,
                    ));
                }
            }
            _ => {}
        }
    }

    fn shadows(&self, name: Sym) -> bool {
        let Some(prelude) = self.runtime.modules.get(PRELUDE) else {
            return false;
        };
        let excluded = self.runtime.modules.get(self.module).is_some_and(|module| {
            module
                .imports
                .iter()
                .any(|import| import.module == PRELUDE && import.exclude.contains(&name))
        });
        self.module != PRELUDE
            && !excluded
            && (prelude.globals.contains_key(&name) || prelude.macros.contains_key(&name))
    }
    fn bind(&mut self, name: &Syntax) {
        self.bind_as(name, false);
    }

    fn bind_as(&mut self, name: &Syntax, quiet: bool) {
        self.note(name, Binding::Local(name.span), true);
        if let Some(text) = symbol(name) {
            self.locals.push(Local {
                name: Sym::intern(text),
                marks: name.marks.clone(),
                span: name.span,
                used: false,
                quiet,
            });
        }
    }

    fn all(&mut self, items: &[Syntax]) {
        for item in items {
            self.form(item);
        }
    }

    fn form(&mut self, form: &Syntax) {
        match &form.datum {
            Datum::Symbol(name) => self.reference(form, name),
            Datum::List(items) => self.list(items),
            Datum::Vector(items)
            | Datum::Map(items)
            | Datum::Set(items)
            | Datum::Record(_, items) => {
                self.all(items);
            }
            _ => {}
        }
    }

    fn scoped(&mut self, walk: impl FnOnce(&mut Self)) {
        let saved = self.locals.len();
        walk(self);
        let ended: Vec<Local> = self.locals.drain(saved..).collect();
        for local in ended {
            let name = local.name.name();
            if !local.used
                && !local.quiet
                && local.marks == Marks::default()
                && !name.starts_with('_')
            {
                self.found.push(Diagnostic::warning(
                    "unused",
                    format!("{name} is never used; call it _{name} if that's on purpose"),
                    local.span,
                ));
            }
        }
    }

    fn list(&mut self, items: &[Syntax]) {
        let Some((head, rest)) = items.split_first() else {
            return;
        };
        let rest = match symbol(head) {
            Some("def" | "defonce" | "defmacro") => split_meta(rest).1,
            _ => rest,
        };
        match symbol(head) {
            Some("quote" | "defrecord" | "module") => {}
            Some("def") => self.definition(rest),
            Some("defonce") => self.all(rest.get(1..).unwrap_or_default()),
            Some("fn") => self.function(rest),
            Some("let") => self.binding(rest),
            Some(name @ ("loop" | "again")) if !self.bound(Sym::intern(name), &head.marks) => {
                if name == "loop" {
                    self.binding(rest);
                } else {
                    self.all(rest);
                }
            }
            Some(".") => {
                if let Some(object) = rest.first() {
                    self.form(object);
                }
            }
            Some("defmacro") => self.scoped(|walker| {
                if let Some(Datum::Vector(parameters)) = rest.get(1).map(|item| &item.datum) {
                    walker.parameters(parameters);
                }
                walker.all(rest.get(2..).unwrap_or_default());
            }),
            Some("match") => self.match_form(rest),
            Some("try") => self.try_form(rest),
            Some("deferror") => self.scoped(|walker| {
                if let Some(Datum::Vector(fields)) = rest.get(1).map(|item| &item.datum) {
                    for field in fields {
                        walker.bind_as(field, true);
                    }
                }
                walker.all(rest.get(2..).unwrap_or_default());
            }),
            Some("if" | "do" | "set!") => self.all(rest),
            _ => {
                self.call(head, rest);
                self.all(items);
            }
        }
    }

    fn call(&mut self, head: &Syntax, given: &[Syntax]) {
        let Some(name) = symbol(head) else {
            return;
        };
        let sym = Sym::intern(name);
        if self.bound(sym, &head.marks) {
            return;
        }
        let shape = if self.defined.contains(&sym) {
            self.shapes.get(&sym).cloned()
        } else {
            self.runtime
                .lookup(self.module, sym)
                .and_then(|value| Shape::of_value(self.runtime, value))
        };
        if let Some(problem) = shape.and_then(|shape| shape.problem(modules::display(sym), given)) {
            self.found
                .push(Diagnostic::error("arity", problem, head.span));
        }
    }

    fn parameters(&mut self, items: &[Syntax]) {
        for item in items {
            match &item.datum {
                Datum::Symbol(name) if name == "&" => {}
                Datum::Symbol(_) => self.bind(item),
                Datum::List(parts) => {
                    if let Some(default) = parts.get(1) {
                        self.form(default);
                    }
                    if let Some(name) = parts.first() {
                        self.bind(name);
                    }
                }
                Datum::Vector(_) | Datum::Record(..) => {
                    for binder in patterns::names(item) {
                        self.bind(binder);
                    }
                }
                Datum::Map(pairs) => {
                    for pair in pairs.chunks(2) {
                        if let [name, default] = pair {
                            self.form(default);
                            self.bind(name);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn definition(&mut self, rest: &[Syntax]) {
        match rest.split_first() {
            Some((
                Syntax {
                    datum: Datum::List(signature),
                    ..
                },
                body,
            )) => self.scoped(|walker| {
                walker.parameters(signature.get(1..).unwrap_or_default());
                walker.all(body);
            }),
            Some((_, values)) => self.all(values),
            None => {}
        }
    }

    fn function(&mut self, rest: &[Syntax]) {
        self.scoped(|walker| {
            let mut index = 0;
            if let Some(name) = rest.first().filter(|name| symbol(name).is_some()) {
                walker.bind_as(name, true);
                index = 1;
            }
            if let Some(Datum::Vector(parameters)) = rest.get(index).map(|item| &item.datum) {
                walker.parameters(parameters);
            }
            walker.all(rest.get(index + 1..).unwrap_or_default());
        });
    }

    fn binding(&mut self, rest: &[Syntax]) {
        let Some(Datum::Vector(bindings)) = rest.first().map(|item| &item.datum) else {
            self.all(rest);
            return;
        };
        self.scoped(|walker| {
            for pair in bindings.chunks(2) {
                if let [pattern, value] = pair {
                    walker.form(value);
                    for binder in patterns::names(pattern) {
                        walker.bind(binder);
                    }
                }
            }
            walker.all(&rest[1..]);
        });
    }

    fn match_form(&mut self, rest: &[Syntax]) {
        let Some((scrutinee, clauses)) = rest.split_first() else {
            return;
        };
        self.form(scrutinee);
        for clause in clauses {
            let Datum::Vector(parts) = &clause.datum else {
                self.form(clause);
                continue;
            };
            self.scoped(|walker| {
                if let Some((pattern, body)) = parts.split_first() {
                    for binder in patterns::names(pattern) {
                        walker.bind(binder);
                    }
                    walker.all(body);
                }
            });
        }
    }

    fn try_form(&mut self, rest: &[Syntax]) {
        for item in rest {
            let Datum::List(parts) = &item.datum else {
                self.form(item);
                continue;
            };
            match parts.first().and_then(symbol) {
                Some("catch") if parts.len() >= 3 => {
                    self.form(&parts[1]);
                    self.scoped(|walker| {
                        walker.bind(&parts[2]);
                        walker.all(&parts[3..]);
                    });
                }
                Some("finally") => self.all(&parts[1..]),
                _ => self.form(item),
            }
        }
    }

    fn bound(&self, name: Sym, marks: &Marks) -> bool {
        self.locals
            .iter()
            .any(|local| local.name == name && local.marks == *marks)
    }

    fn use_local(&mut self, name: Sym, marks: &Marks) -> Option<Span> {
        let local = self
            .locals
            .iter_mut()
            .rev()
            .find(|local| local.name == name && local.marks == *marks)?;
        local.used = true;
        Some(local.span)
    }

    fn reference(&mut self, form: &Syntax, name: &str) {
        if name == "&" {
            return;
        }
        let sym = Sym::intern(name);
        if let Some(binder) = self.use_local(sym, &form.marks) {
            self.note(form, Binding::Local(binder), false);
            return;
        }
        if let Some(binding) = self.global(sym) {
            self.note(form, binding, false);
            return;
        }
        let unresolved = Binding::Global {
            module: None,
            name: name.to_owned(),
        };
        self.note(form, unresolved, false);
        let shown = modules::display(sym);
        if self.unknown.covers(shown) {
            return;
        }
        let message = match self.suggestion(shown) {
            Some(close) => format!("{shown} is not defined; did you mean {close}?"),
            None => format!("{shown} is not defined"),
        };
        self.found
            .push(Diagnostic::error("undefined", message, form.span));
    }

    fn suggestion(&self, name: &str) -> Option<String> {
        if name.contains('/') {
            return None;
        }
        let limit = (name.chars().count() / 3).max(1);
        let visible = [PRELUDE, self.module]
            .into_iter()
            .filter_map(|module| self.runtime.modules.get(module))
            .flat_map(|module| module.globals.keys().copied());
        let mut candidates: Vec<String> = self
            .locals
            .iter()
            .map(|local| local.name)
            .chain(self.defined.iter().copied())
            .chain(visible)
            .map(|candidate| candidate.name().to_owned())
            .filter(|candidate| !candidate.starts_with('#'))
            .collect();
        candidates.sort();
        candidates.dedup();
        candidates
            .into_iter()
            .map(|candidate| (distance(name, &candidate), candidate))
            .filter(|(gap, _)| *gap <= limit)
            .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)))
            .map(|(_, candidate)| candidate)
    }
}
