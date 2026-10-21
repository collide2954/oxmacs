use std::rc::Rc;

use crate::span::Span;
use crate::symbol::Sym;
use crate::value::Value;

pub type Register = u16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Constant { to: Register, index: u32 },
    Global { to: Register, name: Sym },
    DefineGlobal { name: Sym, from: Register },
    Move { to: Register, from: Register },
    Closure { to: Register, function: u32 },
    Call { base: Register, count: u16 },
    Jump { to: u32 },
    JumpUnless { test: Register, to: u32 },
    Return { from: Register },
}

#[derive(Debug, Default)]
pub struct Code {
    pub name: Option<Sym>,
    pub arity: u16,
    pub registers: Register,
    pub ops: Vec<Op>,
    pub constants: Vec<Value>,
    pub functions: Vec<Rc<Code>>,
    pub spans: Vec<Span>,
}
