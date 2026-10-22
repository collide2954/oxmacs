use std::cell::Cell;
use std::rc::Rc;

use crate::span::{Source, Span};
use crate::symbol::Sym;
use crate::value::Value;

pub type Register = u16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Constant {
        to: Register,
        index: u32,
    },
    Global {
        to: Register,
        name: Sym,
    },
    DefineGlobal {
        name: Sym,
        from: Register,
    },
    DefineMacro {
        name: Sym,
        from: Register,
    },
    SetGlobal {
        name: Sym,
        from: Register,
    },
    Move {
        to: Register,
        from: Register,
    },
    Closure {
        to: Register,
        function: u32,
    },
    Captured {
        to: Register,
        index: u16,
    },
    Itself {
        to: Register,
    },
    MakeCell {
        register: Register,
    },
    MakeVector {
        to: Register,
        base: Register,
        count: u16,
    },
    MakeMap {
        to: Register,
        base: Register,
        count: u16,
    },
    MakeSet {
        to: Register,
        base: Register,
        count: u16,
    },
    Field {
        to: Register,
        from: Register,
        name: Sym,
        cache: u16,
    },
    MakeRecord {
        to: Register,
        record: u32,
        base: Register,
        count: u16,
    },
    DefineRecord {
        record: u32,
        constructor: Register,
    },
    MakeRecordFrom {
        to: Register,
        base: Register,
        count: u16,
    },
    CellGet {
        to: Register,
        cell: Register,
    },
    CellSet {
        cell: Register,
        from: Register,
    },
    Call {
        base: Register,
        count: u16,
    },
    TailCall {
        base: Register,
        count: u16,
    },
    Jump {
        to: u32,
    },
    JumpUnless {
        test: Register,
        to: u32,
    },
    JumpIfSupplied {
        bit: u8,
        to: u32,
    },
    PushHandler {
        register: Register,
        to: u32,
    },
    PopHandler,
    Raise {
        from: Register,
    },
    Return {
        from: Register,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    Register(Register),
    Captured(u16),
    Itself,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Arity {
    pub required: u16,
    pub optional: u16,
    pub rest: bool,
    pub keywords: Vec<Sym>,
}

impl Arity {
    pub fn simple(&self) -> bool {
        self.optional == 0 && !self.rest && self.keywords.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct Code {
    pub name: Option<Sym>,
    pub arity: Arity,
    pub registers: Register,
    pub ops: Vec<Op>,
    pub constants: Vec<Value>,
    pub functions: Vec<Rc<Code>>,
    pub captures: Vec<Capture>,
    pub caches: Vec<Cell<Option<(Value, usize)>>>,
    pub source: Option<Rc<Source>>,
    pub spans: Vec<Span>,
}
