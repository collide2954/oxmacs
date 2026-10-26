use std::rc::Rc;

use crate::bytecode::Code;
use crate::error::{Error, Frame as Trace};
use crate::span::Span;
use crate::value::Value;
use crate::vm::bind::Binding;
use crate::vm::routine::Routine;

const MAX_FRAMES: usize = 400_000;

#[derive(Debug)]
pub(crate) struct Frame {
    pub(crate) code: Rc<Code>,
    pub(crate) closure: Value,
    pub(crate) missing: u64,
    pub(crate) pc: usize,
    pub(crate) base: usize,
    pub(crate) result: usize,
    pub(crate) routine: Option<Box<dyn Routine>>,
    pub(crate) call: Option<Span>,
}

#[derive(Debug)]
pub(crate) struct Handler {
    pub(crate) frames: usize,
    pub(crate) to: usize,
    pub(crate) register: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Landing {
    Nowhere,
    Register(usize),
    Leave,
}

#[derive(Debug)]
pub(crate) struct Machine {
    pub(crate) stack: Vec<Value>,
    pub(crate) frames: Vec<Frame>,
    pub(crate) handlers: Vec<Handler>,
    pub(crate) code: Rc<Code>,
    pub(crate) closure: Value,
    pub(crate) missing: u64,
    pub(crate) pc: usize,
    pub(crate) base: usize,
}

#[derive(Debug)]
pub(crate) enum Generator {
    Ready(Box<Machine>, Landing),
    Running,
    Done,
}

#[derive(Debug)]
pub(crate) struct Caller {
    pub(crate) machine: Machine,
    pub(crate) landing: Landing,
    pub(crate) generator: Value,
}

#[derive(Debug)]
pub(crate) struct Fiber {
    pub(crate) machine: Machine,
    pub(crate) callers: Vec<Caller>,
    pub(crate) atomic: u32,
}

impl Fiber {
    pub(crate) fn new(machine: Machine) -> Self {
        Self {
            machine,
            callers: Vec::new(),
            atomic: 0,
        }
    }

    pub(crate) fn roots(&self, values: &mut Vec<Value>, codes: &mut Vec<Rc<Code>>) {
        self.machine.roots(values, codes);
        for caller in &self.callers {
            caller.machine.roots(values, codes);
            values.push(caller.generator);
        }
    }
}

impl Machine {
    pub(crate) fn new(entry: &Rc<Code>) -> Self {
        Self {
            stack: vec![Value::Nil; usize::from(entry.registers)],
            frames: vec![Frame {
                code: Rc::clone(entry),
                closure: Value::Nil,
                missing: 0,
                pc: 0,
                base: 0,
                result: 0,
                routine: None,
                call: None,
            }],
            handlers: Vec::new(),
            code: Rc::clone(entry),
            closure: Value::Nil,
            missing: 0,
            pc: 0,
            base: 0,
        }
    }

    pub(crate) fn roots(&self, values: &mut Vec<Value>, codes: &mut Vec<Rc<Code>>) {
        values.extend(self.stack.iter().copied());
        values.push(self.closure);
        for frame in &self.frames {
            values.push(frame.closure);
            if let Some(routine) = &frame.routine {
                routine.roots(values);
            }
        }
        codes.push(Rc::clone(&self.code));
        codes.extend(self.frames.iter().map(|frame| Rc::clone(&frame.code)));
    }

    pub(crate) fn get(&self, register: u16) -> Value {
        self.stack[self.base + usize::from(register)]
    }

    pub(crate) fn set(&mut self, register: u16, value: Value) {
        self.stack[self.base + usize::from(register)] = value;
    }

    pub(crate) fn items(&self, base: u16, count: u16) -> &[Value] {
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

    pub(crate) fn enter(
        &mut self,
        function: Rc<Code>,
        closure: Value,
        slot: usize,
        result: usize,
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
            result,
            routine: None,
            call: None,
        });
        self.code = function;
        self.closure = closure;
        self.missing = missing;
        self.pc = 0;
        self.base = start;
        Ok(())
    }

    pub(crate) fn replace(
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

    pub(crate) fn span(&self, at: usize) -> Option<Span> {
        self.code
            .spans
            .get(at)
            .or_else(|| self.code.spans.last())
            .copied()
            .or_else(|| self.frames.last().and_then(|frame| frame.call))
    }

    pub(crate) fn run_routine(
        &mut self,
        routine: Box<dyn Routine>,
        call: Option<Span>,
    ) -> Result<(), Error> {
        self.stack[self.base] = Value::Nil;
        let Some(frame) = self.frames.last_mut() else {
            return Err(Error::new("A native lost its frame"));
        };
        frame.routine = Some(routine);
        frame.call = call;
        Ok(())
    }

    pub(crate) fn traceback(&self, at: usize) -> Vec<Trace> {
        let fallback = Span::new(0, 0);
        let mut frames = vec![Trace {
            function: self.code.name,
            span: self.span(at).unwrap_or(fallback),
            source: self.code.source.clone(),
        }];
        let callers = self.frames.len().saturating_sub(1);
        for frame in self.frames[..callers].iter().rev() {
            let call = frame.pc.saturating_sub(1);
            frames.push(Trace {
                function: frame.code.name,
                span: frame.code.spans.get(call).copied().unwrap_or(fallback),
                source: frame.code.source.clone(),
            });
        }
        frames.retain(|frame| frame.source.is_some());
        frames
    }

    pub(crate) fn unwind(&mut self) -> Option<Handler> {
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

    pub(crate) fn leave(&mut self, value: Value) -> Option<Value> {
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

    pub(crate) fn land(&mut self, landing: Landing, value: Value) -> Option<Value> {
        match landing {
            Landing::Nowhere => None,
            Landing::Register(slot) => {
                self.stack[slot] = value;
                None
            }
            Landing::Leave => self.leave(value),
        }
    }
}
