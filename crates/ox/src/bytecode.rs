use std::cell::Cell;
use std::rc::Rc;

use crate::forms::Form;
use crate::span::{Source, Span};
use crate::symbol::Sym;
use crate::value::Value;

pub type Register = u16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prim {
    Add,
    Subtract,
    Multiply,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    Nth,
}

impl Prim {
    pub const ALL: [Self; 9] = [
        Self::Add,
        Self::Subtract,
        Self::Multiply,
        Self::Less,
        Self::LessEqual,
        Self::Greater,
        Self::GreaterEqual,
        Self::Equal,
        Self::Nth,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
            Self::Equal => "=",
            Self::Nth => "nth",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|prim| prim.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Constant {
        to: Register,
        index: u32,
    },
    Global {
        to: Register,
        name: Sym,
        cache: u16,
    },
    DefineGlobal {
        name: Sym,
        from: Register,
    },
    DefineMacro {
        name: Sym,
        from: Register,
    },
    Document {
        name: Sym,
        doc: u32,
        form: u16,
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
    Prim {
        prim: Prim,
        to: Register,
        a: Register,
        b: Register,
        base: Register,
        cache: u16,
    },
    Jump {
        to: u32,
    },
    JumpIfDefined {
        name: Sym,
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
    Resume,
    Invoke,
    Raised,
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
    pub parameters: Option<Rc<str>>,
    pub registers: Register,
    pub ops: Vec<Op>,
    pub constants: Vec<Value>,
    pub functions: Vec<Rc<Code>>,
    pub captures: Vec<Capture>,
    pub caches: Vec<Cell<Option<(Value, usize)>>>,
    pub globals: Vec<Cell<Option<(u64, Value)>>>,
    pub source: Option<Rc<Source>>,
    pub module: usize,
    pub spans: Vec<Span>,
    pub forms: Vec<Form>,
}

pub const NO_FORM: u16 = u16::MAX;
