use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum Datum {
    Nil,
    Bool(bool),
    Int(i64),
    BigInt(String),
    Float(f64),
    Char(char),
    Str(String),
    Symbol(String),
    Keyword(String),
    List(Vec<Syntax>),
    Vector(Vec<Syntax>),
    Map(Vec<Syntax>),
    Set(Vec<Syntax>),
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
    pub incomplete: bool,
}

const MAX_DEPTH: usize = 1000;
const MAX_INTERPOLATION: usize = 16;

pub fn read(source: &str) -> (Vec<Syntax>, Vec<ReadError>) {
    let mut reader = Reader {
        source,
        at: 0,
        end: source.len(),
        depth: 0,
        errors: Vec::new(),
        frames: Vec::new(),
        top: Vec::new(),
    };
    reader.read_all();
    (reader.top, reader.errors)
}

enum Frame {
    Collection {
        bracket: char,
        start: usize,
        items: Vec<Syntax>,
    },
    Prefix {
        name: Option<&'static str>,
        mark: &'static str,
        start: usize,
    },
}

struct Reader<'a> {
    source: &'a str,
    at: usize,
    end: usize,
    depth: usize,
    errors: Vec<ReadError>,
    frames: Vec<Frame>,
    top: Vec<Syntax>,
}

fn closer(opener: char) -> char {
    match opener {
        '(' => ')',
        '[' => ']',
        _ => '}',
    }
}

