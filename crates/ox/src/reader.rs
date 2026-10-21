use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum Datum {
    Int(i64),
    Float(f64),
    Char(char),
    Str(String),
    Symbol(String),
    Keyword(String),
    List(Vec<Syntax>),
    Vector(Vec<Syntax>),
    Map(Vec<Syntax>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Syntax {
    pub datum: Datum,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    pub message: String,
    pub span: Span,
}

const MAX_DEPTH: usize = 1000;

pub fn read(source: &str) -> (Vec<Syntax>, Vec<ReadError>) {
    let mut reader = Reader {
        source,
        at: 0,
        errors: Vec::new(),
        open: Vec::new(),
        top: Vec::new(),
    };
    reader.read_all();
    (reader.top, reader.errors)
}

struct Open {
    bracket: char,
    start: usize,
    items: Vec<Syntax>,
}

struct Reader<'a> {
    source: &'a str,
    at: usize,
    errors: Vec<ReadError>,
    open: Vec<Open>,
    top: Vec<Syntax>,
}

fn closer(opener: char) -> char {
    match opener {
        '(' => ')',
        '[' => ']',
        _ => '}',
    }
}

fn separator(character: char) -> bool {
    character.is_whitespace() || character == ','
}

fn delimiter(character: char) -> bool {
    separator(character) || matches!(character, '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';')
}

impl<'a> Reader<'a> {
    fn rest(&self) -> &'a str {
        &self.source[self.at..]
    }

    fn error(&mut self, start: usize, message: impl Into<String>) {
        self.error_at(Span::new(start, self.at), message);
    }

    fn error_at(&mut self, span: Span, message: impl Into<String>) {
        self.errors.push(ReadError {
            message: message.into(),
            span,
        });
    }

    fn next_char(&mut self) -> Option<char> {
        let character = self.rest().chars().next()?;
        self.at += character.len_utf8();
        Some(character)
    }

    fn skip_separators(&mut self) {
        let rest = self.rest();
        self.at += rest.len() - rest.trim_start_matches(separator).len();
    }

    fn token(&mut self) -> &'a str {
        let rest = self.rest();
        let length = rest.find(delimiter).unwrap_or(rest.len());
        self.at += length;
        &rest[..length]
    }

    fn read_all(&mut self) {
        loop {
            self.skip_separators();
            let start = self.at;
            let Some(first) = self.rest().chars().next() else {
                while let Some(open) = self.open.pop() {
                    self.unclosed(open);
                }
                return;
            };
            match first {
                '(' | '[' | '{' => self.opening(first, start),
                ')' | ']' | '}' => self.closing(first, start),
                '"' => {
                    let datum = self.string(start);
                    self.push(datum, start);
                }
                '\\' => {
                    let datum = self.character(start);
                    self.push(datum, start);
                }
                _ if delimiter(first) => {
                    self.at += first.len_utf8();
                    self.error(start, format!("Unexpected {first}"));
                }
                _ => {
                    let datum = atom(self.token());
                    self.push(datum, start);
                }
            }
        }
    }

    fn push(&mut self, datum: Datum, start: usize) {
        self.place(Syntax {
            datum,
            span: Span::new(start, self.at),
        });
    }

    fn place(&mut self, syntax: Syntax) {
        match self.open.last_mut() {
            Some(open) => open.items.push(syntax),
            None => self.top.push(syntax),
        }
    }

    fn opening(&mut self, bracket: char, start: usize) {
        if self.open.len() >= MAX_DEPTH {
            self.error_at(
                Span::new(start, start + 1),
                format!("Nested deeper than {MAX_DEPTH}"),
            );
            self.skip_balanced();
            return;
        }
        self.at += 1;
        self.open.push(Open {
            bracket,
            start,
            items: Vec::new(),
        });
    }

    fn closing(&mut self, bracket: char, start: usize) {
        let Some(position) = self
            .open
            .iter()
            .rposition(|open| closer(open.bracket) == bracket)
        else {
            self.at += 1;
            self.error(start, format!("Unexpected {bracket}"));
            return;
        };
        while self.open.len() > position + 1 {
            if let Some(open) = self.open.pop() {
                self.unclosed(open);
            }
        }
        self.at += 1;
        if let Some(open) = self.open.pop() {
            self.finish(open);
        }
    }

    fn unclosed(&mut self, open: Open) {
        self.error_at(
            Span::new(open.start, open.start + 1),
            format!("This {} is never closed", open.bracket),
        );
        self.finish(open);
    }

    fn finish(&mut self, open: Open) {
        let span = Span::new(open.start, self.at);
        let datum = match open.bracket {
            '(' => Datum::List(open.items),
            '[' => Datum::Vector(open.items),
            _ => {
                if open.items.len() % 2 == 1 {
                    self.error_at(span, "This map has a key without a value");
                }
                Datum::Map(open.items)
            }
        };
        self.place(Syntax { datum, span });
    }

    fn skip_balanced(&mut self) {
        let mut depth = 0_usize;
        while let Some(character) = self.next_char() {
            match character {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
            if depth == 0 {
                return;
            }
        }
    }
    fn string(&mut self, start: usize) -> Datum {
        self.at += 1;
        let mut text = String::new();
        loop {
            let escape = self.at;
            match self.next_char() {
                None => {
                    self.error(start, "This string is never closed");
                    break;
                }
                Some('"') => break,
                Some('\\') => text.extend(self.escape(escape)),
                Some(other) => text.push(other),
            }
        }
        Datum::Str(text)
    }

    fn escape(&mut self, start: usize) -> Option<char> {
        let character = self.next_char()?;
        let escaped = match character {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '0' => '\0',
            '\\' | '"' => character,
            'u' => {
                let code = self.code_point();
                if code.is_none() {
                    self.error(start, "Write \\u{…} with a hexadecimal code point");
                }
                return code;
            }
            other => {
                self.error(start, format!("Unknown escape \\{other}"));
                other
            }
        };
        Some(escaped)
    }

    fn code_point(&mut self) -> Option<char> {
        let inner = self.rest().strip_prefix('{')?;
        let close = inner.find('}')?;
        self.at += close + 2;
        u32::from_str_radix(&inner[..close], 16)
            .ok()
            .and_then(char::from_u32)
    }

    fn character(&mut self, start: usize) -> Datum {
        self.at += 1;
        let Some(first) = self.rest().chars().next() else {
            self.error(start, "Name a character after \\");
            return Datum::Char(' ');
        };
        let found = if first == 'u' && self.rest()[1..].starts_with('{') {
            self.at += 1;
            self.code_point()
        } else if delimiter(first) {
            self.at += first.len_utf8();
            Some(first)
        } else {
            named(self.token())
        };
        let Some(character) = found else {
            let source = self.source;
            self.error(
                start,
                format!("Unknown character {}", &source[start..self.at]),
            );
            return Datum::Char(' ');
        };
        Datum::Char(character)
    }
}

fn named(name: &str) -> Option<char> {
    let mut characters = name.chars();
    if let (Some(only), None) = (characters.next(), characters.next()) {
        return Some(only);
    }
    match name {
        "newline" => Some('\n'),
        "space" => Some(' '),
        "tab" => Some('\t'),
        "return" => Some('\r'),
        "nul" => Some('\0'),
        _ => None,
    }
}

fn numeric(token: &str) -> bool {
    token
        .strip_prefix(['-', '+'])
        .unwrap_or(token)
        .starts_with(|character: char| character.is_ascii_digit())
}

fn atom(token: &str) -> Datum {
    if numeric(token) {
        if let Ok(value) = token.parse::<i64>() {
            return Datum::Int(value);
        }
        if let Ok(value) = token.parse::<f64>() {
            return Datum::Float(value);
        }
    }
    if let Some(name) = token.strip_prefix(':').filter(|name| !name.is_empty()) {
        return Datum::Keyword(name.to_owned());
    }
    if let Some(name) = token
        .strip_suffix(':')
        .filter(|name| !name.is_empty() && !name.contains(':'))
    {
        return Datum::Keyword(name.to_owned());
    }
    Datum::Symbol(token.to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn data(source: &str) -> Vec<(Datum, Span)> {
        let (data, errors) = read(source);
        assert_eq!(errors, [], "{source}");
        data.into_iter()
            .map(|syntax| (syntax.datum, syntax.span))
            .collect()
    }

    fn symbol(name: &str) -> Datum {
        Datum::Symbol(name.to_owned())
    }

    #[test]
    fn integers_floats_and_symbols_are_read_with_their_spans() {
        assert_eq!(
            data("42 -7 3.5 foo"),
            [
                (Datum::Int(42), Span::new(0, 2)),
                (Datum::Int(-7), Span::new(3, 5)),
                (Datum::Float(3.5), Span::new(6, 9)),
                (symbol("foo"), Span::new(10, 13)),
            ]
        );
    }

    #[test]
    fn exponents_make_floats() {
        assert_eq!(
            data("1e3 -2.5e-1"),
            [
                (Datum::Float(1000.0), Span::new(0, 3)),
                (Datum::Float(-0.25), Span::new(4, 11)),
            ]
        );
    }

    #[test]
    fn signs_dots_and_arrows_alone_are_symbols() {
        let names: Vec<Datum> = data("+ - ... a->b inf nan")
            .into_iter()
            .map(|(datum, _)| datum)
            .collect();

        assert_eq!(names, ["+", "-", "...", "a->b", "inf", "nan"].map(symbol));
    }

    #[test]
    fn commas_and_newlines_separate_like_spaces() {
        assert_eq!(
            data("a,b\n\tc"),
            [
                (symbol("a"), Span::new(0, 1)),
                (symbol("b"), Span::new(2, 3)),
                (symbol("c"), Span::new(5, 6)),
            ]
        );
    }

    fn errors(source: &str) -> Vec<(String, Span)> {
        read(source)
            .1
            .into_iter()
            .map(|error| (error.message, error.span))
            .collect()
    }

    fn string(text: &str) -> Datum {
        Datum::Str(text.to_owned())
    }

    fn keyword(name: &str) -> Datum {
        Datum::Keyword(name.to_owned())
    }

    #[test]
    fn strings_read_with_their_escapes() {
        assert_eq!(
            data(r#""a\nb" "tab\there" "\"q\" \\ \u{263A}\0\r""#),
            [
                (string("a\nb"), Span::new(0, 6)),
                (string("tab\there"), Span::new(7, 18)),
                (string("\"q\" \\ \u{263A}\0\r"), Span::new(19, 42)),
            ]
        );
    }

    #[test]
    fn strings_keep_their_lines_and_unicode() {
        assert_eq!(data("\"λ\nx\""), [(string("λ\nx"), Span::new(0, 6))]);
    }

    #[test]
    fn unclosed_strings_and_unknown_escapes_are_reported() {
        assert_eq!(
            errors(r#""ok\q" "abc"#),
            [
                ("Unknown escape \\q".to_owned(), Span::new(3, 5)),
                ("This string is never closed".to_owned(), Span::new(7, 11)),
            ]
        );
        assert_eq!(
            read(r#""ok\q" "abc"#)
                .0
                .into_iter()
                .map(|syntax| syntax.datum)
                .collect::<Vec<_>>(),
            [string("okq"), string("abc")]
        );
    }

    #[test]
    fn characters_read_by_themselves_or_by_name() {
        let characters: Vec<Datum> = data(r"\a \( \newline \space \tab \return \u{263A} \λ")
            .into_iter()
            .map(|(datum, _)| datum)
            .collect();

        assert_eq!(
            characters,
            ['a', '(', '\n', ' ', '\t', '\r', '\u{263A}', 'λ'].map(Datum::Char)
        );
        assert_eq!(
            data(r"\u{263A}"),
            [(Datum::Char('\u{263A}'), Span::new(0, 8))]
        );
    }

    #[test]
    fn unknown_characters_are_reported() {
        assert_eq!(
            errors(r"\bogus \u{110000} \"),
            [
                ("Unknown character \\bogus".to_owned(), Span::new(0, 6)),
                ("Unknown character \\u{110000}".to_owned(), Span::new(7, 17)),
                ("Name a character after \\".to_owned(), Span::new(18, 19)),
            ]
        );
    }

    #[test]
    fn keywords_read_with_a_leading_or_trailing_colon() {
        assert_eq!(
            data(":key name: : a:b"),
            [
                (keyword("key"), Span::new(0, 4)),
                (keyword("name"), Span::new(5, 10)),
                (symbol(":"), Span::new(11, 12)),
                (symbol("a:b"), Span::new(13, 16)),
            ]
        );
    }

    fn shape(syntax: &Syntax) -> String {
        let join = |items: &[Syntax]| items.iter().map(shape).collect::<Vec<_>>().join(" ");
        match &syntax.datum {
            Datum::Int(number) => number.to_string(),
            Datum::Float(number) => format!("{number:?}"),
            Datum::Char(character) => format!("\\{character}"),
            Datum::Str(text) => format!("{text:?}"),
            Datum::Symbol(name) => name.clone(),
            Datum::Keyword(name) => format!(":{name}"),
            Datum::List(items) => format!("({})", join(items)),
            Datum::Vector(items) => format!("[{}]", join(items)),
            Datum::Map(items) => format!("{{{}}}", join(items)),
        }
    }

    fn shapes(source: &str) -> Vec<String> {
        read(source).0.iter().map(shape).collect()
    }

    #[test]
    fn lists_vectors_and_maps_nest() {
        assert_eq!(errors("(f [1 2] {a: 1 :b [x]}) () [] {}"), []);
        assert_eq!(
            shapes("(f [1 2] {a: 1 :b [x]}) () [] {}"),
            ["(f [1 2] {:a 1 :b [x]})", "()", "[]", "{}"]
        );
    }

    #[test]
    fn collections_span_from_opener_to_closer() {
        let (data, _) = read(" (a [b])");
        let Datum::List(items) = &data[0].datum else {
            panic!("expected a list");
        };

        assert_eq!(data[0].span, Span::new(1, 8));
        assert_eq!(items[1].span, Span::new(4, 7));
    }

    #[test]
    fn unclosed_and_mismatched_brackets_are_reported_where_they_open() {
        assert_eq!(
            errors("(a [b) c"),
            [("This [ is never closed".to_owned(), Span::new(3, 4))]
        );
        assert_eq!(shapes("(a [b) c"), ["(a [b])", "c"]);
        assert_eq!(
            errors("(a"),
            [("This ( is never closed".to_owned(), Span::new(0, 1))]
        );
    }

    #[test]
    fn stray_closers_are_reported_and_skipped() {
        assert_eq!(
            errors("(a ] b) }"),
            [
                ("Unexpected ]".to_owned(), Span::new(3, 4)),
                ("Unexpected }".to_owned(), Span::new(8, 9)),
            ]
        );
        assert_eq!(shapes("(a ] b) }"), ["(a b)"]);
    }

    #[test]
    fn maps_need_a_value_for_every_key() {
        assert_eq!(
            errors("{a: 1 b:}"),
            [(
                "This map has a key without a value".to_owned(),
                Span::new(0, 9)
            )]
        );
    }

    #[test]
    fn nesting_is_limited_without_overflowing_the_stack() {
        let deep = format!("{}{} x", "(".repeat(5000), ")".repeat(5000));

        assert_eq!(
            errors(&deep),
            [("Nested deeper than 1000".to_owned(), Span::new(1000, 1001))]
        );
        assert_eq!(read(&deep).0.len(), 2);
    }

    #[test]
    fn symbols_span_their_bytes_in_utf8() {
        assert_eq!(
            data("λ café"),
            [
                (symbol("λ"), Span::new(0, 2)),
                (symbol("café"), Span::new(3, 8)),
            ]
        );
    }
}
