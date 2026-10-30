mod syntax;

use std::collections::HashMap;
use std::mem;

use crate::span::Span;
use crate::syntax::{Bracket, Kind, Prefix, State, Token, delimiter, tokens};
pub use syntax::{Datum, Marks, PRELUDE_MARK, Syntax, meta_flag, split_meta};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    pub message: String,
    pub span: Span,
    pub incomplete: bool,
}

const MAX_DEPTH: usize = 1000;
const MAX_INTERPOLATION: usize = 16;

pub fn read(source: &str) -> (Vec<Syntax>, Vec<ReadError>) {
    let (found, _) = tokens(source, &State::default());
    let holes = matching_holes(&found);
    let mut reader = Reader {
        source,
        tokens: &found,
        next: 0,
        at: 0,
        stop: None,
        holes,
        depth: 0,
        errors: Vec::new(),
        frames: Vec::new(),
        top: Vec::new(),
    };
    reader.read_all(None);
    (reader.top, reader.errors)
}

fn matching_holes(found: &[Token]) -> HashMap<usize, usize> {
    let mut open = Vec::new();
    let mut matched = HashMap::new();
    for (index, token) in found.iter().enumerate() {
        match token.kind {
            Kind::HoleStart => open.push(index),
            Kind::HoleEnd => {
                if let Some(start) = open.pop() {
                    matched.insert(start, index);
                }
            }
            _ => {}
        }
    }
    matched
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Opener {
    List,
    Vector,
    Map,
    Set,
    Record(String),
    Meta,
}

impl Opener {
    fn closer(&self) -> char {
        match self {
            Self::List => ')',
            Self::Vector => ']',
            Self::Map | Self::Set | Self::Record(_) | Self::Meta => '}',
        }
    }

    fn text(&self) -> String {
        match self {
            Self::List => "(".to_owned(),
            Self::Vector => "[".to_owned(),
            Self::Map => "{".to_owned(),
            Self::Set => "#{".to_owned(),
            Self::Record(name) => format!("#{name}{{"),
            Self::Meta => "^{".to_owned(),
        }
    }
}

const META: &str = "^ takes a keyword or a map, as in ^:shadow or ^{indent: 1}";
const ONE_LINE: &str =
    "Text that follows \"\"\" on its line must end on that line; start a block on the next line";
const CLOSING: &str = "Put the closing \"\"\" of a block on a line of its own";
const SHALLOW: &str = "Indent every line of a \"\"\" block at least as far as its closing \"\"\"";
const RAW: &str = "#r\"…\" is gone: write \"\"\"…\"\"\" for text taken as it is; ox migrate --strings rewrites it";

enum Piece {
    Text(String, Span),
    Hole(Syntax),
}

fn hygienic(name: &str, span: Span) -> Syntax {
    Syntax {
        datum: Datum::Symbol(name.to_owned()),
        span,
        marks: Marks::prelude(),
    }
}

fn assembled(pieces: Vec<Piece>, start: usize) -> Datum {
    if !pieces.iter().any(|piece| matches!(piece, Piece::Hole(_))) {
        let mut text = String::new();
        for piece in pieces {
            if let Piece::Text(part, _) = piece {
                text.push_str(&part);
            }
        }
        return Datum::Str(text);
    }
    let parts = pieces.into_iter().filter_map(|piece| match piece {
        Piece::Text(text, _) if text.is_empty() => None,
        Piece::Text(text, span) => Some(Syntax::new(Datum::Str(text), span)),
        Piece::Hole(hole) => Some(hole),
    });
    let head = hygienic("str", Span::new(start, start + 1));
    Datum::List(std::iter::once(head).chain(parts).collect())
}

fn dedented(pieces: &mut [Piece], indent: &str) -> bool {
    let mut fits = true;
    let mut line_start = true;
    let count = pieces.len();
    for (index, piece) in pieces.iter_mut().enumerate() {
        let Piece::Text(text, _) = piece else {
            fits &= !line_start || indent.is_empty();
            line_start = false;
            continue;
        };
        let last = index + 1 == count;
        let mut out = String::new();
        let mut lines = text.split('\n').peekable();
        let mut first = true;
        while let Some(line) = lines.next() {
            if !first {
                out.push('\n');
                line_start = true;
            }
            first = false;
            let ends = lines.peek().is_some() || last;
            let line = if lines.peek().is_some() {
                line.strip_suffix('\r').unwrap_or(line)
            } else {
                line
            };
            if !line_start {
                out.push_str(line);
                continue;
            }
            line_start = false;
            if let Some(rest) = line.strip_prefix(indent) {
                out.push_str(rest);
            } else if !(ends && line.trim().is_empty()) {
                fits = false;
                out.push_str(line);
            }
        }
        *text = out;
    }
    fits
}

enum Frame {
    Collection {
        opener: Opener,
        start: usize,
        items: Vec<Syntax>,
    },
    Prefix {
        prefix: Prefix,
        start: usize,
    },
}

struct Reader<'a> {
    source: &'a str,
    tokens: &'a [Token],
    next: usize,
    at: usize,
    stop: Option<usize>,
    holes: HashMap<usize, usize>,
    depth: usize,
    errors: Vec<ReadError>,
    frames: Vec<Frame>,
    top: Vec<Syntax>,
}

