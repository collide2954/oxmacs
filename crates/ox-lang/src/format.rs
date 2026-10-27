use crate::forms::Forms;
use crate::read::{self, Datum, Syntax};
use crate::span::line_column;
use crate::syntax::{self, Bracket, tokens};

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
struct Unit {
    kind: Kind,
    start: usize,
    end: usize,
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

#[derive(Debug)]
struct State<'f> {
    stack: Vec<Open>,
    forms: &'f Forms,
}

fn data(head: &str) -> bool {
    head.starts_with(|character: char| character.is_ascii_digit() || character == '"')
}

fn units(source: &str) -> (Vec<Unit>, syntax::State) {
    let (found, after) = tokens(source, &syntax::State::default());
    let mut units = Vec::new();
    let mut prefix: Option<usize> = None;
    let mut index = 0;
    while let Some(token) = found.get(index).copied() {
        index += 1;
        let (kind, end) = match token.kind {
            syntax::Kind::Space => {
                if let Some(start) = prefix.take() {
                    units.push(Unit {
                        kind: Kind::Atom,
                        start,
                        end: token.starts(),
                    });
                }
                continue;
            }
            syntax::Kind::Prefix(_) => {
                prefix.get_or_insert(token.starts());
                continue;
            }
            syntax::Kind::Comment => (Kind::Comment, token.ends()),
            syntax::Kind::Open(Bracket::List) => (Kind::Open(Shape::List), token.ends()),
            syntax::Kind::Open(_) => (Kind::Open(Shape::Collection), token.ends()),
            syntax::Kind::Close | syntax::Kind::HoleEnd => (Kind::Close, token.ends()),
            syntax::Kind::StringStart => {
                let mut depth = 1_usize;
                let mut end = token.ends();
                while depth > 0
                    && let Some(inner) = found.get(index)
                {
                    index += 1;
                    end = inner.ends();
                    match inner.kind {
                        syntax::Kind::StringStart => depth += 1,
                        syntax::Kind::StringEnd => depth -= 1,
                        _ => {}
                    }
                }
                (Kind::Atom, end)
            }
            _ => (Kind::Atom, token.ends()),
        };
        let start = prefix.take().unwrap_or(token.starts());
        units.push(Unit { kind, start, end });
    }
    if let Some(start) = prefix {
        units.push(Unit {
            kind: Kind::Atom,
            start,
            end: source.len(),
        });
    }
    (units, after)
}

fn continuing(units: &[Unit], at: usize) -> Option<Unit> {
    let index = units.partition_point(|unit| unit.start < at);
    index
        .checked_sub(1)
        .map(|index| units[index])
        .filter(|unit| at < unit.end)
}

fn pieces(source: &str, start: usize, end: usize, units: &[Unit]) -> Vec<Piece> {
    let mut found = Vec::new();
    let mut at = start;
    if let Some(unit) = continuing(units, start) {
        at = unit.end.min(end);
        found.push(Piece {
            kind: Kind::Continued,
            text: source[start..at].to_owned(),
            gap: String::new(),
        });
    }
    let first = units.partition_point(|unit| unit.start < at);
    for unit in &units[first..] {
        if unit.start >= end {
            break;
        }
        let stop = unit.end.min(end);
        found.push(Piece {
            kind: unit.kind,
            text: source[unit.start..stop].to_owned(),
            gap: source[at..unit.start].to_owned(),
        });
        at = stop;
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

impl<'f> State<'f> {
    fn new(forms: &'f Forms) -> Self {
        Self {
            stack: Vec::new(),
            forms,
        }
    }

    fn indentation(&self) -> usize {
        let Some(top) = self.stack.last() else {
            return 0;
        };
        let aligned = top.first.unwrap_or(top.column + 1);
        match top.shape {
            Shape::Collection => aligned,
            Shape::List => match top.head.as_deref() {
                None => top.column + 1,
                Some(head) if self.forms.body(head) => top.column + 2,
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

fn lines(source: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut start = 0;
    for (at, _) in source.match_indices('\n') {
        let end = if at > start && source[..at].ends_with('\r') {
            at - 1
        } else {
            at
        };
        found.push((start, end));
        start = at + 1;
    }
    if start < source.len() {
        found.push((start, source.len()));
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
        | (Datum::Set(left), Datum::Set(right))
        | (Datum::Meta(left), Datum::Meta(right)) => all(left, right),
        (Datum::Record(name, left), Datum::Record(other, right)) => {
            name == other && all(left, right)
        }
        (one, other) => one == other,
    }
}

pub fn indentation(before: &str) -> Option<usize> {
    indentation_with(before, &Forms::built_in())
}

pub fn indentation_with(before: &str, forms: &Forms) -> Option<usize> {
    let (found, after) = units(before);
    if after != syntax::State::default() {
        return None;
    }
    let mut state = State::new(forms);
    for (line, (start, end)) in lines(before).into_iter().enumerate() {
        let pieces = pieces(before, start, end, &found);
        let columns = raw_columns(&pieces);
        state.feed(&pieces, &columns, line);
    }
    Some(state.indentation())
}

fn raw_columns(pieces: &[Piece]) -> Vec<usize> {
    render(pieces, false).1
}

pub fn format(source: &str) -> Result<String, String> {
    format_with(source, &Forms::built_in().scanned(source))
}

pub fn format_with(source: &str, known: &Forms) -> Result<String, String> {
    let (forms, errors) = read::read(source);
    if let Some(error) = errors.first() {
        let (line, column) = line_column(source, error.span.start);
        return Err(format!("{line}:{column}: {}", error.message));
    }
    let out = layout(source, known);
    let (again, _) = read::read(&out);
    let kept = again.len() == forms.len() && forms.iter().zip(&again).all(|(a, b)| same(a, b));
    if kept {
        Ok(out)
    } else {
        Err("Formatting would change what this code means, so it was left alone".to_owned())
    }
}

fn layout(source: &str, forms: &Forms) -> String {
    let (found, _) = units(source);
    let mut state = State::new(forms);
    let mut out: Vec<String> = Vec::new();
    let mut commented = false;
    for (start, end) in lines(source) {
        let line = out.len();
        if continuing(&found, start).is_some() {
            let pieces = pieces(source, start, end, &found);
            state.feed(&pieces, &raw_columns(&pieces), line);
            out.push(source[start..end].to_owned());
            commented = pieces
                .last()
                .is_some_and(|piece| piece.kind == Kind::Comment);
            continue;
        }
        let text = &source[start..end];
        let first = start + (text.len() - text.trim_start().len());
        let last = start + text.trim_end().len();
        if first == last {
            if out.last().is_some_and(|previous| !previous.is_empty()) {
                out.push(String::new());
            }
            continue;
        }
        let pieces = pieces(source, first, last, &found);
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
