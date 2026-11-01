use crate::Isolate;
use crate::check::{Binding, Occurrence};
use crate::read::{self, Datum, Syntax};
use crate::span::{Span, line_column};
use crate::syntax::{Kind, Token, legacy_tokens};

const PASSES: usize = 64;

struct Moved {
    module: &'static str,
    name: &'static str,
    to_module: &'static str,
    to_name: &'static str,
}

const MOVED: &[Moved] = &[
    Moved {
        module: "ox/http",
        name: "encode",
        to_module: "ox/string",
        to_name: "url-encode",
    },
    Moved {
        module: "ox/xml",
        name: "parse",
        to_module: "ox/xml",
        to_name: "read",
    },
    Moved {
        module: "ox/string",
        name: "length",
        to_module: "ox",
        to_name: "count",
    },
];

const TO_STRING: [(&str, &str); 3] = [
    ("replace", "replace"),
    ("split", "split"),
    ("matches?", "includes?"),
];

const REGEX_CHARACTERS: &[char] = &['\\', '^', '$', '(', '[', '*', '+', '?', '{', '|'];

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
                    (Some(clause), _) => insertion(clause.end as usize - 1, format!(" {spec}")),
                    (None, Some(form)) => {
                        insertion(form.end as usize - 1, format!(" (import {spec})"))
                    }
                    (None, None) => insertion(0, format!("(module (import {spec}))\n")),
                }
            })
            .collect()
    }
}

fn insertion(at: usize, text: String) -> Rewrite {
    Rewrite {
        span: Span::new(at, at),
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

struct Opened {
    start: usize,
    holes: bool,
    escapes: Vec<Token>,
}

enum Frame {
    String(Opened),
    Hole,
}

fn longest_quotes(text: &str) -> usize {
    text.split(|character| character != '"')
        .map(str::len)
        .max()
        .unwrap_or(0)
}

fn escaped(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(character),
        }
    }
    out.push('"');
    out
}

fn verbatim(text: &str) -> String {
    let quotes = longest_quotes(text);
    if text.contains('\n') {
        let fence = "\"".repeat(quotes.max(2) + 1);
        return format!("{fence}\n{text}\n{fence}");
    }
    if text.is_empty() || text.starts_with('"') || text.ends_with('"') || quotes >= 3 {
        return escaped(text);
    }
    format!("\"\"\"{text}\"\"\"")
}

fn raw_text(token: &str) -> &str {
    let hashes = token[2..].len() - token[2..].trim_start_matches('#').len();
    token
        .get(3 + hashes..token.len() - 1 - hashes)
        .unwrap_or_default()
}

