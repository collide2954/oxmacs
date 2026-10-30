use crate::Isolate;
use crate::check::{Binding, Occurrence};
use crate::read::{self, Datum, Syntax};
use crate::span::{Span, line_column};

const PASSES: usize = 64;

struct Moved {
    module: &'static str,
    name: &'static str,
    to_module: &'static str,
    to_name: &'static str,
}

const MOVED: &[Moved] = &[Moved {
    module: "ox/http",
    name: "encode",
    to_module: "ox/string",
    to_name: "url-encode",
}];

struct Rewrite {
    span: Span,
    text: String,
}

struct Import {
    module: String,
    alias: String,
}

#[derive(Default)]
struct Imports {
    known: Vec<Import>,
    clause: Option<Span>,
    module: Option<Span>,
    wanted: Vec<&'static str>,
}

impl Imports {
    fn of(forms: &[Syntax]) -> Self {
        let mut imports = Self::default();
        let Some(module) = forms.iter().find(|form| head(form) == Some("module")) else {
            return imports;
        };
        imports.module = Some(module.span);
        for clause in children(module)
            .iter()
            .filter(|item| head(item) == Some("import"))
        {
            imports.clause = Some(clause.span);
            imports
                .known
                .extend(children(clause).iter().skip(1).filter_map(import));
        }
        imports
    }

    fn module_of(&self, alias: &str) -> Option<&str> {
        self.known
            .iter()
            .find(|import| import.alias == alias)
            .map(|import| import.module.as_str())
    }

    fn alias_of(&mut self, module: &'static str) -> String {
        if let Some(import) = self.known.iter().find(|import| import.module == module) {
            return import.alias.clone();
        }
        if !self.wanted.contains(&module) {
            self.wanted.push(module);
        }
        module.rsplit('/').next().unwrap_or(module).to_owned()
    }

    fn additions(&self) -> Vec<Rewrite> {
        self.wanted
            .iter()
            .map(|module| {
                let spec = format!("({})", module.replace('/', " "));
                match (self.clause, self.module) {
                    (Some(clause), _) => insertion(clause.end - 1, format!(" {spec}")),
                    (None, Some(form)) => insertion(form.end - 1, format!(" (import {spec})")),
                    (None, None) => insertion(0, format!("(module (import {spec}))\n")),
                }
            })
            .collect()
    }
}

fn insertion(at: u32, text: String) -> Rewrite {
    Rewrite {
        span: Span::new(at as usize, at as usize),
        text,
    }
}

fn head(form: &Syntax) -> Option<&str> {
    match &form.datum {
        Datum::List(items) => match &items.first()?.datum {
            Datum::Symbol(name) => Some(name),
            _ => None,
        },
        _ => None,
    }
}

