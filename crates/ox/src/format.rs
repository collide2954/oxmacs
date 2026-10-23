use crate::read::{self, Datum, Syntax};
use crate::span::line_column;

const BODIES: &[&str] = &[
    "def",
    "defmacro",
    "defrecord",
    "deferror",
    "defonce",
    "defcommand",
    "defmode",
    "defrule",
    "defrules",
    "fn",
    "let",
    "when",
    "unless",
    "when-let",
    "if-let",
    "if",
    "do",
    "try",
    "catch",
    "finally",
    "match",
    "cond",
    "module",
    "test",
    "gen",
    "spawn",
    "atomically",
    "maybe",
    "plugin",
    "use-plugin",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    List,
    Collection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Open(Shape),
    Close,
    Atom,
    Continued,
    Comment,
}

#[derive(Debug, Clone)]
struct Piece {
    kind: Kind,
    text: String,
    gap: String,
}

#[derive(Debug, Clone, Copy)]
struct Literal {
    start: usize,
    end: usize,
    closed: bool,
}

#[derive(Debug, Clone)]
struct Open {
    shape: Shape,
    column: usize,
    line: usize,
    items: usize,
    head: Option<String>,
    first: Option<usize>,
    argument: Option<usize>,
}

#[derive(Debug, Default)]
struct State {
    stack: Vec<Open>,
}

fn body(head: &str) -> bool {
    BODIES.contains(&head)
        || ["def", "with-", "when-", "if-"]
            .iter()
            .any(|prefix| head.starts_with(prefix))
}

fn delimits(character: char) -> bool {
    character.is_whitespace() || "()[]{}\";".contains(character)
}

fn data(head: &str) -> bool {
    head.starts_with(|character: char| character.is_ascii_digit() || character == '"')
}

fn interpolation_end(chars: &[char], mut at: usize) -> (usize, bool) {
    let mut depth = 0_usize;
    while at < chars.len() {
        match chars[at] {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            '}' if depth == 0 => return (at + 1, true),
            '}' => depth -= 1,
            '"' => {
                let (end, closed) = string_end(chars, at + 1);
                if !closed {
                    return (end, false);
                }
                at = end;
                continue;
            }
            '\\' => at += 1,
            _ => {}
        }
        at += 1;
    }
    (chars.len(), false)
}

fn string_end(chars: &[char], mut at: usize) -> (usize, bool) {
    while at < chars.len() {
        match chars[at] {
            '"' => return (at + 1, true),
            '\\' => at += 2,
            '{' if chars.get(at + 1) == Some(&'{') => at += 2,
            '}' if chars.get(at + 1) == Some(&'}') => at += 2,
            '{' => {
                let (end, closed) = interpolation_end(chars, at + 1);
                if !closed {
                    return (end, false);
                }
                at = end;
            }
            _ => at += 1,
        }
    }
    (chars.len(), false)
}

fn regex_end(chars: &[char], mut at: usize) -> (usize, bool) {
    while at < chars.len() {
        match chars[at] {
            '"' => return (at + 1, true),
            '\\' => at += 2,
            _ => at += 1,
        }
    }
    (chars.len(), false)
}

fn raw_end(chars: &[char], at: usize, hashes: usize) -> (usize, bool) {
    let mut at = at;
    while at < chars.len() {
        let closes = chars[at] == '"'
            && chars[at + 1..]
                .iter()
                .take(hashes)
                .filter(|found| **found == '#')
                .count()
                == hashes;
        if closes {
            return (at + 1 + hashes, true);
        }
        at += 1;
    }
    (chars.len(), false)
}

fn literals(chars: &[char]) -> Vec<Literal> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let start = at;
        let ended = match chars[at] {
            ';' => {
                while at < chars.len() && chars[at] != '\n' {
                    at += 1;
                }
                continue;
            }
            '\\' => {
                at += 2;
                continue;
            }
            '"' => Some(string_end(chars, at + 1)),
            '#' if chars.get(at + 1) == Some(&'"') => Some(regex_end(chars, at + 2)),
            '#' if chars.get(at + 1) == Some(&'r') => {
                let hashes = chars[at + 2..]
                    .iter()
                    .take_while(|found| **found == '#')
                    .count();
                (chars.get(at + 2 + hashes) == Some(&'"'))
                    .then(|| raw_end(chars, at + 3 + hashes, hashes))
            }
            _ => None,
        };
        match ended {
            Some((end, closed)) => {
                found.push(Literal { start, end, closed });
                at = end;
            }
            None => at += 1,
        }
    }
    found
}

