use crate::span::Span;

const BOM: char = '\u{feff}';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bracket {
    List,
    Vector,
    Map,
    Set,
    Record,
    Meta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Prefix {
    Quote,
    Quasiquote,
    Unquote,
    Splice,
    Deref,
    Skip,
    Keyword,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Space,
    Comment,
    BlockComment { closed: bool },
    Open(Bracket),
    Close,
    Prefix(Prefix),
    Flag,
    Caret,
    StringStart,
    Text,
    Escape,
    HoleStart,
    HoleEnd,
    StringEnd,
    Raw { closed: bool },
    Regex { closed: bool },
    Char,
    Number,
    Keyword,
    Symbol,
    Dispatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token {
    pub kind: Kind,
    pub span: Span,
}

impl Token {
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        source
            .get(self.span.start as usize..self.span.end as usize)
            .unwrap_or_default()
    }

    pub fn starts(&self) -> usize {
        self.span.start as usize
    }

    pub fn ends(&self) -> usize {
        self.span.end as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Context {
    String,
    Hole(usize),
    Raw(usize),
    Regex { escaped: bool },
    Block(usize),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct State(Vec<Context>);

impl State {
    pub fn is_plain(&self) -> bool {
        self.0.is_empty()
    }

    pub fn in_code(&self) -> bool {
        matches!(self.0.last(), None | Some(Context::Hole(_)))
    }

    pub fn in_string(&self) -> bool {
        matches!(
            self.0.last(),
            Some(Context::String | Context::Raw(_) | Context::Regex { .. })
        )
    }

    pub fn in_comment(&self) -> bool {
        matches!(self.0.last(), Some(Context::Block(_)))
    }
}

pub fn separator(character: char) -> bool {
    character.is_whitespace() || character == ','
}

pub fn delimiter(character: char) -> bool {
    separator(character) || matches!(character, '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';')
}

pub fn numeric(token: &str) -> bool {
    token
        .strip_prefix(['-', '+'])
        .unwrap_or(token)
        .starts_with(|character: char| character.is_ascii_digit())
}

fn keyword(token: &str) -> bool {
    token.strip_prefix(':').is_some_and(|name| !name.is_empty())
        || token
            .strip_suffix(':')
            .is_some_and(|name| !name.is_empty() && !name.contains(':'))
}

pub fn tokens(source: &str, state: &State) -> (Vec<Token>, State) {
    let mut lexer = Lexer::new(source, state.clone());
    let found = lexer.by_ref().collect();
    (found, lexer.state())
}

pub fn state_after(source: &str, state: &State) -> State {
    let mut lexer = Lexer::new(source, state.clone());
    lexer.by_ref().for_each(drop);
    lexer.state()
}

#[derive(Debug, Clone)]
pub struct Lexer<'a> {
    source: &'a str,
    at: usize,
    stack: Vec<Context>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str, state: State) -> Self {
        Self {
            source,
            at: 0,
            stack: state.0,
        }
    }

    pub fn state(&self) -> State {
        State(self.stack.clone())
    }

    fn rest(&self) -> &'a str {
        &self.source[self.at..]
    }

    fn emit(&mut self, kind: Kind, start: usize) -> Token {
        Token {
            kind,
            span: Span::new(start, self.at),
        }
    }

    fn advance(&mut self, bytes: usize) {
        self.at = (self.at + bytes).min(self.source.len());
    }

    fn advance_char(&mut self) -> Option<char> {
        let character = self.rest().chars().next()?;
        self.at += character.len_utf8();
        Some(character)
    }

    fn word(&mut self) -> &'a str {
        let rest = self.rest();
        let length = rest.find(delimiter).unwrap_or(rest.len());
        self.at += length;
        &rest[..length]
    }

    fn block(&mut self, start: usize, mut depth: usize) -> Token {
        while depth > 0 {
            let rest = self.rest();
            if rest.starts_with("#|") {
                depth += 1;
                self.at += 2;
            } else if rest.starts_with("|#") {
                depth -= 1;
                self.at += 2;
            } else if self.advance_char().is_none() {
                self.stack.push(Context::Block(depth));
                return self.emit(Kind::BlockComment { closed: false }, start);
            }
        }
        self.emit(Kind::BlockComment { closed: true }, start)
    }

    fn raw(&mut self, start: usize, hashes: usize) -> Token {
        let closing = format!("\"{}", "#".repeat(hashes));
        if let Some(length) = self.rest().find(&closing) {
            self.at += length + closing.len();
            return self.emit(Kind::Raw { closed: true }, start);
        }
        self.at = self.source.len();
        self.stack.push(Context::Raw(hashes));
        self.emit(Kind::Raw { closed: false }, start)
    }

    fn regex(&mut self, start: usize, mut escaped: bool) -> Token {
        while let Some(character) = self.advance_char() {
            if character == '"' && !escaped {
                return self.emit(Kind::Regex { closed: true }, start);
            }
            escaped = character == '\\' && !escaped;
        }
        self.stack.push(Context::Regex { escaped });
        self.emit(Kind::Regex { closed: false }, start)
    }

    fn string(&mut self, start: usize) -> Token {
        let rest = self.rest();
        if rest.starts_with('"') {
            self.at += 1;
            self.stack.pop();
            return self.emit(Kind::StringEnd, start);
        }
        if rest.starts_with('\\') {
            self.at += 1;
            if self.rest().starts_with("u{") {
                match self.rest().find('}') {
                    Some(close) => self.at += close + 1,
                    None => self.at += 1,
                }
            } else {
                self.advance_char();
            }
            return self.emit(Kind::Escape, start);
        }
        if rest.starts_with("{{") || rest.starts_with("}}") {
            self.at += 2;
            return self.emit(Kind::Escape, start);
        }
        if rest.starts_with('{') {
            self.at += 1;
            self.stack.push(Context::Hole(0));
            return self.emit(Kind::HoleStart, start);
        }
        let mut characters = rest.char_indices().peekable();
        let mut length = rest.len();
        while let Some((offset, character)) = characters.next() {
            let ends = matches!(character, '"' | '\\' | '{')
                || (character == '}' && characters.peek().is_some_and(|(_, next)| *next == '}'));
            if ends {
                length = offset;
                break;
            }
        }
        self.at += length;
        self.emit(Kind::Text, start)
    }

    fn opened(&mut self, bracket: Bracket, length: usize, start: usize) -> Token {
        self.advance(length);
        if let Some(Context::Hole(depth)) = self.stack.last_mut() {
            *depth += 1;
        }
        self.emit(Kind::Open(bracket), start)
    }

    fn closed(&mut self, start: usize) -> Token {
        let closer = self.rest().starts_with('}');
        self.at += 1;
        match self.stack.last_mut() {
            Some(Context::Hole(0)) if closer => {
                self.stack.pop();
                return self.emit(Kind::HoleEnd, start);
            }
            Some(Context::Hole(depth)) => *depth = depth.saturating_sub(1),
            _ => {}
        }
        self.emit(Kind::Close, start)
    }

    fn character(&mut self, start: usize) -> Token {
        self.at += 1;
        let rest = self.rest();
        match rest.chars().next() {
            None => {}
            Some('u') if rest[1..].starts_with('{') => match rest.find('}') {
                Some(close) => self.at += close + 1,
                None => self.at += 1,
            },
            Some(first) if delimiter(first) => self.at += first.len_utf8(),
            Some(_) => {
                self.word();
            }
        }
        self.emit(Kind::Char, start)
    }

    fn hash(&mut self, start: usize) -> Token {
        let rest = self.rest();
        if rest.starts_with("#r\"") || rest.starts_with("#r#") {
            self.at += 2;
            let hashes = self.rest().chars().take_while(|c| *c == '#').count();
            self.at += hashes;
            if !self.rest().starts_with('"') {
                return self.emit(Kind::Raw { closed: false }, start);
            }
            self.at += 1;
            return self.raw(start, hashes);
        }
        if rest.starts_with("#\"") {
            self.at += 2;
            return self.regex(start, false);
        }
        if rest.starts_with("#|") {
            self.at += 2;
            return self.block(start, 1);
        }
        if rest.starts_with("#;") {
            self.at += 2;
            return self.emit(Kind::Prefix(Prefix::Skip), start);
        }
        if rest.starts_with("#{") {
            return self.opened(Bracket::Set, 2, start);
        }
        let name = self.word();
        if name.len() > 1 && self.rest().starts_with('{') {
            return self.opened(Bracket::Record, 1, start);
        }
        self.emit(Kind::Dispatch, start)
    }

    fn caret(&mut self, start: usize) -> Token {
        if self.rest().starts_with("^{") {
            return self.opened(Bracket::Meta, 2, start);
        }
        self.at += 1;
        let after = self.at;
        let word = self.word();
        if word.strip_prefix(':').is_some_and(|name| !name.is_empty()) {
            return self.emit(Kind::Flag, start);
        }
        self.at = after;
        self.emit(Kind::Caret, start)
    }

    fn code(&mut self, start: usize) -> Option<Token> {
        let rest = self.rest();
        let first = rest.chars().next()?;
        if start == 0 && first == BOM {
            self.at += BOM.len_utf8();
            return Some(self.emit(Kind::Space, start));
        }
        if separator(first) {
            self.at += rest.len() - rest.trim_start_matches(separator).len();
            return Some(self.emit(Kind::Space, start));
        }
        let prefix = |prefix| Some((prefix, 1));
        let marked = match first {
            '\'' => prefix(Prefix::Quote),
            '`' => prefix(Prefix::Quasiquote),
            '~' if rest.starts_with("~@") => Some((Prefix::Splice, 2)),
            '~' => prefix(Prefix::Unquote),
            '@' => prefix(Prefix::Deref),
            ':' if rest.starts_with(":\"") => prefix(Prefix::Keyword),
            _ => None,
        };
        if let Some((prefix, length)) = marked {
            self.at += length;
            return Some(self.emit(Kind::Prefix(prefix), start));
        }
        Some(match first {
            ';' => {
                self.at += rest.find('\n').unwrap_or(rest.len());
                self.emit(Kind::Comment, start)
            }
            '(' => self.opened(Bracket::List, 1, start),
            '[' => self.opened(Bracket::Vector, 1, start),
            '{' => self.opened(Bracket::Map, 1, start),
            ')' | ']' | '}' => self.closed(start),
            '"' => {
                self.at += 1;
                self.stack.push(Context::String);
                self.emit(Kind::StringStart, start)
            }
            '\\' => self.character(start),
            '#' => self.hash(start),
            '^' => self.caret(start),
            _ => {
                let word = self.word();
                let kind = if numeric(word) {
                    Kind::Number
                } else if keyword(word) {
                    Kind::Keyword
                } else {
                    Kind::Symbol
                };
                self.emit(kind, start)
            }
        })
    }
}

impl Iterator for Lexer<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        let start = self.at;
        if start >= self.source.len() {
            return None;
        }
        match self.stack.last().copied() {
            None | Some(Context::Hole(_)) => self.code(start),
            Some(Context::String) => Some(self.string(start)),
            Some(Context::Raw(hashes)) => {
                self.stack.pop();
                Some(self.raw(start, hashes))
            }
            Some(Context::Regex { escaped }) => {
                self.stack.pop();
                Some(self.regex(start, escaped))
            }
            Some(Context::Block(depth)) => {
                self.stack.pop();
                Some(self.block(start, depth))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<(Kind, &str)> {
        tokens(source, &State::default())
            .0
            .into_iter()
            .map(|token| (token.kind, token.text(source)))
            .filter(|(kind, _)| *kind != Kind::Space)
            .collect()
    }

    #[test]
    fn code_splits_into_brackets_atoms_and_prefixes() {
        assert_eq!(
            kinds("(f [1 :a b:] {x 2.5} #{s} #Point{} ^{m 1} ^:flag 'q `(~a ~@b) @c #;d)"),
            [
                (Kind::Open(Bracket::List), "("),
                (Kind::Symbol, "f"),
                (Kind::Open(Bracket::Vector), "["),
                (Kind::Number, "1"),
                (Kind::Keyword, ":a"),
                (Kind::Keyword, "b:"),
                (Kind::Close, "]"),
                (Kind::Open(Bracket::Map), "{"),
                (Kind::Symbol, "x"),
                (Kind::Number, "2.5"),
                (Kind::Close, "}"),
                (Kind::Open(Bracket::Set), "#{"),
                (Kind::Symbol, "s"),
                (Kind::Close, "}"),
                (Kind::Open(Bracket::Record), "#Point{"),
                (Kind::Close, "}"),
                (Kind::Open(Bracket::Meta), "^{"),
                (Kind::Symbol, "m"),
                (Kind::Number, "1"),
                (Kind::Close, "}"),
                (Kind::Flag, "^:flag"),
                (Kind::Prefix(Prefix::Quote), "'"),
                (Kind::Symbol, "q"),
                (Kind::Prefix(Prefix::Quasiquote), "`"),
                (Kind::Open(Bracket::List), "("),
                (Kind::Prefix(Prefix::Unquote), "~"),
                (Kind::Symbol, "a"),
                (Kind::Prefix(Prefix::Splice), "~@"),
                (Kind::Symbol, "b"),
                (Kind::Close, ")"),
                (Kind::Prefix(Prefix::Deref), "@"),
                (Kind::Symbol, "c"),
                (Kind::Prefix(Prefix::Skip), "#;"),
                (Kind::Symbol, "d"),
                (Kind::Close, ")"),
            ]
        );
        let found: Vec<Kind> = tokens(r#":"a b""#, &State::default())
            .0
            .iter()
            .map(|token| token.kind)
            .collect();
        assert_eq!(
            found,
            [
                Kind::Prefix(Prefix::Keyword),
                Kind::StringStart,
                Kind::Text,
                Kind::StringEnd
            ]
        );
    }

    #[test]
    fn commas_separate_and_comments_are_kept() {
        assert_eq!(
            kinds("a,b ; note\n#| x #| y |# |# c"),
            [
                (Kind::Symbol, "a"),
                (Kind::Symbol, "b"),
                (Kind::Comment, "; note"),
                (Kind::BlockComment { closed: true }, "#| x #| y |# |#"),
                (Kind::Symbol, "c"),
            ]
        );
    }

    #[test]
    fn strings_split_into_text_escapes_and_holes_that_may_hold_strings() {
        assert_eq!(
            kinds(r#""a\n{{ {(f "}")} b}""#),
            [
                (Kind::StringStart, "\""),
                (Kind::Text, "a"),
                (Kind::Escape, "\\n"),
                (Kind::Escape, "{{"),
                (Kind::Text, " "),
                (Kind::HoleStart, "{"),
                (Kind::Open(Bracket::List), "("),
                (Kind::Symbol, "f"),
                (Kind::StringStart, "\""),
                (Kind::Text, "}"),
                (Kind::StringEnd, "\""),
                (Kind::Close, ")"),
                (Kind::HoleEnd, "}"),
                (Kind::Text, " b}"),
                (Kind::StringEnd, "\""),
            ]
        );
    }

    #[test]
    fn maps_inside_holes_close_before_the_hole_does() {
        assert_eq!(
            kinds(r#""{(get {a: 1} :a)}""#)
                .into_iter()
                .map(|(kind, _)| kind)
                .filter(|kind| matches!(kind, Kind::Close | Kind::HoleEnd))
                .collect::<Vec<_>>(),
            [Kind::Close, Kind::Close, Kind::HoleEnd]
        );
    }

    #[test]
    fn raw_strings_regexes_and_characters_are_whole_tokens() {
        assert_eq!(
            kinds("#r#\"say \"hi\"\"# #r\"C:\\dir\\\" #\"\\d{4}\\\"\" \\( \\newline \\u{41}"),
            [
                (Kind::Raw { closed: true }, "#r#\"say \"hi\"\"#"),
                (Kind::Raw { closed: true }, r#"#r"C:\dir\""#),
                (Kind::Regex { closed: true }, r#"#"\d{4}\"""#),
                (Kind::Char, r"\("),
                (Kind::Char, r"\newline"),
                (Kind::Char, r"\u{41}"),
            ]
        );
    }

    #[test]
    fn unknown_hash_forms_and_bare_carets_stand_out() {
        assert_eq!(
            kinds("#x ^ y #r#! #"),
            [
                (Kind::Dispatch, "#x"),
                (Kind::Caret, "^"),
                (Kind::Symbol, "y"),
                (Kind::Raw { closed: false }, "#r#"),
                (Kind::Symbol, "!"),
                (Kind::Dispatch, "#"),
            ]
        );
    }

    #[test]
    fn lines_resume_where_the_last_one_stopped() {
        let lines = [
            "(def doc \"one {(f",
            " \"x\")} two",
            "three\" #| a",
            "b |# #r#\"c",
            "d\"#)",
        ];
        let mut state = State::default();
        let mut seen = Vec::new();
        for line in lines {
            let (found, after) = tokens(line, &state);
            seen.push(
                found
                    .into_iter()
                    .map(|token| (token.kind, token.text(line).to_owned()))
                    .filter(|(kind, _)| *kind != Kind::Space)
                    .collect::<Vec<_>>(),
            );
            state = after;
        }

        let text = |kind, text: &str| (kind, text.to_owned());
        assert_eq!(seen[1][0], text(Kind::StringStart, "\""));
        assert_eq!(seen[1][4], text(Kind::HoleEnd, "}"));
        assert_eq!(seen[1][5], text(Kind::Text, " two"));
        assert_eq!(seen[2][0], text(Kind::Text, "three"));
        assert_eq!(
            seen[2][2],
            text(Kind::BlockComment { closed: false }, "#| a")
        );
        assert_eq!(
            seen[3][0],
            text(Kind::BlockComment { closed: true }, "b |#")
        );
        assert_eq!(seen[3][1], text(Kind::Raw { closed: false }, "#r#\"c"));
        assert_eq!(seen[4][0], text(Kind::Raw { closed: true }, "d\"#"));
        assert_eq!(seen[4][1], text(Kind::Close, ")"));
        assert_eq!(state, State::default());
    }

    #[test]
    fn the_state_says_what_a_line_ends_inside() {
        let after = |source| state_after(source, &State::default());

        assert!(after("(a b").in_code());
        assert!(after("\"open").in_string());
        assert!(after("\"x {(f").in_code());
        assert!(after("#\"re").in_string());
        assert!(after("#| c").in_comment());
    }

    #[test]
    fn every_byte_belongs_to_a_token() {
        let source = "\u{feff}(a \"b{c}\" ; d\n #|e|# \\f #r\"g\" #\"h\")";
        let (found, _) = tokens(source, &State::default());
        let mut at = 0;
        for token in found {
            assert_eq!(token.starts(), at, "{token:?}");
            at = token.ends();
        }
        assert_eq!(at, source.len());
    }
}