fn import(spec: &Syntax) -> Option<Import> {
    let Datum::List(items) = &spec.datum else {
        return None;
    };
    let segments: Vec<&str> = items
        .iter()
        .map_while(|item| match &item.datum {
            Datum::Symbol(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let alias = items
        .windows(2)
        .find_map(|pair| match (&pair[0].datum, &pair[1].datum) {
            (Datum::Keyword(key), Datum::Symbol(alias)) if key == "as" => Some(alias.clone()),
            _ => None,
        })
        .or_else(|| segments.last().map(|last| (*last).to_owned()))?;
    Some(Import {
        module: segments.join("/"),
        alias,
    })
}

pub fn migrate(isolate: &mut Isolate, name: &str, source: &str) -> Result<String, String> {
    let mut text = source.to_owned();
    for _ in 0..PASSES {
        let (forms, errors) = read::read(&text);
        if let Some(error) = errors.first() {
            let (line, column) = line_column(&text, error.span.start);
            return Err(format!("{line}:{column}: {}", error.message));
        }
        let occurrences = isolate.bindings(name, &text);
        let mut imports = Imports::of(&forms);
        let mut rewrites = Vec::new();
        for form in &forms {
            collect(&text, &occurrences, &mut imports, form, &mut rewrites);
        }
        rewrites.extend(imports.additions());
        if rewrites.is_empty() {
            break;
        }
        rewrites.sort_by_key(|rewrite| rewrite.span.start);
        for rewrite in rewrites.iter().rev() {
            text.replace_range(
                rewrite.span.start as usize..rewrite.span.end as usize,
                &rewrite.text,
            );
        }
    }
    Ok(text)
}

fn children(form: &Syntax) -> &[Syntax] {
    match &form.datum {
        Datum::List(items)
        | Datum::Vector(items)
        | Datum::Map(items)
        | Datum::Set(items)
        | Datum::Record(_, items)
        | Datum::Meta(items) => items,
        _ => &[],
    }
}

fn collect(
    text: &str,
    occurrences: &[Occurrence],
    imports: &mut Imports,
    form: &Syntax,
    rewrites: &mut Vec<Rewrite>,
) {
    if let Datum::List(items) = &form.datum
        && let Some((head, arguments)) = items.split_first()
        && let Datum::Symbol(name) = &head.datum
        && removed(occurrences, head, name)
        && let Some(rewrite) = rewritten(text, name, head, arguments)
    {
        rewrites.push(rewrite);
        return;
    }
    if let Datum::Symbol(name) = &form.datum
        && removed(occurrences, form, name)
        && let Some(rewrite) = moved(imports, form, name)
    {
        rewrites.push(rewrite);
        return;
    }
    for child in children(form) {
        collect(text, occurrences, imports, child, rewrites);
    }
}

fn moved(imports: &mut Imports, symbol: &Syntax, name: &str) -> Option<Rewrite> {
    let (alias, member) = name.split_once('/')?;
    let module = imports.module_of(alias)?;
    let found = MOVED
        .iter()
        .find(|moved| moved.module == module && moved.name == member)?;
    let alias = imports.alias_of(found.to_module);
    Some(Rewrite {
        span: symbol.span,
        text: format!("{alias}/{}", found.to_name),
    })
}

fn unbound(occurrence: &Occurrence, name: &str) -> bool {
    matches!(&occurrence.binding, Binding::Global { module: None, name: known } if known == name)
}

fn removed(occurrences: &[Occurrence], head: &Syntax, name: &str) -> bool {
    let defined = occurrences
        .iter()
        .any(|occurrence| occurrence.defines && unbound(occurrence, name));
    !defined
        && occurrences
            .iter()
            .any(|occurrence| occurrence.span == head.span && unbound(occurrence, name))
}

fn source(text: &str, start: u32, end: u32) -> &str {
    &text[start as usize..end as usize]
}

fn rewritten(text: &str, name: &str, head: &Syntax, arguments: &[Syntax]) -> Option<Rewrite> {
    match name {
        "with" => Some(Rewrite {
            span: head.span,
            text: "assoc".to_owned(),
        }),
        "defrule" => defrule(text, head, arguments),
        "case" => case(text, head, arguments),
        _ => None,
    }
}

fn defrule(text: &str, head: &Syntax, arguments: &[Syntax]) -> Option<Rewrite> {
    let [pattern, template] = arguments else {
        return None;
    };
    let Datum::List(parts) = &pattern.datum else {
        return None;
    };
    let Datum::Symbol(name) = &parts.first()?.datum else {
        return None;
    };
    Some(Rewrite {
        span: Span::new(head.span.start as usize, template.span.end as usize),
        text: format!(
            "defrules {name} []{}[{}]",
            source(text, head.span.end, pattern.span.start),
            source(text, pattern.span.start, template.span.end)
        ),
    })
}

fn constant(text: &str, syntax: &Syntax) -> String {
    match &syntax.datum {
        Datum::Symbol(name) => format!("'{name}"),
        _ => source(text, syntax.span.start, syntax.span.end).to_owned(),
    }
}

fn case(text: &str, head: &Syntax, arguments: &[Syntax]) -> Option<Rewrite> {
    let (key, clauses) = arguments.split_first()?;
    let mut migrated = format!("match{}", source(text, head.span.end, key.span.end));
    let mut gap = " ";
    let mut otherwise = false;
    let mut end = key.span.end;
    for clause in clauses {
        let (Datum::Vector(parts) | Datum::List(parts)) = &clause.datum else {
            return None;
        };
        let first = parts.first()?;
        gap = source(text, end, clause.span.start);
        let body = source(text, first.span.end, clause.span.end - 1);
        let patterns = match &first.datum {
            Datum::Symbol(name) if name == "else" => {
                otherwise = true;
                vec!["_".to_owned()]
            }
            Datum::List(options) => options
                .iter()
                .map(|option| constant(text, option))
                .collect(),
            _ => vec![constant(text, first)],
        };
        let written: Vec<String> = patterns
            .iter()
            .map(|pattern| format!("[{pattern}{body}]"))
            .collect();
        migrated.push_str(gap);
        migrated.push_str(&written.join(gap));
        end = clause.span.end;
    }
    if !otherwise {
        migrated.push_str(gap);
        migrated.push_str("[_ nil]");
    }
    Some(Rewrite {
        span: Span::new(head.span.start as usize, end as usize),
        text: migrated,
    })
}
