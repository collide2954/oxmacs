use std::collections::HashMap;
use std::rc::Rc;

use crate::bytecode::{Capture, Code, Op};
use crate::error::Error;
use crate::heap::{Closure, Heap};
use crate::native::arguments;
use crate::print;
use crate::symbol::Sym;
use crate::value::Value;

const MAX_FRAMES: usize = 400_000;

#[derive(Debug, Default)]
pub struct Runtime {
    pub heap: Heap,
    pub globals: HashMap<Sym, Value>,
}

struct Frame {
    code: Rc<Code>,
    closure: Value,
    pc: usize,
    base: usize,
    result: usize,
}

struct Machine {
    stack: Vec<Value>,
    frames: Vec<Frame>,
    code: Rc<Code>,
    closure: Value,
    pc: usize,
    base: usize,
}

impl Machine {
    fn new(entry: &Rc<Code>) -> Self {
        Self {
            stack: vec![Value::Nil; usize::from(entry.registers)],
            frames: vec![Frame {
                code: Rc::clone(entry),
                closure: Value::Nil,
                pc: 0,
                base: 0,
                result: 0,
            }],
            code: Rc::clone(entry),
            closure: Value::Nil,
            pc: 0,
            base: 0,
        }
    }

    fn get(&self, register: u16) -> Value {
        self.stack[self.base + usize::from(register)]
    }

    fn set(&mut self, register: u16, value: Value) {
        self.stack[self.base + usize::from(register)] = value;
    }

    fn enter(&mut self, function: Rc<Code>, closure: Value, slot: usize) -> Result<(), Error> {
        if self.frames.len() >= MAX_FRAMES {
            return Err(Error::new("Stack overflow"));
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.pc = self.pc;
        }
        let start = slot + 1;
        let needed = start + usize::from(function.registers);
        if self.stack.len() < needed {
            self.stack.resize(needed, Value::Nil);
        }
        self.frames.push(Frame {
            code: Rc::clone(&function),
            closure,
            pc: 0,
            base: start,
            result: slot,
        });
        self.code = function;
        self.closure = closure;
        self.pc = 0;
        self.base = start;
        Ok(())
    }

    fn replace(&mut self, function: Rc<Code>, closure: Value, slot: usize, count: usize) {
        self.stack.copy_within(slot + 1..=slot + count, self.base);
        let needed = self.base + usize::from(function.registers);
        if self.stack.len() < needed {
            self.stack.resize(needed, Value::Nil);
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.code = Rc::clone(&function);
            frame.closure = closure;
        }
        self.code = function;
        self.closure = closure;
        self.pc = 0;
    }

    fn leave(&mut self, value: Value) -> Option<Value> {
        let Some(finished) = self.frames.pop() else {
            return Some(value);
        };
        let Some(caller) = self.frames.last() else {
            return Some(value);
        };
        self.stack[finished.result] = value;
        self.code = Rc::clone(&caller.code);
        self.closure = caller.closure;
        self.pc = caller.pc;
        self.base = caller.base;
        None
    }
}

fn full(error: crate::heap::HeapFull) -> Error {
    Error::new(error.to_string())
}

fn arity_error(code: &Code, count: usize) -> Error {
    let name = code.name.map_or("fn", Sym::name);
    Error::new(format!(
        "{name} takes {}, got {count}",
        arguments(usize::from(code.arity))
    ))
}

impl Runtime {
    pub fn run(&mut self, entry: &Rc<Code>) -> Result<Value, Error> {
        let mut machine = Machine::new(entry);
        loop {
            let at = machine.pc;
            let Some(&op) = machine.code.ops.get(at) else {
                return Err(Error::new("The code ended without returning"));
            };
            machine.pc += 1;
            let outcome = self.execute(&mut machine, op);
            match outcome {
                Ok(Some(value)) => return Ok(value),
                Ok(None) => {}
                Err(error) => return Err(error.or_at(machine.code.spans[at])),
            }
        }
    }

