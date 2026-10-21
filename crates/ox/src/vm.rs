use std::collections::HashMap;

use crate::bytecode::{Code, Op};
use crate::error::Error;
use crate::heap::Heap;
use crate::print;
use crate::symbol::Sym;
use crate::value::Value;

#[derive(Debug, Default)]
pub struct Runtime {
    pub heap: Heap,
    pub globals: HashMap<Sym, Value>,
}

impl Runtime {
    pub fn run(&mut self, code: &Code) -> Result<Value, Error> {
        let mut registers = vec![Value::Nil; usize::from(code.registers)];
        let mut pc = 0;
        loop {
            let at = pc;
            let op = code
                .ops
                .get(pc)
                .ok_or_else(|| Error::new("The code ended without returning"))?;
            pc += 1;
            match *op {
                Op::Constant { to, index } => {
                    registers[usize::from(to)] = code.constants[index as usize];
                }
                Op::Global { to, name } => {
                    registers[usize::from(to)] =
                        self.globals.get(&name).copied().ok_or_else(|| {
                            Error::at(code.spans[at], format!("{} is not defined", name.name()))
                        })?;
                }
                Op::DefineGlobal { name, from } => {
                    self.globals.insert(name, registers[usize::from(from)]);
                }
                Op::Move { to, from } => {
                    registers[usize::from(to)] = registers[usize::from(from)];
                }
                Op::Call { base, count } => {
                    let base = usize::from(base);
                    let arguments = &registers[base + 1..=base + usize::from(count)];
                    registers[base] = self
                        .call(registers[base], arguments)
                        .map_err(|error| error.or_at(code.spans[at]))?;
                }
                Op::Jump { to } => pc = to as usize,
                Op::JumpUnless { test, to } => {
                    if !registers[usize::from(test)].truthy() {
                        pc = to as usize;
                    }
                }
                Op::Return { from } => return Ok(registers[usize::from(from)]),
            }
        }
    }

    fn call(&mut self, callee: Value, arguments: &[Value]) -> Result<Value, Error> {
        let Some(native) = self.heap.native_of(callee).cloned() else {
            return Err(Error::new(format!(
                "{} is not a function",
                print::write(&self.heap, callee)
            )));
        };
        native.check(arguments.len())?;
        (native.function)(&mut self.heap, arguments)
    }
}