fn literal_starting(literals: &[Literal], at: usize) -> Option<Literal> {
    literals
        .binary_search_by_key(&at, |literal| literal.start)
        .ok()
        .map(|index| literals[index])
}

fn literal_around(literals: &[Literal], at: usize) -> Option<Literal> {
    let index = literals.partition_point(|literal| literal.start < at);
    index
        .checked_sub(1)
        .map(|index| literals[index])
        .filter(|literal| at < literal.end)
}

fn pieces(chars: &[char], start: usize, end: usize, literals: &[Literal]) -> Vec<Piece> {
    let mut found = Vec::new();
    let mut at = start;
    if let Some(literal) = literal_around(literals, start) {
        at = literal.end.min(end);
        found.push(Piece {
            kind: Kind::Continued,
            text: chars[start..at].iter().collect(),
            gap: String::new(),
        });
    }
    while at < end {
        let gap_start = at;
        while at < end && chars[at].is_whitespace() {
            at += 1;
        }
        if at >= end {
            break;
        }
        let gap: String = chars[gap_start..at].iter().collect();
        let token = at;
        if chars[at] == ';' {
            found.push(Piece {
                kind: Kind::Comment,
                text: chars[at..end].iter().collect(),
                gap,
            });
            break;
        }
        while at < end
            && literal_starting(literals, at).is_none()
            && ("'`~@".contains(chars[at]) || (chars[at] == '#' && chars.get(at + 1) == Some(&';')))
        {
            at += if chars[at] == '#' { 2 } else { 1 };
        }
        let kind = if let Some(literal) = literal_starting(literals, at) {
            at = literal.end.min(end);
            Kind::Atom
        } else {
            match chars.get(at).filter(|_| at < end) {
                Some('(') => {
                    at += 1;
                    Kind::Open(Shape::List)
                }
                Some('[' | '{') => {
                    at += 1;
                    Kind::Open(Shape::Collection)
                }
                Some('#') if chars.get(at + 1) == Some(&'{') => {
                    at += 2;
                    Kind::Open(Shape::Collection)
                }
                Some(')' | ']' | '}') => {
                    at += 1;
                    Kind::Close
                }
                Some('\\') => {
                    at = (at + 2).min(end);
                    while at < end && !delimits(chars[at]) {
                        at += 1;
                    }
                    Kind::Atom
                }
                Some(_) => {
                    at += 1;
                    while at < end && !delimits(chars[at]) {
                        at += 1;
                    }
                    Kind::Atom
                }
                None => Kind::Atom,
            }
        };
        found.push(Piece {
            kind,
            text: chars[token..at].iter().collect(),
            gap,
        });
    }
    found
}

fn render(pieces: &[Piece], tidy: bool) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut columns = Vec::with_capacity(pieces.len());
    let mut column = 0;
    let mut last: Option<Kind> = None;
    for piece in pieces {
        let gap = if !tidy {
            piece.gap.clone()
        } else if matches!(last, None | Some(Kind::Open(_))) || piece.kind == Kind::Close {
            String::new()
        } else if piece.gap.is_empty() {
            " ".to_owned()
        } else {
            piece.gap.clone()
        };
        column += gap.chars().count();
        text.push_str(&gap);
        columns.push(column);
        column += piece.text.chars().count();
        text.push_str(&piece.text);
        last = Some(piece.kind);
    }
    (text, columns)
}

impl State {
    fn indentation(&self) -> usize {
        let Some(top) = self.stack.last() else {
            return 0;
        };
        let aligned = top.first.unwrap_or(top.column + 1);
        match top.shape {
            Shape::Collection => aligned,
            Shape::List => match top.head.as_deref() {
                None => top.column + 1,
                Some(head) if body(head) => top.column + 2,
                Some(head) if data(head) => aligned,
                Some(_) => top.argument.unwrap_or(top.column + 1),
            },
        }
    }

    fn element(&mut self, column: usize, line: usize, text: &str) {
        let Some(top) = self.stack.last_mut() else {
            return;
        };
        top.items += 1;
        let same = line == top.line;
        match top.items {
            1 => {
                top.head = Some(text.trim_start_matches(['\'', '`', '~', '@']).to_owned());
                top.first = same.then_some(column);
            }
            2 if same => top.argument = Some(column),
            _ => {}
        }
    }

