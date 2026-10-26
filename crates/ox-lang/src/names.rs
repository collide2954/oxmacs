use crate::describe::Kind;
use crate::forms::Forms;
use crate::read::{self, Datum, Syntax, split_meta};
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    pub kind: Kind,
    pub signature: String,
    pub doc: Option<String>,
    pub span: Span,
    pub name_span: Span,
}

fn delimits(character: char) -> bool {
    character.is_whitespace() || "()[]{}\";".contains(character)
}

fn starts_symbol(character: char) -> bool {
    !delimits(character) && !"'`~@#,\\".contains(character)
}

pub fn short(name: &str) -> &str {
    match name.split_once('/') {
        Some((alias, rest)) if !alias.is_empty() && !rest.is_empty() => rest,
        _ => name,
    }
}

pub fn call_head(before: &str) -> Option<String> {
    let chars: Vec<char> = before.chars().collect();
    let mut open: Vec<(char, usize)> = Vec::new();
    let mut quoted = false;
    let mut at = 0;
    while at < chars.len() {
        let character = chars[at];
        if quoted {
            match character {
                '\\' => at += 1,
                '"' => quoted = false,
                _ => {}
            }
        } else {
            match character {
                '"' => quoted = true,
                ';' => {
                    while at < chars.len() && chars[at] != '\n' {
                        at += 1;
                    }
                }
                '\\' => at += 1,
                '(' | '[' | '{' => open.push((character, at)),
                ')' | ']' | '}' => {
                    open.pop();
                }
                _ => {}
            }
        }
        at += 1;
    }
    if quoted {
        return None;
    }
    let &(bracket, at) = open.last()?;
    if bracket != '(' {
        return None;
    }
    let head: String = chars[at + 1..]
        .iter()
        .take_while(|character| !delimits(**character))
        .collect();
    head.chars()
        .next()
        .is_some_and(starts_symbol)
        .then_some(head)
}

pub fn symbol_at(line: &str, column: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let symbolic = |at: usize| chars.get(at).is_some_and(|character| !delimits(*character));
    let mut start = column.min(chars.len());
    while start > 0 && symbolic(start - 1) {
        start -= 1;
    }
    let mut end = column.min(chars.len());
    while symbolic(end) {
        end += 1;
    }
    let word: String = chars[start..end].iter().collect();
    let word = word.trim_start_matches(['\'', '`', '~', '@', '#']);
    let number = word
        .trim_start_matches(['-', '+'])
        .starts_with(|character: char| character.is_ascii_digit());
    let keyword = word.starts_with(':') || (word.len() > 1 && word.ends_with(':'));
    (!word.is_empty() && !number && !keyword).then(|| word.to_owned())
}

fn walk(syntax: &Syntax, wanted: &str, found: &mut Vec<Span>) {
    match &syntax.datum {
        Datum::Symbol(name) if short(name) == wanted => found.push(syntax.span),
        Datum::List(items)
        | Datum::Vector(items)
        | Datum::Map(items)
        | Datum::Set(items)
        | Datum::Record(_, items) => {
            for item in items {
                walk(item, wanted, found);
            }
        }
        _ => {}
    }
}

pub fn occurrences(text: &str, name: &str) -> Vec<Span> {
    let wanted = short(name);
    let (forms, _) = read::read(text);
    let mut found = Vec::new();
    for form in &forms {
        walk(form, wanted, &mut found);
    }
    found.sort_by_key(|span| span.start);
    found
}

fn symbol(syntax: &Syntax) -> Option<&str> {
    match &syntax.datum {
        Datum::Symbol(name) => Some(name),
        _ => None,
    }
}