fn opener(bracket: char) -> String {
    if bracket == '#' {
        "#{".to_owned()
    } else {
        bracket.to_string()
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
        &self.source[self.at..self.end]
    }

    fn error(&mut self, start: usize, message: impl Into<String>) {
        self.error_at(Span::new(start, self.at), message);
    }

    fn error_at(&mut self, span: Span, message: impl Into<String>) {
        self.errors.push(ReadError {
            message: message.into(),
            span,
            incomplete: false,
        });
    }

    fn unfinished(&mut self, span: Span, message: impl Into<String>) {
        self.errors.push(ReadError {
            message: message.into(),
            span,
            incomplete: self.end == self.source.len(),
        });
    }

    fn next_char(&mut self) -> Option<char> {
        let character = self.rest().chars().next()?;
        self.at += character.len_utf8();
        Some(character)
    }

    fn skip_separators(&mut self) {
        loop {
            let rest = self.rest();
            self.at += rest.len() - rest.trim_start_matches(separator).len();
            let rest = self.rest();
            if rest.starts_with(';') {
                self.at += rest.find('\n').unwrap_or(rest.len());
            } else if rest.starts_with("#|") {
                self.block_comment();
            } else {
                return;
            }
        }
    }

    fn block_comment(&mut self) {
        let start = self.at;
        self.at += 2;
        let mut depth = 1_usize;
        while depth > 0 {
            let rest = self.rest();
            if rest.starts_with("#|") {
                depth += 1;
                self.at += 2;
            } else if rest.starts_with("|#") {
                depth -= 1;
                self.at += 2;
            } else if self.next_char().is_none() {
                self.unfinished(Span::new(start, self.at), "This comment is never closed");
                return;
            }
        }
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
                while let Some(frame) = self.frames.pop() {
                    self.abandon(frame, true);
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
                '\'' => self.prefix(Some("quote"), "'", start),
                '`' => self.prefix(Some("quasiquote"), "`", start),
                '~' if self.rest().starts_with("~@") => {
                    self.prefix(Some("unquote-splicing"), "~@", start);
                }
                '~' => self.prefix(Some("unquote"), "~", start),
                '@' => self.prefix(Some("deref"), "@", start),
                '#' if self.rest().starts_with("#;") => self.prefix(None, "#;", start),
                '#' if self.rest().starts_with("#{") => self.opening('#', start),
                '#' => {
                    let token = self.token();
                    self.error(start, format!("Unknown form {token}"));
                }
                _ if delimiter(first) => {
                    self.at += first.len_utf8();
                    self.error(start, format!("Unexpected {first}"));
                }
                _ => {
                    let token = self.token();
                    let datum = if numeric(token) {
                        self.number(token, start)
                    } else {
                        self.fields(token, start).unwrap_or_else(|| atom(token))
                    };
                    self.push(datum, start);
                }
            }
        }
    }

    fn number(&mut self, token: &str, start: usize) -> Datum {
        let digits = token.strip_prefix(['-', '+']).unwrap_or(token);
        if digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return token.parse::<i64>().map_or_else(
                |_| Datum::BigInt(token.trim_start_matches('+').to_owned()),
                Datum::Int,
            );
        }
        if let Ok(value) = token.parse::<f64>() {
            return Datum::Float(value);
        }
        self.error(start, format!("{token} is not a number"));
        Datum::Symbol(token.to_owned())
    }

    fn fields(&mut self, token: &str, start: usize) -> Option<Datum> {
        if !token.contains('.') || token.starts_with([':', '.']) || token.ends_with('.') {
            return None;
        }
        if token.split('.').any(str::is_empty) {
            self.error(start, format!("Empty field name in {token}"));
            return Some(Datum::Symbol(token.to_owned()));
        }
        let mut offset = start;
        let mut parts = Vec::new();
        for (index, part) in token.split('.').enumerate() {
            let span = Span::new(offset, offset + part.len());
            if index == 0 {
                parts.push(Syntax {
                    datum: Datum::Symbol(".".to_owned()),
                    span: Span::new(span_end(span), span_end(span) + 1),
                });
                parts.push(Syntax {
                    datum: Datum::Symbol(part.to_owned()),
                    span,
                });
            } else {
                parts.push(Syntax {
                    datum: Datum::Keyword(part.to_owned()),
                    span,
                });
            }
            offset += part.len() + 1;
        }
        Some(Datum::List(parts))
    }

    fn push(&mut self, datum: Datum, start: usize) {
        self.place(Syntax {
            datum,
            span: Span::new(start, self.at),
        });
    }

    fn place(&mut self, mut syntax: Syntax) {
        loop {
            match self.frames.last_mut() {
                Some(Frame::Collection { items, .. }) => {
                    items.push(syntax);
                    return;
                }
                Some(Frame::Prefix { .. }) => {
                    let Some(Frame::Prefix { name, mark, start }) = self.frames.pop() else {
                        return;
                    };
                    let Some(name) = name else {
                        return;
                    };
                    let head = Span::new(start, start + mark.len());
                    syntax = Syntax {
                        span: head.through(syntax.span),
                        datum: Datum::List(vec![
                            Syntax {
                                datum: Datum::Symbol(name.to_owned()),
                                span: head,
                            },
                            syntax,
                        ]),
                    };
                }
                None => {
                    self.top.push(syntax);
                    return;
                }
            }
        }
    }

    fn prefix(&mut self, name: Option<&'static str>, mark: &'static str, start: usize) {
        self.at += mark.len();
        self.frames.push(Frame::Prefix { name, mark, start });
    }

    fn opening(&mut self, bracket: char, start: usize) {
        if self.frames.len() >= MAX_DEPTH {
            self.error_at(
                Span::new(start, start + 1),
                format!("Nested deeper than {MAX_DEPTH}"),
            );
            self.skip_balanced();
            return;
        }
        self.at += opener(bracket).len();
        self.frames.push(Frame::Collection {
            bracket,
            start,
            items: Vec::new(),
        });
    }

    fn closing(&mut self, bracket: char, start: usize) {
        let Some(position) = self.frames.iter().rposition(
            |frame| matches!(frame, Frame::Collection { bracket: opener, .. } if closer(*opener) == bracket),
        ) else {
            while matches!(self.frames.last(), Some(Frame::Prefix { .. })) {
                if let Some(frame) = self.frames.pop() {
                    self.abandon(frame, false);
                }
            }
            self.at += 1;
            self.error(start, format!("Unexpected {bracket}"));
            return;
        };
        while self.frames.len() > position + 1 {
            if let Some(frame) = self.frames.pop() {
                self.abandon(frame, false);
            }
        }
        self.at += 1;
        if let Some(Frame::Collection {
            bracket,
            start,
            items,
        }) = self.frames.pop()
        {
            self.finish(bracket, start, items);
        }
    }

    fn abandon(&mut self, frame: Frame, at_end: bool) {
        let (span, message) = match &frame {
            Frame::Collection { bracket, start, .. } => (
                Span::new(*start, start + opener(*bracket).len()),
                format!("This {} is never closed", opener(*bracket)),
            ),
            Frame::Prefix { mark, start, .. } => (
                Span::new(*start, start + mark.len()),
                format!("Put something after {mark}"),
            ),
        };
        if at_end {
            self.unfinished(span, message);
        } else {
            self.error_at(span, message);
        }
        if let Frame::Collection {
            bracket,
            start,
            items,
        } = frame
        {
            self.finish(bracket, start, items);
        }
    }

    fn finish(&mut self, bracket: char, start: usize, items: Vec<Syntax>) {
        let span = Span::new(start, self.at);
        let datum = match bracket {
            '(' => Datum::List(items),
            '[' => Datum::Vector(items),
            '#' => Datum::Set(items),
            _ => {
                if items.len() % 2 == 1 {
                    self.error_at(span, "This map has a key without a value");
                }
                Datum::Map(items)
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
        let mut pieces = Vec::new();
        let mut piece_start = self.at;
        let mut interpolated = false;
        let piece_end = loop {
            let here = self.at;
            match self.next_char() {
                None => {
                    self.unfinished(Span::new(start, self.at), "This string is never closed");
                    break self.at;
                }
                Some('"') => break here,
                Some('\\') => text.extend(self.escape(here)),
                Some('{') if self.rest().starts_with('{') => {
                    self.at += 1;
                    text.push('{');
                }
                Some('}') if self.rest().starts_with('}') => {
                    self.at += 1;
                    text.push('}');
                }
                Some('{') => {
                    interpolated = true;
                    if !text.is_empty() {
                        pieces.push(Syntax {
                            datum: Datum::Str(std::mem::take(&mut text)),
                            span: Span::new(piece_start, here),
                        });
                    }
                    pieces.extend(self.interpolation(here));
                    piece_start = self.at;
                }
                Some(other) => text.push(other),
            }
        };
        if !interpolated {
            return Datum::Str(text);
        }
        if !text.is_empty() {
            pieces.push(Syntax {
                datum: Datum::Str(text),
                span: Span::new(piece_start, piece_end),
            });
        }
        let head = Syntax {
            datum: Datum::Symbol("str".to_owned()),
            span: Span::new(start, start + 1),
        };
        Datum::List(std::iter::once(head).chain(pieces).collect())
    }

    fn interpolation(&mut self, open: usize) -> Option<Syntax> {
        let Some(close) = self.matching_brace() else {
            self.unfinished(Span::new(open, open + 1), "This { is never closed");
            self.at = self.end;
            return None;
        };
        let braces = Span::new(open, close + 1);
        if self.depth >= MAX_INTERPOLATION {
            self.error_at(braces, "Strings nested too deeply");
            self.at = close + 1;
            return None;
        }
        let mut inner = Reader {
            source: self.source,
            at: self.at,
            end: close,
            depth: self.depth + 1,
            errors: Vec::new(),
            frames: Vec::new(),
            top: Vec::new(),
        };
        inner.read_all();
        self.errors.append(&mut inner.errors);
        self.at = close + 1;
        let mut found = inner.top.into_iter();
        match (found.next(), found.next()) {
            (Some(only), None) => Some(only),
            (None, _) => {
                self.error_at(braces, "Put an expression inside the braces");
                None
            }
            (Some(first), Some(_)) => {
                self.error_at(braces, "Put one expression inside the braces");
                Some(first)
            }
        }
    }

    fn matching_brace(&self) -> Option<usize> {
        let mut depth = 0_usize;
        let mut characters = self.rest().char_indices();
        while let Some((offset, character)) = characters.next() {
            match character {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' => depth = depth.saturating_sub(1),
                '}' if depth == 0 => return Some(self.at + offset),
                '}' => depth -= 1,
                '"' => loop {
                    match characters.next()? {
                        (_, '\\') => {
                            characters.next();
                        }
                        (_, '"') => break,
                        _ => {}
                    }
                },
                '\\' => {
                    characters.next();
                }
                _ => {}
            }
        }
        None
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

fn span_end(span: Span) -> usize {
    span.end as usize
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
    if let Some(name) = token.strip_prefix(':').filter(|name| !name.is_empty()) {
        return Datum::Keyword(name.to_owned());
    }
    if let Some(name) = token
        .strip_suffix(':')
        .filter(|name| !name.is_empty() && !name.contains(':'))
    {
        return Datum::Keyword(name.to_owned());
    }
    match token {
        "nil" => Datum::Nil,
        "true" => Datum::Bool(true),
        "false" => Datum::Bool(false),
        _ => Datum::Symbol(token.to_owned()),
    }
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
            Datum::Nil => "nil".to_owned(),
            Datum::Bool(value) => value.to_string(),
            Datum::Int(number) => number.to_string(),
            Datum::BigInt(digits) => digits.clone(),
            Datum::Float(number) => format!("{number:?}"),
            Datum::Char(character) => format!("\\{character}"),
            Datum::Str(text) => format!("{text:?}"),
            Datum::Symbol(name) => name.clone(),
            Datum::Keyword(name) => format!(":{name}"),
            Datum::List(items) => format!("({})", join(items)),
            Datum::Vector(items) => format!("[{}]", join(items)),
            Datum::Map(items) => format!("{{{}}}", join(items)),
            Datum::Set(items) => format!("#{{{}}}", join(items)),
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
    fn sets_are_read_from_hash_braces() {
        assert_eq!(errors("#{1 [2] :a} #{} (f #{x})"), []);
        assert_eq!(
            shapes("#{1 [2] :a} #{} (f #{x})"),
            ["#{1 [2] :a}", "#{}", "(f #{x})"]
        );
        assert_eq!(data("#{}")[0].1, Span::new(0, 3));
        assert_eq!(
            errors("#{1"),
            [("This #{ is never closed".to_owned(), Span::new(0, 2))]
        );
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
    fn true_false_and_nil_are_read_as_themselves() {
        assert_eq!(
            data("true false nil truthy"),
            [
                (Datum::Bool(true), Span::new(0, 4)),
                (Datum::Bool(false), Span::new(5, 10)),
                (Datum::Nil, Span::new(11, 14)),
                (symbol("truthy"), Span::new(15, 21)),
            ]
        );
    }

    #[test]
    fn quote_forms_wrap_the_next_form() {
        assert_eq!(errors("'x `(a ~b ~@c) @atom '[1]"), []);
        assert_eq!(
            shapes("'x `(a ~b ~@c) @atom '[1]"),
            [
                "(quote x)",
                "(quasiquote (a (unquote b) (unquote-splicing c)))",
                "(deref atom)",
                "(quote [1])",
            ]
        );
        assert_eq!(read("'x").0[0].span, Span::new(0, 2));
    }

    #[test]
    fn quotes_with_nothing_after_them_are_reported() {
        assert_eq!(
            errors("(a ') '"),
            [
                ("Put something after '".to_owned(), Span::new(3, 4)),
                ("Put something after '".to_owned(), Span::new(6, 7)),
            ]
        );
        assert_eq!(shapes("(a ') '"), ["(a)"]);
    }

    #[test]
    fn comments_are_skipped() {
        let source = "a ; line\nb #| block #| nested |# |# c #;(skipped form) d #;  e f";

        assert_eq!(errors(source), []);
        assert_eq!(shapes(source), ["a", "b", "c", "d", "f"]);
    }

    #[test]
    fn unclosed_block_comments_and_unknown_hash_forms_are_reported() {
        assert_eq!(
            errors("#x a #| open"),
            [
                ("Unknown form #x".to_owned(), Span::new(0, 2)),
                ("This comment is never closed".to_owned(), Span::new(5, 12)),
            ]
        );
    }
    #[test]
    fn dots_inside_symbols_read_fields() {
        assert_eq!(
            shapes("city.name a.b.c ... .5 x. :a.b 1.5"),
            [
                "(. city :name)",
                "(. a :b :c)",
                "...",
                ".5",
                "x.",
                ":a.b",
                "1.5",
            ]
        );
        let (data, _) = read("city.name");
        let Datum::List(parts) = &data[0].datum else {
            panic!("expected a field access");
        };
        assert_eq!(data[0].span, Span::new(0, 9));
        assert_eq!(parts[1].span, Span::new(0, 4));
        assert_eq!(parts[2].span, Span::new(5, 9));
    }

    #[test]
    fn empty_field_names_are_reported() {
        assert_eq!(
            errors("a..b"),
            [("Empty field name in a..b".to_owned(), Span::new(0, 4))]
        );
    }

    #[test]
    fn strings_interpolate_expressions_in_braces() {
        assert_eq!(
            shapes(r#""Hi {name}!" "{{x}} {(+ 1 2)}" "{city.name}" "plain}""#),
            [
                r#"(str "Hi " name "!")"#,
                r#"(str "{x} " (+ 1 2))"#,
                "(str (. city :name))",
                r#""plain}""#,
            ]
        );
    }

    #[test]
    fn interpolated_expressions_keep_their_spans_and_may_hold_strings() {
        let source = r#""a{(f "}")}b""#;
        let (data, errors) = read(source);
        let Datum::List(pieces) = &data[0].datum else {
            panic!("expected an interpolation");
        };

        assert_eq!(errors, []);
        assert_eq!(shape(&data[0]), r#"(str "a" (f "}") "b")"#);
        assert_eq!(data[0].span, Span::new(0, 13));
        assert_eq!(pieces[2].span, Span::new(3, 10));
    }

    #[test]
    fn interpolation_mistakes_are_reported() {
        assert_eq!(
            errors(r#""{}" "{a b}" "{open""#),
            [
                (
                    "Put an expression inside the braces".to_owned(),
                    Span::new(1, 3)
                ),
                (
                    "Put one expression inside the braces".to_owned(),
                    Span::new(6, 11)
                ),
                ("This { is never closed".to_owned(), Span::new(14, 15)),
                ("This string is never closed".to_owned(), Span::new(13, 20)),
            ]
        );
    }
    #[test]
    fn integers_too_big_for_64_bits_read_as_big_integers() {
        assert_eq!(
            data("123456789012345678901234567890 -99999999999999999999 +5"),
            [
                (
                    Datum::BigInt("123456789012345678901234567890".to_owned()),
                    Span::new(0, 30)
                ),
                (
                    Datum::BigInt("-99999999999999999999".to_owned()),
                    Span::new(31, 52)
                ),
                (Datum::Int(5), Span::new(53, 55)),
            ]
        );
    }

    #[test]
    fn malformed_numbers_are_reported() {
        assert_eq!(
            errors("1abc 2.5.1 -3x 1e"),
            [
                ("1abc is not a number".to_owned(), Span::new(0, 4)),
                ("2.5.1 is not a number".to_owned(), Span::new(5, 10)),
                ("-3x is not a number".to_owned(), Span::new(11, 14)),
                ("1e is not a number".to_owned(), Span::new(15, 17)),
            ]
        );
    }

    #[test]
    fn reading_goes_on_after_mistakes() {
        assert_eq!(
            errors(r#"(a 1x) "ok" ) b"#),
            [
                ("1x is not a number".to_owned(), Span::new(3, 5)),
                ("Unexpected )".to_owned(), Span::new(12, 13)),
            ]
        );
        assert_eq!(shapes(r#"(a 1x) "ok" ) b"#), ["(a 1x)", r#""ok""#, "b"]);
    }
    #[test]
    fn errors_at_the_end_say_more_input_could_finish_them() {
        let flags = |source: &str| -> Vec<bool> {
            read(source)
                .1
                .into_iter()
                .map(|error| error.incomplete)
                .collect()
        };

        assert_eq!(flags("(a [b"), [true, true]);
        assert_eq!(flags("\"open"), [true]);
        assert_eq!(flags("#| open"), [true]);
        assert_eq!(flags("'"), [true]);
        assert_eq!(flags("(a [b)"), [false]);
        assert_eq!(flags("1x"), [false]);
        assert_eq!(flags(r#""{"#), [true, true]);
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
