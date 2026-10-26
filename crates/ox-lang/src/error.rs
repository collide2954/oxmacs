use std::fmt;
use std::rc::Rc;

use crate::heap::HeapFull;
use crate::span::{Source, Span, line_column};
use crate::symbol::Sym;
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
    Interrupt,
    Refused,
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
pub struct Frame {
    pub function: Option<Sym>,
    pub span: Span,
    pub source: Option<Rc<Source>>,
}

impl Frame {
    pub fn file(&self) -> &str {
        self.source
            .as_ref()
            .map_or("?", |source| source.name.as_str())
    }

    pub fn line_column(&self) -> (usize, usize) {
        self.source
            .as_ref()
            .map_or((0, 0), |source| line_column(&source.text, self.span.start))
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (line, column) = self.line_column();
        let name = self.function.map_or("top level", Sym::name);
        write!(formatter, "{name} at {}:{line}:{column}", self.file())
    }
}

impl From<HeapFull> for Error {
    fn from(full: HeapFull) -> Self {
        Self::new(full.to_string())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub message: String,
    pub span: Option<Span>,
    pub kind: ErrorKind,
    pub raised: Option<Value>,
    pub traceback: Vec<Frame>,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
            kind: ErrorKind::Other,
            raised: None,
            traceback: Vec::new(),
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

    pub fn place(&self, origin: Option<(&str, &str)>) -> Option<String> {
        match (self.traceback.first(), self.span, origin) {
            (Some(frame), _, _) => {
                let (line, column) = frame.line_column();
                Some(format!("{}:{line}:{column}", frame.file()))
            }
            (None, Some(span), Some((file, source))) => {
                let (line, column) = line_column(source, span.start);
                Some(format!("{file}:{line}:{column}"))
            }
            (None, None, Some((file, _))) => Some(file.to_owned()),
            (None, _, None) => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