fn joined(items: &[Syntax]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

fn signature(name: &str, parameters: &[Syntax]) -> String {
    if parameters.is_empty() {
        format!("({name})")
    } else {
        format!("({name} {})", joined(parameters))
    }
}

fn doc_of(body: &[Syntax]) -> Option<String> {
    match body {
        [
            Syntax {
                datum: Datum::Str(doc),
                ..
            },
            _,
            ..,
        ] => Some(doc.clone()),
        _ => None,
    }
}

fn defined(rest: &[Syntax], at: usize) -> Option<(&Syntax, String, Option<String>)> {
    let target = rest.get(at)?;
    let name_syntax = match &target.datum {
        Datum::List(parts) => parts.first()?,
        _ => target,
    };
    let name = symbol(name_syntax)?;
    let doc = match rest.get(at + 1).map(|item| &item.datum) {
        Some(Datum::Str(text)) => Some(text.clone()),
        _ => None,
    };
    Some((name_syntax, format!("({name})"), doc))
}

fn definition(form: &Syntax, forms: &Forms) -> Option<Definition> {
    let Datum::List(items) = &form.datum else {
        return None;
    };
    let head = symbol(items.first()?)?;
    let rest = split_meta(&items[1..]).1;
    let (name_syntax, kind, signature, doc) = match (head, rest) {
        ("def" | "defonce", [target, body @ ..]) => match &target.datum {
            Datum::List(parts) => {
                let name_syntax = parts.first()?;
                let name = symbol(name_syntax)?;
                (
                    name_syntax,
                    Kind::Function,
                    signature(name, &parts[1..]),
                    doc_of(body),
                )
            }
            Datum::Symbol(name) => (target, Kind::Value, name.clone(), doc_of(body)),
            _ => return None,
        },
        ("defmacro", [target, parameters, body @ ..]) => {
            let name = symbol(target)?;
            let Datum::Vector(parameters) = &parameters.datum else {
                return None;
            };
            (
                target,
                Kind::Macro,
                signature(name, parameters),
                doc_of(body),
            )
        }
        ("defrecord" | "deferror", [target, fields, ..]) => {
            let name = symbol(target)?;
            let Datum::Vector(fields) = &fields.datum else {
                return None;
            };
            (target, Kind::Record, signature(name, fields), None)
        }
        _ => {
            let (name_syntax, signature, doc) = defined(rest, forms.defines(head)?)?;
            (name_syntax, Kind::Function, signature, doc)
        }
    };
    Some(Definition {
        name: symbol(name_syntax)?.to_owned(),
        kind,
        signature,
        doc,
        span: form.span,
        name_span: name_syntax.span,
    })
}

pub fn definitions(text: &str) -> Vec<Definition> {
    definitions_with(text, &Forms::built_in())
}

pub fn definitions_with(text: &str, known: &Forms) -> Vec<Definition> {
    let (forms, _) = read::read(text);
    forms
        .iter()
        .filter_map(|form| definition(form, known))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Definition, call_head, occurrences, short, symbol_at};
    use crate::describe::Kind;

    #[test]
    fn the_innermost_open_call_names_its_head() {
        assert_eq!(call_head("(area 2 ").as_deref(), Some("area"));
        assert_eq!(call_head("(+ (area 2 3) ").as_deref(), Some("+"));
        assert_eq!(call_head("(s/upper \"a(b\" ").as_deref(), Some("s/upper"));
        assert_eq!(call_head("(f \\( ").as_deref(), Some("f"));
        assert_eq!(call_head("(f ; (g\n x").as_deref(), Some("f"));
        assert_eq!(call_head("(f [1 2 "), None);
        assert_eq!(call_head("(f \"open"), None);
        assert_eq!(call_head("('quoted"), None);
    }

    #[test]
    fn the_symbol_at_a_column_skips_quotes_numbers_and_keywords() {
        assert_eq!(symbol_at("(area 2 3)", 3).as_deref(), Some("area"));
        assert_eq!(symbol_at("'foo", 2).as_deref(), Some("foo"));
        assert_eq!(symbol_at("(- 4 1)", 1).as_deref(), Some("-"));
        assert_eq!(symbol_at("(f -12)", 4), None);
        assert_eq!(symbol_at("(f :size)", 4), None);
        assert_eq!(short("s/upper"), "upper");
        assert_eq!(short("/"), "/");
    }

    #[test]
    fn occurrences_count_code_but_not_strings_or_comments() {
        let text = "(def (area w h) (* w h))\n(s/area 1 2) ; area\n\"area\"";

        let found: Vec<u32> = occurrences(text, "area")
            .iter()
            .map(|span| span.start)
            .collect();

        assert_eq!(found, [6, 26]);
    }

    #[test]
    fn macros_that_define_names_are_listed_from_their_forms() {
        let mut forms = crate::forms::Forms::built_in();
        forms.insert(
            "defwidget",
            crate::forms::Form {
                indent: Some(1),
                defines: Some(0),
            },
        );

        let found: Vec<(String, Option<String>)> = super::definitions_with(
            "(defwidget dial \"Turns\" 1)\n(defrule (swap! a b) [b a])\n(other thing)",
            &forms,
        )
        .into_iter()
        .map(|definition| (definition.name, definition.doc))
        .collect();

        assert_eq!(
            found,
            [
                ("dial".to_owned(), Some("Turns".to_owned())),
                ("swap!".to_owned(), None),
            ]
        );
    }

    #[test]
    fn top_level_definitions_are_listed_with_their_signatures() {
        let text = "(def (area w h) \"Width times height.\" (* w h))
(def ^:shadow limit 5)
(defmacro twice [x] `(do ~x ~x))
(defrecord Point [x y])
(defcommand greet \"Say hello\" (message \"hi\"))
(area 1 2)";
        let mut forms = crate::forms::Forms::built_in();
        forms.insert(
            "defcommand",
            crate::forms::Form {
                indent: Some(2),
                defines: Some(0),
            },
        );

        let found: Vec<(String, Kind, String, Option<String>)> =
            super::definitions_with(text, &forms)
                .into_iter()
                .map(
                    |Definition {
                         name,
                         kind,
                         signature,
                         doc,
                         ..
                     }| (name, kind, signature, doc),
                )
                .collect();

        assert_eq!(
            found,
            [
                (
                    "area".to_owned(),
                    Kind::Function,
                    "(area w h)".to_owned(),
                    Some("Width times height.".to_owned())
                ),
                ("limit".to_owned(), Kind::Value, "limit".to_owned(), None),
                (
                    "twice".to_owned(),
                    Kind::Macro,
                    "(twice x)".to_owned(),
                    None
                ),
                (
                    "Point".to_owned(),
                    Kind::Record,
                    "(Point x y)".to_owned(),
                    None
                ),
                (
                    "greet".to_owned(),
                    Kind::Function,
                    "(greet)".to_owned(),
                    Some("Say hello".to_owned())
                ),
            ]
        );
    }
}
