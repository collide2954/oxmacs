pub(crate) mod bind;
pub(crate) mod build;

use build::{
    call_data, field, full, make_map, make_set, make_vector, no_field, raised, record_literal,
};

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::bytecode::{Capture, Code, Op};
use crate::error::{Error, ErrorKind};
use crate::heap::{Closure, Heap};
use crate::library::collections;
use crate::span::Span;
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::record::Record;
use crate::vm::bind::{Binding, bind};

const MAX_FRAMES: usize = 400_000;

#[derive(Debug, Default)]
pub struct Runtime {
    pub heap: Heap,
    pub globals: HashMap<Sym, Value>,
    pub field_scans: usize,
    pub apply: Value,
    pub macros: HashMap<Sym, Value>,
    pub error_types: HashMap<ErrorKind, Value>,
    marks: u32,
}

struct Frame {
    code: Rc<Code>,
    closure: Value,
    missing: u64,
    pc: usize,
    base: usize,
    result: usize,
}

struct Handler {
    frames: usize,
    to: usize,
    register: u16,
}

struct Machine {
    stack: Vec<Value>,
    frames: Vec<Frame>,
    handlers: Vec<Handler>,
    code: Rc<Code>,
    closure: Value,
    missing: u64,
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
                missing: 0,
                pc: 0,
                base: 0,
                result: 0,
            }],
            handlers: Vec::new(),
            code: Rc::clone(entry),
            closure: Value::Nil,
            missing: 0,
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

    fn items(&self, base: u16, count: u16) -> &[Value] {
        let start = self.base + usize::from(base);
        &self.stack[start..start + usize::from(count)]
    }

    fn place(&mut self, start: usize, function: &Code, binding: Binding) -> u64 {
        let needed = start + usize::from(function.registers);
        if self.stack.len() < needed {
            self.stack.resize(needed, Value::Nil);
        }
        match binding {
            Binding::Direct => 0,
            Binding::Bound { values, missing } => {
                self.stack[start..start + values.len()].copy_from_slice(&values);
                missing
            }
        }
    }

    fn enter(
        &mut self,
        function: Rc<Code>,
        closure: Value,
        slot: usize,
        binding: Binding,
    ) -> Result<(), Error> {
        if self.frames.len() >= MAX_FRAMES {
            return Err(Error::new("Stack overflow"));
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.pc = self.pc;
        }
        let start = slot + 1;
        let missing = self.place(start, &function, binding);
        self.frames.push(Frame {
            code: Rc::clone(&function),
            closure,
            missing,
            pc: 0,
            base: start,
            result: slot,
        });
        self.code = function;
        self.closure = closure;
        self.missing = missing;
        self.pc = 0;
        self.base = start;
        Ok(())
    }

    fn replace(
        &mut self,
        function: Rc<Code>,
        closure: Value,
        slot: usize,
        count: usize,
        binding: Binding,
    ) {
        if matches!(binding, Binding::Direct) {
            self.stack.copy_within(slot + 1..=slot + count, self.base);
        }
        let missing = self.place(self.base, &function, binding);
        if let Some(frame) = self.frames.last_mut() {
            frame.code = Rc::clone(&function);
            frame.closure = closure;
            frame.missing = missing;
        }
        self.code = function;
        self.closure = closure;
        self.missing = missing;
        self.pc = 0;
    }

    fn unwind(&mut self) -> Option<Handler> {
        while let Some(handler) = self.handlers.pop() {
            if handler.frames == 0 || handler.frames > self.frames.len() {
                continue;
            }
            self.frames.truncate(handler.frames);
            let frame = self.frames.last()?;
            self.code = Rc::clone(&frame.code);
            self.closure = frame.closure;
            self.missing = frame.missing;
            self.base = frame.base;
            self.pc = handler.to;
            return Some(handler);
        }
        None
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
        self.missing = caller.missing;
        self.pc = caller.pc;
        self.base = caller.base;
        None
    }
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
                Err(error) => {
                    let error = error.or_at(machine.code.spans[at]);
                    let Some(handler) = machine.unwind() else {
                        return Err(error);
                    };
                    let value = self.error_value(&error)?;
                    machine.set(handler.register, value);
                }
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
                let value = self.globals.get(&name).copied().ok_or_else(|| {
                    Error::new(format!("{} is not defined", name.name())).of(ErrorKind::Name)
                })?;
                machine.set(to, value);
            }
            Op::DefineMacro { name, from } => {
                self.macros.insert(name, machine.get(from));
            }
            Op::DefineGlobal { name, from } => {
                self.globals.insert(name, machine.get(from));
            }
            Op::SetGlobal { name, from } => {
                let Some(slot) = self.globals.get_mut(&name) else {
                    return Err(
                        Error::new(format!("{} is not defined", name.name())).of(ErrorKind::Name)
                    );
                };
                *slot = machine.get(from);
            }
            Op::Move { to, from } => machine.set(to, machine.get(from)),
            Op::Closure { to, function } => {
                let value = self.close(machine, function)?;
                machine.set(to, value);
            }
            Op::Itself { to } => machine.set(to, machine.closure),
            Op::Captured { to, index } => {
                let value = self.captured(machine.closure, index)?;
                machine.set(to, value);
            }
            Op::MakeCell { register } => {
                let cell = self.heap.cell(machine.get(register)).map_err(full)?;
                machine.set(register, cell);
            }
            Op::MakeVector { .. }
            | Op::MakeMap { .. }
            | Op::MakeSet { .. }
            | Op::MakeRecord { .. }
            | Op::MakeRecordFrom { .. }
            | Op::DefineRecord { .. }
            | Op::Field { .. }
            | Op::PushHandler { .. }
            | Op::PopHandler
            | Op::Raise { .. } => self.build(machine, op)?,
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
            Op::JumpIfSupplied { bit, to } => {
                if machine.missing & (1 << bit) == 0 {
                    machine.pc = to as usize;
                }
            }
            Op::JumpUnless { test, to } => {
                if !machine.get(test).truthy() {
                    machine.pc = to as usize;
                }
            }
            Op::Return { from } => return Ok(machine.leave(machine.get(from))),
        }
        Ok(None)
    }

    fn build(&mut self, machine: &mut Machine, op: Op) -> Result<(), Error> {
        let (to, value) = match op {
            Op::MakeVector { to, base, count } => {
                (to, make_vector(&mut self.heap, machine.items(base, count))?)
            }
            Op::MakeMap { to, base, count } => {
                (to, make_map(&mut self.heap, machine.items(base, count))?)
            }
            Op::MakeSet { to, base, count } => {
                (to, make_set(&mut self.heap, machine.items(base, count))?)
            }
            Op::MakeRecord {
                to,
                record,
                base,
                count,
            } => (to, self.make_record(machine, record, base, count)?),
            Op::MakeRecordFrom { to, base, count } => (
                to,
                record_literal(&mut self.heap, machine.items(base, count))?,
            ),
            Op::Field {
                to,
                from,
                name,
                cache,
            } => (to, self.field(machine, machine.get(from), name, cache)?),
            Op::DefineRecord {
                record,
                constructor,
            } => {
                self.define_record(machine, record, constructor);
                return Ok(());
            }
            Op::PushHandler { register, to } => {
                machine.handlers.push(Handler {
                    frames: machine.frames.len(),
                    to: to as usize,
                    register,
                });
                return Ok(());
            }
            Op::PopHandler => {
                machine.handlers.pop();
                return Ok(());
            }
            Op::Raise { from } => return Err(raised(&self.heap, machine.get(from))),
            _ => return Ok(()),
        };
        machine.set(to, value);
        Ok(())
    }

    fn call(&mut self, machine: &mut Machine, base: u16, count: u16) -> Result<(), Error> {
        if self.heap.wants_collection() {
            self.collect_running(machine);
        }
        let slot = machine.base + usize::from(base);
        let count = self.spread(machine, slot, usize::from(count))?;
        let callee = self.constructor_of(machine.stack[slot]);
        let Some(function) = self
            .heap
            .closure_of(callee)
            .map(|closure| Rc::clone(&closure.code))
        else {
            let result = self.call_native(callee, &machine.stack[slot + 1..=slot + count])?;
            machine.stack[slot] = result;
            return Ok(());
        };
        let binding = bind(
            &mut self.heap,
            &function,
            &machine.stack[slot + 1..=slot + count],
        )?;
        machine.enter(function, callee, slot, binding)
    }

    fn tail_call(
        &mut self,
        machine: &mut Machine,
        base: u16,
        count: u16,
    ) -> Result<Option<Value>, Error> {
        if self.heap.wants_collection() {
            self.collect_running(machine);
        }
        let slot = machine.base + usize::from(base);
        let count = self.spread(machine, slot, usize::from(count))?;
        let callee = self.constructor_of(machine.stack[slot]);
        let Some(function) = self
            .heap
            .closure_of(callee)
            .map(|closure| Rc::clone(&closure.code))
        else {
            let result = self.call_native(callee, &machine.stack[slot + 1..=slot + count])?;
            return Ok(machine.leave(result));
        };
        let binding = bind(
            &mut self.heap,
            &function,
            &machine.stack[slot + 1..=slot + count],
        )?;
        machine.replace(function, callee, slot, count, binding);
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
                Capture::Itself => Ok(machine.closure),
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

    fn field(
        &mut self,
        machine: &Machine,
        value: Value,
        name: Sym,
        cache: u16,
    ) -> Result<Value, Error> {
        let Some(record) = self.heap.record_of(value) else {
            return field(&self.heap, value, name);
        };
        let site = machine.code.caches.get(usize::from(cache));
        let slot = match site.and_then(Cell::get) {
            Some((kind, slot)) if kind == record.kind => slot,
            _ => {
                self.field_scans += 1;
                let slot = record
                    .slot(&self.heap, name)
                    .ok_or_else(|| no_field(record.type_name(&self.heap), name))?;
                if let Some(site) = site {
                    site.set(Some((record.kind, slot)));
                }
                slot
            }
        };
        record
            .fields
            .get(slot)
            .copied()
            .ok_or_else(|| no_field(record.type_name(&self.heap), name))
    }

    fn base_roots(&self) -> Vec<Value> {
        let mut roots: Vec<Value> = self.globals.values().copied().collect();
        roots.extend(self.macros.values().copied());
        roots.extend(self.error_types.values().copied());
        roots.push(self.apply);
        roots
    }

    fn error_value(&mut self, error: &Error) -> Result<Value, Error> {
        if let Some(value) = error.raised {
            return Ok(value);
        }
        let message = self.heap.string(&error.message).map_err(full)?;
        let kind = self
            .error_types
            .get(&error.kind)
            .or_else(|| self.error_types.get(&ErrorKind::Other))
            .copied();
        let Some(kind) = kind else {
            return Ok(message);
        };
        self.heap
            .record(Record {
                kind,
                fields: Box::new([message, Value::Nil]),
            })
            .map_err(full)
    }

    pub fn fresh_mark(&mut self) -> u32 {
        self.marks += 1;
        self.marks
    }

    pub fn invoke(
        &mut self,
        function: Value,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Error> {
        let count = u16::try_from(arguments.len())
            .map_err(|_| Error::at(span, "This call has too many arguments"))?;
        let mut constants = vec![function];
        constants.extend_from_slice(arguments);
        let mut ops: Vec<Op> = (0..=count)
            .map(|register| Op::Constant {
                to: register,
                index: u32::from(register),
            })
            .collect();
        ops.push(Op::Call { base: 0, count });
        ops.push(Op::Return { from: 0 });
        let code = Code {
            registers: count + 1,
            spans: vec![span; ops.len()],
            ops,
            constants,
            ..Code::default()
        };
        self.run(&Rc::new(code))
    }

    pub fn collect(&mut self) -> usize {
        let roots = self.base_roots();
        self.heap.collect(&roots, &[])
    }

    fn collect_running(&mut self, machine: &Machine) {
        let mut roots = self.base_roots();
        roots.extend(machine.stack.iter().copied());
        roots.push(machine.closure);
        roots.extend(machine.frames.iter().map(|frame| frame.closure));
        let mut codes = vec![Rc::clone(&machine.code)];
        codes.extend(machine.frames.iter().map(|frame| Rc::clone(&frame.code)));
        self.heap.collect(&roots, &codes);
    }

    fn spread(&mut self, machine: &mut Machine, slot: usize, count: usize) -> Result<usize, Error> {
        let mut count = count;
        while self.apply != Value::Nil && machine.stack[slot] == self.apply {
            if count < 2 {
                return Err(
                    Error::new(format!("apply takes at least 2 arguments, got {count}"))
                        .of(ErrorKind::Arity),
                );
            }
            let items =
                collections::sequence(&mut self.heap, "apply", machine.stack[slot + count])?;
            machine.stack.copy_within(slot + 1..slot + count, slot);
            let start = slot + count - 1;
            let needed = start + items.len() + 1;
            if machine.stack.len() < needed {
                machine.stack.resize(needed, Value::Nil);
            }
            machine.stack[start..start + items.len()].copy_from_slice(&items);
            count = count - 2 + items.len();
        }
        Ok(count)
    }

    fn constructor_of(&self, callee: Value) -> Value {
        self.heap
            .record_type_of(callee)
            .map_or(callee, |kind| kind.constructor)
    }

    fn make_record(
        &mut self,
        machine: &Machine,
        record: u32,
        base: u16,
        count: u16,
    ) -> Result<Value, Error> {
        let kind = machine.code.constants[record as usize];
        let fields = machine.items(base, count).into();
        self.heap.record(Record { kind, fields }).map_err(full)
    }

    fn define_record(&mut self, machine: &Machine, record: u32, constructor: u16) {
        let kind = machine.code.constants[record as usize];
        self.heap.set_constructor(kind, machine.get(constructor));
    }

    fn call_native(&mut self, callee: Value, arguments: &[Value]) -> Result<Value, Error> {
        let Some(native) = self.heap.native_of(callee).cloned() else {
            return call_data(&self.heap, callee, arguments);
        };
        native.check(arguments.len())?;
        (native.function)(&mut self.heap, arguments)
    }
}
