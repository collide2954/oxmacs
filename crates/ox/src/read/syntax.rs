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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Marks(Vec<u32>);

impl Marks {
    #[must_use]
    pub fn flipped(&self, mark: u32) -> Self {
        let mut marks = self.0.clone();
        match marks.binary_search(&mark) {
            Ok(at) => {
                marks.remove(at);
            }
            Err(at) => marks.insert(at, mark),
        }
        Self(marks)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Syntax {
    pub datum: Datum,
    pub span: Span,
    pub marks: Marks,
}

impl Syntax {
    pub fn new(datum: Datum, span: Span) -> Self {
        Self {
            datum,
            span,
            marks: Marks::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Marks;

    #[test]
    fn flipping_a_mark_twice_removes_it() {
        let one = Marks::default().flipped(3);
        let two = one.flipped(1);

        assert_eq!(two, Marks::default().flipped(1).flipped(3));
        assert_eq!(two.flipped(3), Marks::default().flipped(1));
        assert_eq!(one.flipped(3), Marks::default());
    }
}
