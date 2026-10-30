use crate::describe::Kind;
use crate::forms::Forms;
use crate::read::{self, Datum, Syntax, split_meta};
use crate::span::Span;
use crate::syntax::{self, Bracket, State, delimiter, numeric, tokens};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    pub kind: Kind,
    pub signature: String,
    pub doc: Option<String>,
    pub span: Span,
    pub name_span: Span,
}

pub fn short(name: &str) -> &str {
    match name.split_once('/') {
        Some((alias, rest)) if !alias.is_empty() && !rest.is_empty() => rest,
        _ => name,
    }
}

pub fn call_head(before: &str) -> Option<String> {
    let (found, state) = tokens(before, &State::default());
    if !state.in_code() {
        return None;
    }
    let mut open: Vec<usize> = Vec::new();
    for (index, token) in found.iter().enumerate() {
        match token.kind {
            syntax::Kind::Open(_) => open.push(index),
            syntax::Kind::Close => {
                open.pop();
            }
            _ => {}
        }
    }
    let at = *open.last()?;
    if found[at].kind != syntax::Kind::Open(Bracket::List) {
        return None;
    }
    found
        .get(at + 1)
        .filter(|head| head.kind == syntax::Kind::Symbol)
        .map(|head| head.text(before).to_owned())
}

pub fn typed_name(before: &str) -> &str {
    let (found, _) = tokens(before, &State::default());
    found
        .last()
        .filter(|token| token.kind == syntax::Kind::Symbol)
        .map_or("", |token| token.text(before))
}

fn word_in(text: &str, at: usize) -> &str {
    let inside = |character: char| !delimiter(character) && !"'`".contains(character);
    let start = text[..at]
        .char_indices()
        .rev()
        .take_while(|(_, character)| inside(*character))
        .last()
        .map_or(at, |(offset, _)| offset);
    let end = text[at..]
        .char_indices()
        .find(|(_, character)| !inside(*character))
        .map_or(text.len(), |(offset, _)| at + offset);
    text[start..end]
        .trim_start_matches(['~', '@', '#'])
        .trim_end_matches(['.', '!', '?', ':', ','].as_slice())
}

pub fn symbol_at(line: &str, column: usize) -> Option<String> {
    let at = line
        .char_indices()
        .nth(column)
        .map_or(line.len(), |(offset, _)| offset);
    let (found, _) = tokens(line, &State::default());
    let index = found
        .iter()
        .position(|token| token.starts() <= at && at < token.ends())
        .filter(|index| {
            !matches!(
                found[*index].kind,
                syntax::Kind::Space | syntax::Kind::Close
            )
        })
        .or_else(|| found.iter().position(|token| token.ends() == at))?;
    let token = match found[index].kind {
        syntax::Kind::Prefix(_) => found.get(index + 1)?,
        _ => &found[index],
    };
    let word = match token.kind {
        syntax::Kind::Symbol => token.text(line),
        syntax::Kind::Text | syntax::Kind::Comment | syntax::Kind::BlockComment { .. } => {
            let offset = at.clamp(token.starts(), token.ends());
            word_in(line, offset)
        }
        _ => return None,
    };
    (!word.is_empty() && !numeric(word) && !word.starts_with(':')).then(|| word.to_owned())
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
    use super::{Definition, call_head, short, symbol_at, typed_name};
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
    fn call_heads_see_through_holes_raw_strings_and_comments() {
        assert_eq!(
            call_head("(msg \"{(if moving \"Moved\" ").as_deref(),
            Some("if")
        );
        assert_eq!(call_head("(f #r#\"(\"# ").as_deref(), Some("f"));
        assert_eq!(call_head("(f #| ( |# ").as_deref(), Some("f"));
        assert_eq!(call_head("(f #\"(\" ").as_deref(), Some("f"));
        assert_eq!(call_head("(f \"{x} open"), None);
    }

    #[test]
    fn the_name_being_typed_is_a_symbol_in_code() {
        assert_eq!(typed_name("(li"), "li");
        assert_eq!(typed_name("(f 'ali"), "ali");
        assert_eq!(typed_name("#{s/up"), "s/up");
        assert_eq!(typed_name("(f "), "");
        assert_eq!(typed_name("(f ; lim"), "");
        assert_eq!(typed_name("(f \"lim"), "");
        assert_eq!(typed_name("(f :ke"), "");
        assert_eq!(typed_name("\"{(str/jo"), "str/jo");
    }

    #[test]
    fn symbols_end_at_commas_and_are_found_in_comments_and_strings() {
        assert_eq!(symbol_at("[a,b]", 1).as_deref(), Some("a"));
        assert_eq!(symbol_at("[a,b]", 3).as_deref(), Some("b"));
        assert_eq!(symbol_at("; see area.", 7).as_deref(), Some("area"));
        assert_eq!(
            symbol_at("\"call `area` first\"", 8).as_deref(),
            Some("area")
        );
        assert_eq!(symbol_at("(area 2)", 5).as_deref(), Some("area"));
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
            "(defwidget dial \"Turns\" 1)\n(defrules swap! [] [(swap! a b) [b a]])\n(other thing)",
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
