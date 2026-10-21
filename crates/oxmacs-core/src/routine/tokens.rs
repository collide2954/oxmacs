use std::ops::Range;

use super::{Piece, Template};

pub(super) type Found = Vec<(Range<usize>, String)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Token {
    Word(String),
    Quoted(Template),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Spanned {
    pub(super) token: Token,
    pub(super) columns: Range<usize>,
}

pub(super) fn valid_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

fn pieces(chars: &[char], start: usize, end: usize, found: &mut Found) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut at = start;
    while at < end {
        match chars[at] {
            '\\' if at + 1 < end => {
                text.push(match chars[at + 1] {
                    'n' => '\n',
                    't' => '\t',
                    other => other,
                });
                at += 2;
            }
            '{' if at + 1 < end && chars[at + 1] == '{' => {
                text.push('{');
                at += 2;
            }
            '}' if at + 1 < end && chars[at + 1] == '}' => {
                text.push('}');
                at += 2;
            }
            '{' => {
                let Some(close) = (at + 1..end).find(|index| chars[*index] == '}') else {
                    found.push((at..end, "Close {value} with }".to_owned()));
                    text.extend(&chars[at..end]);
                    break;
                };
                let name: String = chars[at + 1..close].iter().collect();
                if !valid_name(&name) {
                    found.push((at..close + 1, format!("{{{name}}} is not a value name")));
                }
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(Piece::Value {
                    name,
                    columns: at..close + 1,
                });
                at = close + 1;
            }
            other => {
                text.push(other);
                at += 1;
            }
        }
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    pieces
}

pub(super) fn word_template(word: &str, columns: &Range<usize>, found: &mut Found) -> Template {
    let chars: Vec<char> = word.chars().collect();
    let mut inner = Vec::new();
    let pieces = pieces(&chars, 0, chars.len(), &mut inner);
    let shift = |range: Range<usize>| range.start + columns.start..range.end + columns.start;
    found.extend(
        inner
            .into_iter()
            .map(|(range, message)| (shift(range), message)),
    );
    Template {
        pieces: pieces
            .into_iter()
            .map(|piece| match piece {
                Piece::Value { name, columns } => Piece::Value {
                    name,
                    columns: shift(columns),
                },
                text @ Piece::Text(_) => text,
            })
            .collect(),
        columns: columns.clone(),
    }
}

pub(super) fn tokenize(chars: &[char], from: usize, found: &mut Found) -> Vec<Spanned> {
    let mut tokens = Vec::new();
    let mut at = from;
    while at < chars.len() {
        if chars[at].is_whitespace() {
            at += 1;
            continue;
        }
        let start = at;
        if chars[at] == '"' {
            let mut end = at + 1;
            while end < chars.len() && chars[end] != '"' {
                end += if chars[end] == '\\' { 2 } else { 1 };
            }
            let inner_end = end.min(chars.len());
            if end >= chars.len() {
                found.push((start..chars.len(), "Close the text with \"".to_owned()));
            }
            let pieces = pieces(chars, start + 1, inner_end, found);
            at = (inner_end + 1).min(chars.len());
            tokens.push(Spanned {
                token: Token::Quoted(Template {
                    pieces,
                    columns: start..at,
                }),
                columns: start..at,
            });
        } else {
            while at < chars.len() && !chars[at].is_whitespace() {
                at += 1;
            }
            tokens.push(Spanned {
                token: Token::Word(chars[start..at].iter().collect()),
                columns: start..at,
            });
        }
    }
    tokens
}
