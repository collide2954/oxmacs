use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum Datum {
    Int(i64),
    Float(f64),
    Symbol(String),
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

pub fn read(source: &str) -> (Vec<Syntax>, Vec<ReadError>) {
    let mut reader = Reader {
        source,
        at: 0,
        errors: Vec::new(),
    };
    let mut data = Vec::new();
    while let Some(syntax) = reader.datum() {
        data.push(syntax);
    }
    (data, reader.errors)
}

struct Reader<'a> {
    source: &'a str,
    at: usize,
    errors: Vec<ReadError>,
}

fn separator(character: char) -> bool {
    character.is_whitespace() || character == ','
}

fn delimiter(character: char) -> bool {
    separator(character) || matches!(character, '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';')
}

impl Reader<'_> {
    fn rest(&self) -> &str {
        &self.source[self.at..]
    }

    fn skip_separators(&mut self) {
        let rest = self.rest();
        self.at += rest.len() - rest.trim_start_matches(separator).len();
    }

    fn datum(&mut self) -> Option<Syntax> {
        loop {
            self.skip_separators();
            let start = self.at;
            let first = self.rest().chars().next()?;
            if delimiter(first) {
                self.at += first.len_utf8();
                self.errors.push(ReadError {
                    message: format!("Unexpected {first}"),
                    span: Span::new(start, self.at),
                });
                continue;
            }
            let rest = self.rest();
            self.at += rest.find(delimiter).unwrap_or(rest.len());
            return Some(Syntax {
                datum: atom(&self.source[start..self.at]),
                span: Span::new(start, self.at),
            });
        }
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