pub fn migrate_strings(source: &str) -> Result<String, String> {
    let mut edits: Vec<Rewrite> = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    for token in legacy_tokens(source) {
        let text = token.text(source);
        match token.kind {
            Kind::StringStart => frames.push(Frame::String(Opened {
                start: token.starts(),
                holes: false,
                escapes: Vec::new(),
            })),
            Kind::Escape if text == "{{" || text == "}}" => {
                if let Some(Frame::String(opened)) = frames.last_mut() {
                    opened.escapes.push(token);
                }
            }
            Kind::HoleStart => {
                if let Some(Frame::String(opened)) = frames.last_mut() {
                    opened.holes = true;
                }
                frames.push(Frame::Hole);
            }
            Kind::HoleEnd => {
                frames.pop();
            }
            Kind::StringEnd => {
                if let Some(Frame::String(opened)) = frames.pop() {
                    if opened.holes {
                        edits.push(insertion(opened.start, "$".to_owned()));
                    } else {
                        edits.extend(opened.escapes.iter().map(|escape| Rewrite {
                            span: escape.span,
                            text: escape.text(source)[..1].to_owned(),
                        }));
                    }
                }
            }
            Kind::Raw { closed: true } => edits.push(Rewrite {
                span: token.span,
                text: verbatim(raw_text(text)),
            }),
            _ => {}
        }
    }
    edits.sort_by_key(|edit| edit.span.start);
    let mut out = source.to_owned();
    for edit in edits.iter().rev() {
        out.replace_range(edit.span.start as usize..edit.span.end as usize, &edit.text);
    }
    match read::read(&out).1.first() {
        Some(error) => {
            let (line, column) = line_column(&out, error.span.start);
            Err(format!(
                "{line}:{column}: {} after rewriting its strings",
                error.message
            ))
        }
        None => Ok(out),
    }
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
        let waiting: Vec<Span> = occurrences
            .iter()
            .filter(|occurrence| match &occurrence.binding {
                Binding::Global {
                    module: Some(module),
                    name,
                } => isolate.suspends(module, name),
                _ => false,
            })
            .map(|occurrence| occurrence.span)
            .collect();
        let known = Known {
            occurrences: &occurrences,
            waiting: &waiting,
            patterns: patterns(&forms),
        };
        let mut imports = Imports::of(&forms);
        let mut rewrites = Vec::new();
        for form in &forms {
            collect(&text, &known, &mut imports, form, &mut rewrites);
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

struct Known<'a> {
    occurrences: &'a [Occurrence],
    waiting: &'a [Span],
    patterns: Vec<(String, bool)>,
}

impl Known<'_> {
    fn awaits(&self, head: &Syntax) -> bool {
        self.occurrences.iter().any(|occurrence| {
            occurrence.span == head.span
                && matches!(&occurrence.binding, Binding::Global { module: Some(module), name } if module == "ox" && name == "await")
        })
    }

    fn is_cond(&self, head: &Syntax) -> bool {
        if !matches!(&head.datum, Datum::Symbol(name) if name == "cond") {
            return false;
        }
        let found = self
            .occurrences
            .iter()
            .find(|occurrence| occurrence.span == head.span);
        match found {
            Some(occurrence) => {
                matches!(&occurrence.binding, Binding::Global { module: Some(module), name } if module == "ox" && name == "cond")
            }
            None => !self
                .occurrences
                .iter()
                .any(|occurrence| occurrence.defines && unbound(occurrence, "cond")),
        }
    }

    fn bracketed(&self, form: &Syntax) -> Vec<Rewrite> {
        let Datum::List(items) = &form.datum else {
            return Vec::new();
        };
        let Some((head, clauses)) = items.split_first() else {
            return Vec::new();
        };
        if !self.is_cond(head) {
            return Vec::new();
        }
        clauses
            .iter()
            .filter(|clause| matches!(clause.datum, Datum::List(_)))
            .flat_map(|clause| {
                let start = clause.span.start as usize;
                let end = clause.span.end as usize;
                [
                    Rewrite {
                        span: Span::new(start, start + 1),
                        text: "[".to_owned(),
                    },
                    Rewrite {
                        span: Span::new(end - 1, end),
                        text: "]".to_owned(),
                    },
                ]
            })
            .collect()
    }

    fn redundant(&self, form: &Syntax) -> Option<Span> {
        let Datum::List(items) = &form.datum else {
            return None;
        };
        let [head, call] = items.as_slice() else {
            return None;
        };
        let Datum::List(parts) = &call.datum else {
            return None;
        };
        let callee = parts.first()?;
        (self.awaits(head) && self.waiting.contains(&callee.span)).then_some(call.span)
    }
}

fn collect(
    text: &str,
    known: &Known<'_>,
    imports: &mut Imports,
    form: &Syntax,
    rewrites: &mut Vec<Rewrite>,
) {
    if let Some(call) = known.redundant(form) {
        rewrites.push(Rewrite {
            span: form.span,
            text: source(text, call.start, call.end).to_owned(),
        });
        return;
    }
    let brackets = known.bracketed(form);
    if !brackets.is_empty() {
        rewrites.extend(brackets);
        return;
    }
    if let Datum::List(items) = &form.datum
        && let Some((head, arguments)) = items.split_first()
        && let Datum::Symbol(name) = &head.datum
        && removed(known.occurrences, head, name)
        && let Some(rewrite) = rewritten(text, name, form, arguments)
    {
        rewrites.push(rewrite);
        return;
    }
    if let Some(rewrite) = reordered(text, known, imports, form) {
        rewrites.push(rewrite);
        return;
    }
    if let Datum::Symbol(name) = &form.datum
        && removed(known.occurrences, form, name)
        && let Some(rewrite) = moved(imports, form, name)
    {
        rewrites.push(rewrite);
        return;
    }
    for child in children(form) {
        collect(text, known, imports, child, rewrites);
    }
}

fn moved(imports: &mut Imports, symbol: &Syntax, name: &str) -> Option<Rewrite> {
    let (alias, member) = name.split_once('/')?;
    let module = imports.module_of(alias)?;
    let found = MOVED
        .iter()
        .find(|moved| moved.module == module && moved.name == member)?;
    let text = if found.to_module == "ox" {
        found.to_name.to_owned()
    } else {
        format!("{}/{}", imports.alias_of(found.to_module), found.to_name)
    };
    Some(Rewrite {
        span: symbol.span,
        text,
    })
}

