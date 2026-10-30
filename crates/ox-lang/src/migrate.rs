use crate::Isolate;
use crate::check::{Binding, Occurrence};
use crate::read::{self, Datum, Syntax};
use crate::span::{Span, line_column};

const PASSES: usize = 64;

struct Rewrite {
    span: Span,
    text: String,
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
        let mut rewrites = Vec::new();
        for form in &forms {
            collect(&text, &occurrences, form, &mut rewrites);
        }
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

fn collect(text: &str, occurrences: &[Occurrence], form: &Syntax, rewrites: &mut Vec<Rewrite>) {
    if let Datum::List(items) = &form.datum
        && let Some((head, arguments)) = items.split_first()
        && let Datum::Symbol(name) = &head.datum
        && removed(occurrences, head, name)
        && let Some(rewrite) = rewritten(text, name, head, arguments)
    {
        rewrites.push(rewrite);
        return;
    }
    for child in children(form) {
        collect(text, occurrences, child, rewrites);
    }
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
