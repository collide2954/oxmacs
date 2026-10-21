use crate::span::Span;
use crate::symbol::Sym;
use crate::value::Value;

pub type Register = u16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Constant { to: Register, index: u32 },
    Global { to: Register, name: Sym },
    Move { to: Register, from: Register },
    Call { base: Register, count: u16 },
    Return { from: Register },
}

#[derive(Debug, Default)]
pub struct Code {
    pub name: Option<Sym>,
    pub registers: Register,
    pub ops: Vec<Op>,
    pub constants: Vec<Value>,
    pub spans: Vec<Span>,
}