    fn execute(&mut self, machine: &mut Machine, op: Op) -> Result<Option<Value>, Error> {
        match op {
            Op::Constant { to, index } => {
                let value = machine.code.constants[index as usize];
                machine.set(to, value);
            }
            Op::Global { to, name } => {
                let value = self
                    .globals
                    .get(&name)
                    .copied()
                    .ok_or_else(|| Error::new(format!("{} is not defined", name.name())))?;
                machine.set(to, value);
            }
            Op::DefineGlobal { name, from } => {
                self.globals.insert(name, machine.get(from));
            }
            Op::SetGlobal { name, from } => {
                let Some(slot) = self.globals.get_mut(&name) else {
                    return Err(Error::new(format!("{} is not defined", name.name())));
                };
                *slot = machine.get(from);
            }
            Op::Move { to, from } => machine.set(to, machine.get(from)),
            Op::Closure { to, function } => {
                let value = self.close(machine, function)?;
                machine.set(to, value);
            }
            Op::Captured { to, index } => {
                let value = self.captured(machine.closure, index)?;
                machine.set(to, value);
            }
            Op::MakeCell { register } => {
                let cell = self.heap.cell(machine.get(register)).map_err(full)?;
                machine.set(register, cell);
            }
            Op::CellGet { to, cell } => {
                let value = self
                    .heap
                    .cell_get(machine.get(cell))
                    .ok_or_else(|| Error::new("A variable lost its cell"))?;
                machine.set(to, value);
            }
            Op::CellSet { cell, from } => {
                if !self.heap.cell_set(machine.get(cell), machine.get(from)) {
                    return Err(Error::new("A variable lost its cell"));
                }
            }
            Op::Call { base, count } => self.call(machine, base, count)?,
            Op::TailCall { base, count } => return self.tail_call(machine, base, count),
            Op::Jump { to } => machine.pc = to as usize,
            Op::JumpUnless { test, to } => {
                if !machine.get(test).truthy() {
                    machine.pc = to as usize;
                }
            }
            Op::Return { from } => return Ok(machine.leave(machine.get(from))),
        }
        Ok(None)
    }

    fn call(&mut self, machine: &mut Machine, base: u16, count: u16) -> Result<(), Error> {
        let slot = machine.base + usize::from(base);
        let count = usize::from(count);
        let callee = machine.stack[slot];
        let Some(function) = self
            .heap
            .closure_of(callee)
            .map(|closure| Rc::clone(&closure.code))
        else {
            let result = self.call_native(callee, &machine.stack[slot + 1..=slot + count])?;
            machine.stack[slot] = result;
            return Ok(());
        };
        if count != usize::from(function.arity) {
            return Err(arity_error(&function, count));
        }
        machine.enter(function, callee, slot)
    }

    fn tail_call(
        &mut self,
        machine: &mut Machine,
        base: u16,
        count: u16,
    ) -> Result<Option<Value>, Error> {
        let slot = machine.base + usize::from(base);
        let count = usize::from(count);
        let callee = machine.stack[slot];
        let Some(function) = self
            .heap
            .closure_of(callee)
            .map(|closure| Rc::clone(&closure.code))
        else {
            let result = self.call_native(callee, &machine.stack[slot + 1..=slot + count])?;
            return Ok(machine.leave(result));
        };
        if count != usize::from(function.arity) {
            return Err(arity_error(&function, count));
        }
        machine.replace(function, callee, slot, count);
        Ok(None)
    }

    fn close(&mut self, machine: &Machine, function: u32) -> Result<Value, Error> {
        let code = Rc::clone(&machine.code.functions[function as usize]);
        let captured = code
            .captures
            .iter()
            .map(|capture| match *capture {
                Capture::Register(register) => Ok(machine.get(register)),
                Capture::Captured(index) => self.captured(machine.closure, index),
            })
            .collect::<Result<Box<[Value]>, Error>>()?;
        self.heap.closure(Closure { code, captured }).map_err(full)
    }

    fn captured(&self, closure: Value, index: u16) -> Result<Value, Error> {
        self.heap
            .closure_of(closure)
            .and_then(|closure| closure.captured.get(usize::from(index)))
            .copied()
            .ok_or_else(|| Error::new("A closure lost a captured variable"))
    }

    fn call_native(&mut self, callee: Value, arguments: &[Value]) -> Result<Value, Error> {
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