    fn feed(&mut self, pieces: &[Piece], columns: &[usize], line: usize) {
        for (piece, column) in pieces.iter().zip(columns) {
            match piece.kind {
                Kind::Open(shape) => {
                    self.element(*column, line, "(");
                    self.stack.push(Open {
                        shape,
                        column: column + piece.text.chars().count() - 1,
                        line,
                        items: 0,
                        head: None,
                        first: None,
                        argument: None,
                    });
                }
                Kind::Close => {
                    self.stack.pop();
                }
                Kind::Atom => self.element(*column, line, &piece.text),
                Kind::Continued | Kind::Comment => {}
            }
        }
    }
}

fn lines(chars: &[char]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut start = 0;
    for (at, character) in chars.iter().enumerate() {
        if *character == '\n' {
            let end = if at > start && chars[at - 1] == '\r' {
                at - 1
            } else {
                at
            };
            found.push((start, end));
            start = at + 1;
        }
    }
    if start < chars.len() {
        found.push((start, chars.len()));
    }
    found
}

fn same(before: &Syntax, after: &Syntax) -> bool {
    let all = |left: &[Syntax], right: &[Syntax]| {
        left.len() == right.len() && left.iter().zip(right).all(|(one, other)| same(one, other))
    };
    match (&before.datum, &after.datum) {
        (Datum::List(left), Datum::List(right))
        | (Datum::Vector(left), Datum::Vector(right))
        | (Datum::Map(left), Datum::Map(right))
        | (Datum::Set(left), Datum::Set(right)) => all(left, right),
        (Datum::Record(name, left), Datum::Record(other, right)) => {
            name == other && all(left, right)
        }
        (one, other) => one == other,
    }
}

pub fn indentation(before: &str) -> Option<usize> {
    let chars: Vec<char> = before.chars().collect();
    let found = literals(&chars);
    if found.last().is_some_and(|literal| !literal.closed) {
        return None;
    }
    let mut state = State::default();
    for (line, (start, end)) in lines(&chars).into_iter().enumerate() {
        let pieces = pieces(&chars, start, end, &found);
        let columns = raw_columns(&pieces);
        state.feed(&pieces, &columns, line);
    }
    Some(state.indentation())
}

fn raw_columns(pieces: &[Piece]) -> Vec<usize> {
    render(pieces, false).1
}

pub fn format(source: &str) -> Result<String, String> {
    let (forms, errors) = read::read(source);
    if let Some(error) = errors.first() {
        let (line, column) = line_column(source, error.span.start);
        return Err(format!("{line}:{column}: {}", error.message));
    }
    let out = layout(source);
    let (again, _) = read::read(&out);
    let kept = again.len() == forms.len() && forms.iter().zip(&again).all(|(a, b)| same(a, b));
    if kept {
        Ok(out)
    } else {
        Err("Formatting would change what this code means, so it was left alone".to_owned())
    }
}

fn layout(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let found = literals(&chars);
    let mut state = State::default();
    let mut out: Vec<String> = Vec::new();
    let mut commented = false;
    for (start, end) in lines(&chars) {
        let line = out.len();
        if literal_around(&found, start).is_some_and(|literal| literal.start < start) {
            let pieces = pieces(&chars, start, end, &found);
            state.feed(&pieces, &raw_columns(&pieces), line);
            out.push(chars[start..end].iter().collect());
            commented = pieces
                .last()
                .is_some_and(|piece| piece.kind == Kind::Comment);
            continue;
        }
        let mut first = start;
        while first < end && chars[first].is_whitespace() {
            first += 1;
        }
        let mut last = end;
        while last > first && chars[last - 1].is_whitespace() {
            last -= 1;
        }
        if first == last {
            if out.last().is_some_and(|previous| !previous.is_empty()) {
                out.push(String::new());
            }
            continue;
        }
        let pieces = pieces(&chars, first, last, &found);
        let closers = pieces.iter().all(|piece| piece.kind == Kind::Close);
        if closers && !state.stack.is_empty() && !commented {
            while out.last().is_some_and(String::is_empty) {
                out.pop();
            }
            let line = out.len().saturating_sub(1);
            if let Some(previous) = out.last_mut() {
                let (text, columns) = render(&pieces, true);
                let offset = previous.chars().count();
                let columns: Vec<usize> = columns.iter().map(|column| column + offset).collect();
                previous.push_str(&text);
                state.feed(&pieces, &columns, line);
                continue;
            }
        }
        let indent = state.indentation();
        let (text, columns) = render(&pieces, true);
        let columns: Vec<usize> = columns.iter().map(|column| column + indent).collect();
        state.feed(&pieces, &columns, line);
        commented = pieces
            .last()
            .is_some_and(|piece| piece.kind == Kind::Comment);
        out.push(" ".repeat(indent) + &text);
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}
