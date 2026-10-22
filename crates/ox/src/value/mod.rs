pub(crate) mod equal;
pub(crate) mod foreign;
pub(crate) mod map;
pub(crate) mod number;
pub mod print;
pub(crate) mod promise;
pub(crate) mod record;
pub(crate) mod vector;

use crate::symbol::Sym;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Value {
    #[default]
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Char(char),
    Symbol(Sym),
    Keyword(Sym),
    Ref(Ref),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ref {
    pub(crate) kind: Kind,
    pub(crate) index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    String,
    Pair,
    Native,
    Closure,
    Cell,
    BigInt,
    Vector,
    VecNode,
    Map,
    MapNode,
    Set,
    RecordType,
    Record,
    Atom,
    Identifier,
    Generator,
    Promise,
    Channel,
    Foreign,
}

impl Value {
    pub fn symbol(name: &str) -> Self {
        Self::Symbol(Sym::intern(name))
    }

    pub fn keyword(name: &str) -> Self {
        Self::Keyword(Sym::intern(name))
    }

    pub fn truthy(self) -> bool {
        !matches!(self, Self::Nil | Self::Bool(false))
    }
}

#[cfg(test)]
mod tests {
    use super::Value;

    #[test]
    fn values_fit_in_sixteen_bytes() {
        assert_eq!(size_of::<Value>(), 16);
    }

    #[test]
    fn only_false_and_nil_are_false() {
        assert!(!Value::Nil.truthy());
        assert!(!Value::Bool(false).truthy());
        for value in [
            Value::Bool(true),
            Value::Int(0),
            Value::Float(0.0),
            Value::Char('\0'),
            Value::symbol("nil"),
            Value::keyword("false"),
        ] {
            assert!(value.truthy(), "{value:?}");
        }
    }
}
