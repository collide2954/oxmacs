use std::fmt;

use crate::span::Span;
use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ErrorKind {
    #[default]
    Other,
    Type,
    Arity,
    Name,
    Range,
    ZeroDivision,
    NoMatch,
}

impl ErrorKind {
    pub const NAMES: [(Self, &'static str); 7] = [
        (Self::Other, "error"),
        (Self::Type, "type-error"),
        (Self::Arity, "arity-error"),
        (Self::Name, "name-error"),
        (Self::Range, "range-error"),
        (Self::ZeroDivision, "zero-division-error"),
        (Self::NoMatch, "no-match-error"),
    ];
}

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub message: String,
    pub span: Option<Span>,
    pub kind: ErrorKind,
    pub raised: Option<Value>,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
            kind: ErrorKind::Other,
            raised: None,
        }
    }

    pub fn at(span: Span, message: impl Into<String>) -> Self {
        Self {
            span: Some(span),
            ..Self::new(message)
        }
    }

    pub fn raised(message: impl Into<String>, value: Value) -> Self {
        Self {
            raised: Some(value),
            ..Self::new(message)
        }
    }

    #[must_use]
    pub fn of(mut self, kind: ErrorKind) -> Self {
        self.kind = kind;
        self
    }

    #[must_use]
    pub fn or_at(mut self, span: Span) -> Self {
        self.span.get_or_insert(span);
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
