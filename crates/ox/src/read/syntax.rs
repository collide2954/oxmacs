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
    Record(String, Vec<Syntax>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Syntax {
    pub datum: Datum,
    pub span: Span,
}