fn marked(prefix: Prefix) -> (Option<&'static str>, &'static str) {
    match prefix {
        Prefix::Quote => (Some("quote"), "'"),
        Prefix::Quasiquote => (Some("quasiquote"), "`"),
        Prefix::Unquote => (Some("unquote"), "~"),
        Prefix::Splice => (Some("unquote-splicing"), "~@"),
        Prefix::Deref => (Some("deref"), "@"),
        Prefix::Skip => (None, "#;"),
        Prefix::Keyword => (None, ":"),
    }
}

impl<'a> Reader<'a> {
    fn peek(&self) -> Option<Token> {
        (Some(self.next) != self.stop)
            .then(|| self.tokens.get(self.next).copied())
            .flatten()
    }

    fn take(&mut self) -> Option<Token> {
        let token = self.peek()?;
        self.next += 1;
        self.at = token.ends();
        Some(token)
    }

    fn text(&self, token: Token) -> &'a str {
        token.text(self.source)
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
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
            incomplete: true,
        });
    }

    fn read_all(&mut self, stop: Option<usize>) {
        let outer = mem::replace(&mut self.stop, stop);
        while let Some(token) = self.take() {
            self.read_token(token);
        }
        while let Some(frame) = self.frames.pop() {
            self.abandon(frame, stop.is_none());
        }
        self.stop = outer;
    }

    fn read_token(&mut self, token: Token) {
        let start = token.starts();
        let text = self.text(token);
        match token.kind {
            Kind::Space
            | Kind::Comment
            | Kind::BlockComment { closed: true }
            | Kind::Text
            | Kind::Escape
            | Kind::HoleStart
            | Kind::StringEnd => {}
            Kind::BlockComment { closed: false } => {
                self.unfinished(token.span, "This comment is never closed");
            }
            Kind::Open(bracket) => {
                let opener = match bracket {
                    Bracket::List => Opener::List,
                    Bracket::Vector => Opener::Vector,
                    Bracket::Map => Opener::Map,
                    Bracket::Set => Opener::Set,
                    Bracket::Record => Opener::Record(text[1..text.len() - 1].to_owned()),
                    Bracket::Meta => Opener::Meta,
                };
                self.opening(opener, start);
            }
            Kind::Close | Kind::HoleEnd => {
                self.closing(text.chars().next().unwrap_or(')'), token);
            }
            Kind::Prefix(prefix) => self.frames.push(Frame::Prefix { prefix, start }),
            Kind::Flag => {
                let after = start + 1;
                let items = vec![
                    Syntax::new(
                        Datum::Keyword(text[2..].to_owned()),
                        Span::new(after, self.at),
                    ),
                    Syntax::new(Datum::Bool(true), Span::new(after, self.at)),
                ];
                self.push(Datum::Meta(items), start);
            }
            Kind::Caret => self.error(token.span, META),
            Kind::Dispatch => self.error(token.span, format!("Unknown form {text}")),
            Kind::StringStart => {
                let datum = self.string(token);
                self.push(datum, start);
            }
            Kind::Raw { .. } => {
                let datum = self.raw(token);
                self.push(datum, start);
            }
            Kind::Regex { closed } => {
                let datum = self.regex(token, closed);
                self.push(datum, start);
            }
            Kind::Char => {
                let datum = self.character(token);
                self.push(datum, start);
            }
            Kind::Number => {
                let datum = self.number(text, token.span);
                self.push(datum, start);
            }
            Kind::Keyword | Kind::Symbol => {
                let datum = self.fields(text, token.span).unwrap_or_else(|| atom(text));
                self.push(datum, start);
            }
        }
    }

    fn number(&mut self, token: &str, span: Span) -> Datum {
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
        self.error(span, format!("{token} is not a number"));
        Datum::Symbol(token.to_owned())
    }

    fn fields(&mut self, token: &str, span: Span) -> Option<Datum> {
        if !token.contains('.') || token.starts_with([':', '.']) || token.ends_with('.') {
            return None;
        }
        if token.split('.').any(str::is_empty) {
            self.error(span, format!("Empty field name in {token}"));
            return Some(Datum::Symbol(token.to_owned()));
        }
        let mut offset = span.start as usize;
        let mut parts = Vec::new();
        for (index, part) in token.split('.').enumerate() {
            let span = Span::new(offset, offset + part.len());
            if index == 0 {
                parts.push(Syntax {
                    datum: Datum::Symbol(".".to_owned()),
                    span: Span::new(span_end(span), span_end(span) + 1),
                    marks: Marks::default(),
                });
                parts.push(Syntax {
                    datum: Datum::Symbol(part.to_owned()),
                    span,
                    marks: Marks::default(),
                });
            } else {
                parts.push(Syntax {
                    datum: Datum::Keyword(part.to_owned()),
                    span,
                    marks: Marks::default(),
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
            marks: Marks::default(),
        });
    }

    fn quoted_keyword(&mut self, head: Span, syntax: Syntax) -> Syntax {
        let span = head.through(syntax.span);
        if let Datum::Str(name) = &syntax.datum {
            return Syntax::new(Datum::Keyword(name.clone()), span);
        }
        self.error(
            span,
            "A colon makes a keyword of a plain string, as in :\"Unit Price\"",
        );
        syntax
    }

    fn place(&mut self, mut syntax: Syntax) {
        loop {
            match self.frames.last_mut() {
                Some(Frame::Collection { items, .. }) => {
                    items.push(syntax);
                    return;
                }
                Some(Frame::Prefix { .. }) => {
                    let Some(Frame::Prefix { prefix, start }) = self.frames.pop() else {
                        return;
                    };
                    let (name, mark) = marked(prefix);
                    let head = Span::new(start, start + mark.len());
                    if prefix == Prefix::Keyword {
                        syntax = self.quoted_keyword(head, syntax);
                        continue;
                    }
                    let Some(name) = name else {
                        return;
                    };
                    let called = if prefix == Prefix::Deref {
                        hygienic(name, head)
                    } else {
                        Syntax::new(Datum::Symbol(name.to_owned()), head)
                    };
                    syntax = Syntax {
                        span: head.through(syntax.span),
                        datum: Datum::List(vec![called, syntax]),
                        marks: Marks::default(),
                    };
                }
                None => {
                    self.top.push(syntax);
                    return;
                }
            }
        }
    }

    fn opening(&mut self, opener: Opener, start: usize) {
        if self.frames.len() >= MAX_DEPTH {
            self.error(
                Span::new(start, start + 1),
                format!("Nested deeper than {MAX_DEPTH}"),
            );
            self.skip_balanced();
            return;
        }
        self.frames.push(Frame::Collection {
            opener,
            start,
            items: Vec::new(),
        });
    }

    fn closing(&mut self, bracket: char, token: Token) {
        let Some(position) = self.frames.iter().rposition(
            |frame| matches!(frame, Frame::Collection { opener, .. } if opener.closer() == bracket),
        ) else {
            while matches!(self.frames.last(), Some(Frame::Prefix { .. })) {
                if let Some(frame) = self.frames.pop() {
                    self.abandon(frame, false);
                }
            }
            self.error(token.span, format!("Unexpected {bracket}"));
            return;
        };
        self.at = token.starts();
        while self.frames.len() > position + 1 {
            if let Some(frame) = self.frames.pop() {
                self.abandon(frame, false);
            }
        }
        self.at = token.ends();
        if let Some(Frame::Collection {
            opener,
            start,
            items,
        }) = self.frames.pop()
        {
            self.finish(opener, start, items);
        }
    }

    fn abandon(&mut self, frame: Frame, at_end: bool) {
        let (span, message) = match &frame {
            Frame::Collection { opener, start, .. } => (
                Span::new(*start, start + opener.text().len()),
                format!("This {} is never closed", opener.text()),
            ),
            Frame::Prefix { prefix, start } => {
                let (_, mark) = marked(*prefix);
                (
                    Span::new(*start, start + mark.len()),
                    format!("Put something after {mark}"),
                )
            }
        };
        if at_end {
            self.unfinished(span, message);
        } else {
            self.error(span, message);
        }
        if let Frame::Collection {
            opener,
            start,
            items,
        } = frame
        {
            self.finish(opener, start, items);
        }
    }

    fn finish(&mut self, opener: Opener, start: usize, items: Vec<Syntax>) {
        let span = Span::new(start, self.at);
        let datum = match opener {
            Opener::List => Datum::List(items),
            Opener::Vector => Datum::Vector(items),
            Opener::Set => Datum::Set(items),
            Opener::Map => Datum::Map(items),
            Opener::Record(name) => Datum::Record(name, items),
            Opener::Meta => Datum::Meta(items),
        };
        self.place(Syntax {
            datum,
            span,
            marks: Marks::default(),
        });
    }

    fn skip_balanced(&mut self) {
        let mut depth = 1_usize;
        while depth > 0 {
            match self.take().map(|token| token.kind) {
                None => return,
                Some(Kind::Open(_)) => depth += 1,
                Some(Kind::Close) => depth -= 1,
                Some(_) => {}
            }
        }
    }

    fn string(&mut self, open: Token) -> Datum {
        let start = open.starts();
        let template = self.text(open).starts_with('$');
        let fence = open.ends() - start - usize::from(template);
        let mut text = String::new();
        let mut pieces = Vec::new();
        let mut piece_start = self.at;
        let mut closed = true;
        let end = loop {
            let index = self.next;
            let Some(token) = self.take() else {
                self.unfinished(
                    Span::new(start, self.source.len()),
                    "This string is never closed",
                );
                closed = false;
                break self.source.len();
            };
            match token.kind {
                Kind::StringEnd => break token.ends(),
                Kind::Text if template && self.text(token) == "}" => {
                    self.error(token.span, "Write }} for a } in a $ string");
                    text.push('}');
                }
                Kind::Text => text.push_str(self.text(token)),
                Kind::Escape => text.extend(self.escape(token)),
                Kind::HoleStart => {
                    if !text.is_empty() {
                        let span = Span::new(piece_start, token.starts());
                        pieces.push(Piece::Text(mem::take(&mut text), span));
                    }
                    pieces.extend(self.interpolation(token, index).map(Piece::Hole));
                    piece_start = self.at;
                }
                _ => {}
            }
        };
        if !text.is_empty() {
            pieces.push(Piece::Text(text, Span::new(piece_start, end)));
        }
        if fence > 1 && closed {
            pieces = self.verbatim(pieces, Span::new(start, end));
        }
        assembled(pieces, start)
    }

    fn verbatim(&mut self, mut pieces: Vec<Piece>, span: Span) -> Vec<Piece> {
        let opening = match pieces.first() {
            Some(Piece::Text(text, _)) => {
                text.find('\n').filter(|at| text[..*at].trim().is_empty())
            }
            _ => None,
        };
        let Some(opening) = opening else {
            if pieces
                .iter()
                .any(|piece| matches!(piece, Piece::Text(text, _) if text.contains('\n')))
            {
                self.error(span, ONE_LINE);
            }
            return pieces;
        };
        if let Some(Piece::Text(text, _)) = pieces.first_mut() {
            text.drain(..=opening);
        }
        let alone = pieces.len() == 1;
        let indent = match pieces.last_mut() {
            Some(Piece::Text(text, _)) if alone || text.contains('\n') => {
                let line = text.rfind('\n').map_or(0, |at| at + 1);
                if !text[line..].trim().is_empty() {
                    self.error(span, CLOSING);
                    return pieces;
                }
                let indent = text[line..].to_owned();
                text.truncate(line.saturating_sub(1));
                if text.ends_with('\r') {
                    text.pop();
                }
                indent
            }
            _ => {
                self.error(span, CLOSING);
                return pieces;
            }
        };
        if !dedented(&mut pieces, &indent) {
            self.error(span, SHALLOW);
        }
        pieces
    }

    fn escape(&mut self, token: Token) -> Option<char> {
        let text = self.text(token);
        match text {
            "{{" => return Some('{'),
            "}}" => return Some('}'),
            _ => {}
        }
        let mut characters = text.chars().skip(1);
        let escaped = match characters.next()? {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '0' => '\0',
            '\\' => '\\',
            '"' => '"',
            'u' => {
                let code = code_point(&text[2..]);
                if code.is_none() {
                    self.error(token.span, "Write \\u{…} with a hexadecimal code point");
                }
                return code;
            }
            other => {
                self.error(token.span, format!("Unknown escape \\{other}"));
                other
            }
        };
        Some(escaped)
    }

    fn raw(&mut self, token: Token) -> Datum {
        self.error(token.span, RAW);
        Datum::Str(String::new())
    }

    fn regex(&mut self, token: Token, closed: bool) -> Datum {
        if !closed {
            self.unfinished(token.span, "This regex is never closed");
            return Datum::Str(String::new());
        }
        let start = token.starts();
        let text = self.text(token);
        let fence = text[1..].len() - text[1..].trim_start_matches('"').len();
        let fence = if fence >= 3 { fence } else { 1 };
        let inner = Span::new(start + 1 + fence, token.ends() - fence);
        let pattern = self.source[inner.start as usize..inner.end as usize].to_owned();
        let pattern = if fence > 1 {
            let pieces = self.verbatim(vec![Piece::Text(pattern, inner)], token.span);
            match assembled(pieces, start) {
                Datum::Str(text) => text,
                _ => String::new(),
            }
        } else {
            pattern
        };
        Datum::List(vec![
            hygienic("regex", Span::new(start, start + 1)),
            Syntax::new(Datum::Str(pattern), inner),
        ])
    }

    fn interpolation(&mut self, open: Token, index: usize) -> Option<Syntax> {
        let Some(&close) = self.holes.get(&index) else {
            self.unfinished(open.span, "This { is never closed");
            self.next = self.tokens.len();
            self.at = self.source.len();
            return None;
        };
        let closer = self.tokens[close];
        let braces = Span::new(open.starts(), closer.ends());
        if self.depth >= MAX_INTERPOLATION {
            self.error(braces, "Strings nested too deeply");
        } else {
            let frames = mem::take(&mut self.frames);
            let top = mem::take(&mut self.top);
            self.depth += 1;
            self.read_all(Some(close));
            self.depth -= 1;
            self.frames = frames;
            let inner = mem::replace(&mut self.top, top);
            self.next = close + 1;
            self.at = closer.ends();
            let mut found = inner.into_iter();
            return match (found.next(), found.next()) {
                (Some(only), None) => Some(only),
                (None, _) => {
                    self.error(braces, "Put an expression inside the braces");
                    None
                }
                (Some(first), Some(_)) => {
                    self.error(braces, "Put one expression inside the braces");
                    Some(first)
                }
            };
        }
        self.next = close + 1;
        self.at = closer.ends();
        None
    }

    fn character(&mut self, token: Token) -> Datum {
        let text = self.text(token);
        let name = &text[1..];
        let found = match name.chars().next() {
            None => {
                self.error(token.span, "Name a character after \\");
                return Datum::Char(' ');
            }
            Some('u') if name.starts_with("u{") => code_point(&name[1..]),
            Some('u') if name == "u" && self.source[token.ends()..].starts_with('{') => None,
            Some(first) if delimiter(first) => Some(first),
            Some(_) => named(name),
        };
        let Some(character) = found else {
            self.error(token.span, format!("Unknown character {text}"));
            return Datum::Char(' ');
        };
        Datum::Char(character)
    }
}

fn code_point(braced: &str) -> Option<char> {
    let inner = braced.strip_prefix('{')?.strip_suffix('}')?;
    u32::from_str_radix(inner, 16).ok().and_then(char::from_u32)
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
    fn a_byte_order_mark_at_the_start_is_skipped() {
        assert_eq!(data("\u{feff}42"), [(Datum::Int(42), Span::new(3, 5))]);
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

    #[test]
    fn verbatim_blocks_drop_the_indentation_of_their_closing_fence() {
        assert_eq!(
            shapes("\"\"\"\n    one\n      two\n\n    three\n    \"\"\" \"\"\"a \"q\" \\n\"\"\""),
            [r#""one\n  two\n\nthree""#, r#""a \"q\" \\n""#]
        );
        assert_eq!(
            shapes(
                "$\"\"\"\n  Hi {name}\n  \"\"\" #\"\"\"\n  \\d+\n  \"\"\" \"\"\"\r\n  a\r\n  \"\"\""
            ),
            [r#"(str "Hi " name)"#, r#"(regex "\\d+")"#, r#""a""#]
        );
        assert_eq!(
            shapes("\"\"\"\"\n  say \"\"\"hi\"\"\"\n  \"\"\"\""),
            [r#""say \"\"\"hi\"\"\"""#]
        );
        let messages = |source: &str| -> Vec<String> {
            read(source)
                .1
                .into_iter()
                .map(|error| error.message)
                .collect()
        };
        assert_eq!(messages("\"\"\"\n  a\n b\n  \"\"\""), [SHALLOW]);
        assert_eq!(messages("\"\"\"\n  a\"\"\""), [CLOSING]);
        assert_eq!(messages("\"\"\"text\nmore\"\"\""), [ONE_LINE]);
        assert_eq!(messages("$\"a}b\""), ["Write }} for a } in a $ string"]);
    }

    #[test]
    fn a_colon_before_a_string_reads_as_any_keyword() {
        assert_eq!(
            data(r#":"Unit Price" :"a.b" : "x""#),
            [
                (keyword("Unit Price"), Span::new(0, 13)),
                (keyword("a.b"), Span::new(14, 20)),
                (symbol(":"), Span::new(21, 22)),
                (string("x"), Span::new(23, 26)),
            ]
        );
        let (_, errors) = read(":(x)");
        assert!(errors.is_empty());
        let (_, errors) = read(r#":$"{x}""#);
        assert_eq!(
            errors.first().map(|error| error.message.as_str()),
            Some("A colon makes a keyword of a plain string, as in :\"Unit Price\"")
        );
    }

    fn shapes(source: &str) -> Vec<String> {
        read(source).0.iter().map(ToString::to_string).collect()
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
    fn records_are_read_from_a_name_and_braces() {
        assert_eq!(errors("#Point{x: 1 y: [2]} (f #Pair{})"), []);
        assert_eq!(
            shapes("#Point{x: 1 y: [2]} (f #Pair{})"),
            ["#Point{:x 1 :y [2]}", "(f #Pair{})"]
        );
        assert_eq!(data("#Pair{}")[0].1, Span::new(0, 7));
        assert_eq!(
            errors("#Point{x: 1"),
            [("This #Point{ is never closed".to_owned(), Span::new(0, 7))]
        );
        assert_eq!(errors("#Point{x y z}"), []);
        assert_eq!(shapes("#Point{x y z}"), ["#Point{x y z}"]);
        assert_eq!(
            errors("#Point"),
            [("Unknown form #Point".to_owned(), Span::new(0, 6))]
        );
    }

    #[test]
    fn metadata_reads_as_an_item_of_its_own() {
        assert_eq!(
            errors("(def ^:shadow (each f xs) 1) (defmacro ^{indent: 1 defines: 0} m [] 1)"),
            []
        );
        assert_eq!(
            shapes("(def ^:shadow (each f xs) 1) (defmacro ^{indent: 1 defines: 0} m [] 1)"),
            [
                "(def ^{:shadow true} (each f xs) 1)",
                "(defmacro ^{:indent 1 :defines 0} m [] 1)"
            ]
        );
        let (forms, _) = read("(def ^:shadow x 1)");
        let Datum::List(items) = &forms[0].datum else {
            panic!("expected a list");
        };
        assert_eq!(items[1].span, Span::new(5, 13));
        assert_eq!(
            errors("^x ^ (f) ^{a: 1"),
            [
                (
                    "^ takes a keyword or a map, as in ^:shadow or ^{indent: 1}".to_owned(),
                    Span::new(0, 1)
                ),
                (
                    "^ takes a keyword or a map, as in ^:shadow or ^{indent: 1}".to_owned(),
                    Span::new(3, 4)
                ),
                ("This ^{ is never closed".to_owned(), Span::new(9, 11)),
            ]
        );
    }

    #[test]
    fn maps_with_an_odd_number_of_items_still_read() {
        assert_eq!(errors("{a: 1 b:} {name age city}"), []);
        assert_eq!(
            shapes("{a: 1 b:} {name age city}"),
            ["{:a 1 :b}", "{name age city}"]
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
            shapes(r#"$"Hi {name}!" $"{{x}} {(+ 1 2)}" $"{city.name}" "plain}" "a {b} c""#),
            [
                r#"(str "Hi " name "!")"#,
                r#"(str "{x} " (+ 1 2))"#,
                "(str (. city :name))",
                r#""plain}""#,
                r#""a {b} c""#,
            ]
        );
    }

    #[test]
    fn interpolated_expressions_keep_their_spans_and_may_hold_strings() {
        let source = r#"$"a{(f "}")}b""#;
        let (data, errors) = read(source);
        let Datum::List(pieces) = &data[0].datum else {
            panic!("expected an interpolation");
        };

        assert_eq!(errors, []);
        assert_eq!(data[0].to_string(), r#"(str "a" (f "}") "b")"#);
        assert_eq!(data[0].span, Span::new(0, 14));
        assert_eq!(pieces[2].span, Span::new(4, 11));
    }

    #[test]
    fn holes_may_hold_verbatim_text_regexes_and_comments_with_braces_and_quotes() {
        let source = r#"$"<{(f """a"}b""" #"\d{2}" #| } |# x)}>""#;

        assert_eq!(errors(source), []);
        let (data, _) = read(source);
        let Datum::List(pieces) = &data[0].datum else {
            panic!("expected an interpolation");
        };
        let Datum::List(call) = &pieces[2].datum else {
            panic!("expected the call inside the braces");
        };
        assert_eq!(call[1].datum, string("a\"}b"));
        assert_eq!(call[3].datum, symbol("x"));
        assert_eq!(pieces[3].datum, string(">"));
    }

    #[test]
    fn interpolation_mistakes_are_reported() {
        assert_eq!(
            errors(r#"$"{}" $"{a b}" $"{open""#),
            [
                (
                    "Put an expression inside the braces".to_owned(),
                    Span::new(2, 4)
                ),
                (
                    "Put one expression inside the braces".to_owned(),
                    Span::new(8, 13)
                ),
                ("This { is never closed".to_owned(), Span::new(17, 18)),
                ("This string is never closed".to_owned(), Span::new(15, 23)),
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
        assert_eq!(flags(r#"$"{"#), [true, true]);
        assert_eq!(flags("\"\"\"\nopen"), [true]);
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