fn regex_form(syntax: &Syntax) -> bool {
    head(syntax) == Some("regex")
}

fn regex_text(syntax: &Syntax) -> bool {
    match &syntax.datum {
        Datum::Str(text) => text.contains(REGEX_CHARACTERS),
        Datum::List(items) if head(syntax) == Some("str") => items.iter().skip(1).any(regex_text),
        _ => false,
    }
}

fn patterns(forms: &[Syntax]) -> Vec<(String, bool)> {
    forms
        .iter()
        .filter(|form| head(form) == Some("def"))
        .filter_map(|form| match children(form) {
            [_, name, value] if regex_form(value) || regex_text(value) => match &name.datum {
                Datum::Symbol(name) => Some((name.clone(), regex_form(value))),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn defined_pattern(known: &Known<'_>, syntax: &Syntax) -> Option<bool> {
    let Datum::Symbol(name) = &syntax.datum else {
        return None;
    };
    let local = known.occurrences.iter().any(|occurrence| {
        occurrence.span == syntax.span && matches!(occurrence.binding, Binding::Local(_))
    });
    if local {
        return None;
    }
    known
        .patterns
        .iter()
        .find(|(known, _)| known == name)
        .map(|(_, regex)| *regex)
}

fn pattern_like(known: &Known<'_>, syntax: &Syntax) -> bool {
    match &syntax.datum {
        Datum::Symbol(_) => defined_pattern(known, syntax).is_some(),
        _ => regex_form(syntax) || regex_text(syntax),
    }
}

fn regex_call<'s>(known: &Known<'_>, imports: &Imports, head: &'s Syntax) -> Option<&'s str> {
    let Datum::Symbol(name) = &head.datum else {
        return None;
    };
    let (alias, member) = name.split_once('/')?;
    let local = known.occurrences.iter().any(|occurrence| {
        occurrence.span == head.span && matches!(occurrence.binding, Binding::Local(_))
    });
    (imports.module_of(alias) == Some("ox/regex") && !local).then_some(member)
}

fn reordered(
    text: &str,
    known: &Known<'_>,
    imports: &mut Imports,
    form: &Syntax,
) -> Option<Rewrite> {
    let (head, arguments) = children(form).split_first()?;
    let member = regex_call(known, imports, head)?;
    let written = |syntax: &Syntax| source(text, syntax.span.start, syntax.span.end).to_owned();
    if let Some((_, to)) = TO_STRING.iter().find(|(from, _)| *from == member) {
        let [pattern, whole, rest @ ..] = arguments else {
            return None;
        };
        let pattern = if regex_form(pattern) || defined_pattern(known, pattern) == Some(true) {
            written(pattern)
        } else {
            format!("(regex {})", written(pattern))
        };
        let mut parts = vec![
            format!("{}/{to}", imports.alias_of("ox/string")),
            written(whole),
            pattern,
        ];
        parts.extend(rest.iter().map(written));
        return Some(Rewrite {
            span: form.span,
            text: format!("({})", parts.join(" ")),
        });
    }
    match (member, arguments) {
        ("find" | "find-all", [pattern, whole])
            if pattern_like(known, pattern) && !pattern_like(known, whole) =>
        {
            Some(Rewrite {
                span: Span::new(pattern.span.start as usize, whole.span.end as usize),
                text: format!(
                    "{}{}{}",
                    written(whole),
                    source(text, pattern.span.end, whole.span.start),
                    written(pattern)
                ),
            })
        }
        _ => None,
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

fn rewritten(text: &str, name: &str, form: &Syntax, arguments: &[Syntax]) -> Option<Rewrite> {
    let head = children(form).first()?;
    match (name, arguments) {
        ("with", _) => Some(Rewrite {
            span: head.span,
            text: "assoc".to_owned(),
        }),
        ("buffer-exists?", _) => Some(Rewrite {
            span: head.span,
            text: "buffer-live?".to_owned(),
        }),
        ("buffer-contents", [buffer]) => Some(Rewrite {
            span: form.span,
            text: format!(
                "(with-buffer {} (buffer-text))",
                source(text, buffer.span.start, buffer.span.end)
            ),
        }),
        ("defrule", _) => defrule(text, head, arguments),
        ("case", _) => case(text, head, arguments),
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
