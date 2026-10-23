pub(crate) mod bind;
pub(crate) mod build;
mod convert;
pub(crate) mod machine;
pub(crate) mod modules;
mod prims;
mod tasks;
mod workers;

use build::{
    call_data, field, full, interrupt_marker, make_map, make_set, make_vector, no_field, raised,
    record_literal,
};

use std::any::Any;
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::bytecode::{Capture, Code, Op, Prim, Register};
use crate::describe::Place;
use crate::error::{Error, ErrorKind, Frame as Trace};
use crate::heap::{Closure, Heap};
use crate::isolate::Isolate;
use crate::library::collections;
use crate::native::{Control, NativeFn};
use crate::portable::Portable;
use crate::profile::{Profile, Profiler};
use crate::span::Span;
use crate::symbol::Sym;
use crate::task::{Interrupter, TaskId};
use crate::value::Value;
use crate::value::equal::identical;
use crate::value::map::Map;
use crate::value::number::to_float;
use crate::value::print;
use crate::value::promise::Promise;
use crate::value::record::Record;
use crate::vm::bind::bind;
use machine::{Caller, Fiber, Generator, Landing, Machine};
use modules::{Module, ModuleId};
use tasks::Task;

#[derive(Debug)]
pub struct Runtime {
    pub heap: Heap,
    pub field_scans: usize,
    pub global_lookups: usize,
    pub(crate) profiler: Option<Box<Profiler>>,
    epoch: u64,
    pub apply: Value,
    pub(crate) prims: [Value; Prim::ALL.len()],
    pub error_types: HashMap<ErrorKind, Value>,
    pub modules: Vec<Module>,
    pub module_ids: HashMap<String, ModuleId>,
    mark_modules: HashMap<u32, ModuleId>,
    last_raised: Option<(Value, Vec<Trace>)>,
    marks: u32,
    pub interrupter: Interrupter,
    tasks: VecDeque<Task>,
    blocked: Vec<Task>,
    parked: Vec<(Fiber, Wait)>,
    finished: Vec<(TaskId, Result<Value, Error>)>,
    unhandled: Vec<Error>,
    next_task: u64,
    running: Option<Value>,
    pub(crate) roots: Vec<PathBuf>,
    pub(crate) worker_setup: Option<fn(&mut Isolate)>,
    stand_ins: HashMap<(Sym, Sym), Value>,
    channels: HashMap<usize, Value>,
}

const CHECK_EVERY: u32 = 1024;
const PROFILED_EVERY: u32 = 128;

#[derive(Debug, Clone, Copy)]
enum Flow {
    Next,
    Return(Value),
    Land(Landing, Value),
    Yield(Value, Landing),
    Resume(Value, Landing),
    Await(Value, Landing),
    Send(Value, Value, Landing),
    Receive(Value, Landing),
    Sleep(Value, Landing),
    Atomic(bool, Landing),
}

#[derive(Debug, Clone)]
enum Wait {
    Promise(Value),
    Receive(Value),
    Send(Value, Portable),
    Until(Instant),
}

impl Wait {
    fn value(&self) -> Value {
        match self {
            Self::Promise(value) | Self::Receive(value) | Self::Send(value, _) => *value,
            Self::Until(_) => Value::Nil,
        }
    }
}

#[derive(Debug)]
enum Stop {
    Finished(Value),
    Paused,
    Blocked(Wait, Landing),
}

type Step = Result<Flow, Error>;

enum Target {
    Code(Rc<Code>),
    Native(Rc<NativeFn>, Sym, bool),
    Control(Control),
    Data,
}

impl Default for Runtime {
    fn default() -> Self {
        let mut runtime = Self {
            heap: Heap::default(),
            field_scans: 0,
            global_lookups: 0,
            profiler: None,
            epoch: 0,
            apply: Value::Nil,
            prims: [Value::Nil; Prim::ALL.len()],
            error_types: HashMap::new(),
            modules: Vec::new(),
            module_ids: HashMap::new(),
            mark_modules: HashMap::new(),
            last_raised: None,
            marks: 0,
            interrupter: Interrupter::default(),
            tasks: VecDeque::new(),
            blocked: Vec::new(),
            parked: Vec::new(),
            finished: Vec::new(),
            unhandled: Vec::new(),
            next_task: 0,
            running: None,
            roots: Vec::new(),
            worker_setup: None,
            stand_ins: HashMap::new(),
            channels: HashMap::new(),
        };
        runtime.module_named("ox");
        runtime.module_named("user");
        runtime
    }
}

fn not_defined(name: Sym) -> Error {
    Error::new(format!("{} is not defined", modules::display(name))).of(ErrorKind::Name)
}

fn interrupted() -> Error {
    Error::new("Interrupted").of(ErrorKind::Interrupt)
}

impl Runtime {
    pub fn run(&mut self, entry: &Rc<Code>, host: &mut dyn Any) -> Result<Value, Error> {
        let mut fiber = Fiber::new(Machine::new(entry));
        let mut first = None;
        loop {
            match self.drive(&mut fiber, None, first.take(), host)? {
                Stop::Finished(value) => return Ok(value),
                Stop::Paused => {}
                Stop::Blocked(wait, landing) => {
                    self.parked.push((fiber, wait.clone()));
                    let step = self.wait_for(&wait, landing, host);
                    let Some((parked, _)) = self.parked.pop() else {
                        return Err(Error::new("A waiting evaluation was lost"));
                    };
                    fiber = parked;
                    first = Some(step);
                }
            }
        }
    }

    fn drive(
        &mut self,
        fiber: &mut Fiber,
        deadline: Option<Instant>,
        first: Option<Step>,
        host: &mut dyn Any,
    ) -> Result<Stop, Error> {
        if let Some(profiler) = &mut self.profiler {
            profiler.resume();
        }
        let stopped = self.step_through(fiber, deadline, first, host);
        self.sample_profile(fiber, true);
        stopped
    }

    fn step_through(
        &mut self,
        fiber: &mut Fiber,
        deadline: Option<Instant>,
        first: Option<Step>,
        host: &mut dyn Any,
    ) -> Result<Stop, Error> {
        if let Some(step) = first
            && let Some(ending) = self.conclude(fiber, step, fiber.machine.pc)?
        {
            return Ok(ending);
        }
        let mut countdown = self.check_every();
        loop {
            if countdown == 0 {
                countdown = self.check_every();
                self.sample_profile(fiber, false);
                if self.heap.wants_collection() {
                    self.collect_with(fiber);
                }
                if self.interrupter.take() {
                    if let Some(ending) =
                        self.conclude(fiber, Err(interrupted()), fiber.machine.pc)?
                    {
                        return Ok(ending);
                    }
                    continue;
                }
                if fiber.atomic == 0 && deadline.is_some_and(|deadline| Instant::now() >= deadline)
                {
                    return Ok(Stop::Paused);
                }
            }
            countdown -= 1;
            let at = fiber.machine.pc;
            let Some(&op) = fiber.machine.code.ops.get(at) else {
                return Err(Error::new("The code ended without returning"));
            };
            fiber.machine.pc += 1;
            let step = self.execute(&mut fiber.machine, op, host);
            if !matches!(step, Ok(Flow::Next))
                && let Some(ending) = self.conclude(fiber, step, at)?
            {
                return Ok(ending);
            }
        }
    }

    #[inline(never)]
    fn conclude(
        &mut self,
        fiber: &mut Fiber,
        step: Step,
        at: usize,
    ) -> Result<Option<Stop>, Error> {
        let outcome = match step {
            Ok(flow) => self.switch(fiber, flow),
            Err(error) => Err(error),
        };
        match outcome {
            Ok(ending) => Ok(ending),
            Err(error) => {
                self.recover(fiber, error, at)?;
                Ok(None)
            }
        }
    }

    fn switch(&mut self, fiber: &mut Fiber, flow: Flow) -> Result<Option<Stop>, Error> {
        let mut flow = flow;
        loop {
            flow = match flow {
                Flow::Next => return Ok(None),
                Flow::Land(landing, value) => fiber
                    .machine
                    .land(landing, value)
                    .map_or(Flow::Next, Flow::Return),
                Flow::Return(value) => {
                    let Some(caller) = fiber.callers.pop() else {
                        return Ok(Some(Stop::Finished(value)));
                    };
                    self.finish_generator(caller.generator);
                    fiber.machine = caller.machine;
                    Flow::Land(caller.landing, Value::Nil)
                }
                Flow::Yield(value, landing) => {
                    let Some(caller) = fiber.callers.pop() else {
                        return Err(Error::new("yield is only allowed inside a generator"));
                    };
                    let machine = std::mem::replace(&mut fiber.machine, caller.machine);
                    if let Some(generator) = self.heap.generator_mut(caller.generator) {
                        *generator = Generator::Ready(Box::new(machine), landing);
                    }
                    Flow::Land(caller.landing, value)
                }
                Flow::Resume(generator, landing) => match self.take_generator(generator)? {
                    None => Flow::Land(landing, Value::Nil),
                    Some((machine, resumed)) => {
                        let machine = std::mem::replace(&mut fiber.machine, machine);
                        fiber.callers.push(Caller {
                            machine,
                            landing,
                            generator,
                        });
                        Flow::Land(resumed, Value::Nil)
                    }
                },
                Flow::Atomic(entering, landing) => {
                    fiber.atomic = if entering {
                        fiber.atomic + 1
                    } else {
                        fiber.atomic.saturating_sub(1)
                    };
                    Flow::Land(landing, Value::Nil)
                }
                Flow::Await(..) | Flow::Send(..) | Flow::Receive(..) | Flow::Sleep(..)
                    if fiber.atomic > 0 =>
                {
                    return Err(Error::new(
                        "atomically blocks can't wait; await outside the block",
                    ));
                }
                Flow::Await(..) | Flow::Send(..) | Flow::Receive(..) | Flow::Sleep(..) => {
                    let (wait, landing) = self.wait_of(flow)?;
                    let Some(step) = self.attempt(&wait, landing) else {
                        self.blocked_on(&wait);
                        return Ok(Some(Stop::Blocked(wait, landing)));
                    };
                    step?
                }
            };
        }
    }

    fn expected(&self, name: &str, wanted: &str, value: Value) -> Error {
        Error::new(format!(
            "{name} expects {wanted}, got {}",
            print::write(&self.heap, value)
        ))
        .of(ErrorKind::Type)
    }

    fn wait_of(&self, flow: Flow) -> Result<(Wait, Landing), Error> {
        match flow {
            Flow::Await(promise, landing) => {
                self.heap
                    .promise_of(promise)
                    .ok_or_else(|| self.expected("await", "a promise", promise))?;
                Ok((Wait::Promise(promise), landing))
            }
            Flow::Receive(channel, landing) => {
                self.heap
                    .channel_of(channel)
                    .ok_or_else(|| self.expected("receive", "a channel", channel))?;
                Ok((Wait::Receive(channel), landing))
            }
            Flow::Send(channel, item, landing) => {
                self.heap
                    .channel_of(channel)
                    .ok_or_else(|| self.expected("send", "a channel", channel))?;
                let item = self.portable(item, "through a channel")?;
                Ok((Wait::Send(channel, item), landing))
            }
            Flow::Sleep(time, landing) => {
                let millis = match time {
                    Value::Int(millis) => to_float(millis),
                    Value::Float(millis) => millis,
                    _ => return Err(self.expected("sleep", "a number of milliseconds", time)),
                };
                let delay =
                    Duration::try_from_secs_f64(millis.max(0.0) / 1000.0).unwrap_or(Duration::MAX);
                let until = Instant::now()
                    .checked_add(delay)
                    .unwrap_or_else(Instant::now);
                Ok((Wait::Until(until), landing))
            }
            _ => Err(Error::new(
                "Only promises, channels and timers can be waited on",
            )),
        }
    }

    fn blocked_on(&mut self, wait: &Wait) {
        match wait {
            Wait::Promise(promise) => {
                self.heap
                    .set_promise(*promise, Promise::Pending { awaited: true });
            }
            Wait::Until(_) => {}
            Wait::Receive(channel) | Wait::Send(channel, _) => {
                if let Some(channel) = self.heap.channel_of(*channel) {
                    channel.watch(self.heap.signal());
                }
            }
        }
    }

    fn take_generator(&mut self, value: Value) -> Result<Option<(Machine, Landing)>, Error> {
        if self.heap.generator_of(value).is_none() {
            return Err(self.expected("next", "a generator", value));
        }
        let Some(generator) = self.heap.generator_mut(value) else {
            return Ok(None);
        };
        match std::mem::replace(generator, Generator::Running) {
            Generator::Ready(machine, landing) => Ok(Some((*machine, landing))),
            Generator::Running => Err(Error::new("This generator is already running")),
            Generator::Done => {
                *generator = Generator::Done;
                Ok(None)
            }
        }
    }

    fn finish_generator(&mut self, value: Value) {
        if let Some(generator) = self.heap.generator_mut(value) {
            *generator = Generator::Done;
        }
    }

    fn recover(&mut self, fiber: &mut Fiber, error: Error, at: usize) -> Result<(), Error> {
        let mut error = match fiber.machine.span(at) {
            Some(span) => error.or_at(span),
            None => error,
        };
        if error.traceback.is_empty() {
            error.traceback = self
                .reraised(&error)
                .unwrap_or_else(|| fiber.machine.traceback(at));
        }
        loop {
            if let Some(handler) = fiber.machine.unwind() {
                let value = self.error_value(&error)?;
                self.last_raised = Some((value, error.traceback.clone()));
                fiber.machine.set(handler.register, value);
                return Ok(());
            }
            let Some(caller) = fiber.callers.pop() else {
                return Err(error);
            };
            self.finish_generator(caller.generator);
            fiber.machine = caller.machine;
            let call = fiber.machine.pc.saturating_sub(1);
            error.traceback.extend(fiber.machine.traceback(call));
        }
    }

    fn execute(&mut self, machine: &mut Machine, op: Op, host: &mut dyn Any) -> Step {
        match op {
            Op::Constant { to, index } => {
                let value = machine.code.constants[index as usize];
                machine.set(to, value);
            }
            Op::Global { to, name, cache } => {
                let cached = machine
                    .code
                    .globals
                    .get(usize::from(cache))
                    .and_then(Cell::get);
                let value = match cached {
                    Some((epoch, value)) if epoch == self.epoch => value,
                    _ => self.global(machine, name, cache)?,
                };
                machine.set(to, value);
            }
            Op::Move { to, from } => machine.set(to, machine.get(from)),
            Op::Itself { to } => machine.set(to, machine.closure),
            Op::Captured { to, index } => {
                let value = self.captured(machine.closure, index)?;
                machine.set(to, value);
            }
            Op::Call { base, count } => return self.call(machine, base, count, host),
            Op::TailCall { base, count } => return self.tail_call(machine, base, count, host),
            Op::Prim {
                prim,
                to,
                a,
                b,
                base,
                cache,
            } => return self.prim(machine, prim, [to, a, b, base], cache, host),
            Op::Jump { to } => machine.pc = to as usize,
            Op::JumpUnless { test, to } => {
                if !machine.get(test).truthy() {
                    machine.pc = to as usize;
                }
            }
            Op::Return { from } => {
                return Ok(machine
                    .leave(machine.get(from))
                    .map_or(Flow::Next, Flow::Return));
            }
            _ => self.rare(machine, op)?,
        }
        Ok(Flow::Next)
    }

    #[inline(never)]
    fn rare(&mut self, machine: &mut Machine, op: Op) -> Result<(), Error> {
        match op {
            Op::DefineMacro { name, from } => {
                self.define_macro(machine.code.module, name, machine.get(from));
            }
            Op::Document { name, doc } => {
                let text = self
                    .heap
                    .text(machine.code.constants[doc as usize])
                    .map(str::to_owned);
                let span = machine.code.spans.get(machine.pc.saturating_sub(1));
                let place = machine
                    .code
                    .source
                    .clone()
                    .zip(span.copied())
                    .map(|(source, span)| Place { source, span });
                self.document(machine.code.module, name, text, place);
            }
            Op::DefineGlobal { name, from } => {
                self.define(machine.code.module, name, machine.get(from));
            }
            Op::SetGlobal { name, from } => {
                let value = machine.get(from);
                let Some(slot) = self.assignable(machine.code.module, name) else {
                    return Err(not_defined(name));
                };
                *slot = value;
            }
            Op::JumpIfDefined { name, to } => {
                if self.defined(machine.code.module, name) {
                    machine.pc = to as usize;
                }
            }
            Op::Closure { to, function } => {
                let value = self.close(machine, function)?;
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
            Op::JumpIfSupplied { bit, to } => {
                if machine.missing & (1 << bit) == 0 {
                    machine.pc = to as usize;
                }
            }
            Op::Constant { .. }
            | Op::Global { .. }
            | Op::Move { .. }
            | Op::Itself { .. }
            | Op::Captured { .. }
            | Op::Call { .. }
            | Op::TailCall { .. }
            | Op::Prim { .. }
            | Op::Jump { .. }
            | Op::JumpUnless { .. }
            | Op::Return { .. } => {}
        }
        Ok(())
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
                machine.handlers.push(machine::Handler {
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

    fn arguments(
        &mut self,
        machine: &mut Machine,
        slot: usize,
        count: u16,
    ) -> Result<usize, Error> {
        let count = usize::from(count);
        if self.apply != Value::Nil && machine.stack[slot] == self.apply {
            return self.spread(machine, slot, count);
        }
        Ok(count)
    }

    fn target(&self, callee: Value, count: usize) -> Result<Target, Error> {
        if let Some(closure) = self.heap.closure_of(callee) {
            return Ok(Target::Code(Rc::clone(&closure.code)));
        }
        let Some(native) = self.heap.native_of(callee) else {
            return Ok(Target::Data);
        };
        native.check(count)?;
        Ok(match native.control {
            Some(control) => Target::Control(control),
            None => Target::Native(Rc::clone(&native.function), native.name, native.awaits),
        })
    }

    fn promised(&self, arguments: &[Value]) -> Option<(usize, Value)> {
        arguments
            .iter()
            .position(|argument| self.heap.promise_of(*argument).is_some())
            .map(|index| (index, arguments[index]))
    }

    fn await_argument(
        &self,
        machine: &mut Machine,
        slot: usize,
        arguments: &std::ops::RangeInclusive<usize>,
    ) -> Flow {
        let Some((index, promise)) = self.promised(&machine.stack[arguments.clone()]) else {
            return Flow::Next;
        };
        machine.pc -= 1;
        Flow::Await(promise, Landing::Register(slot + 1 + index))
    }

    fn call(&mut self, machine: &mut Machine, base: u16, count: u16, host: &mut dyn Any) -> Step {
        let slot = machine.base + usize::from(base);
        self.call_into(machine, slot, count, slot, host)
    }

    fn call_into(
        &mut self,
        machine: &mut Machine,
        slot: usize,
        count: u16,
        result: usize,
        host: &mut dyn Any,
    ) -> Step {
        let spread = self.apply != Value::Nil && machine.stack[slot] == self.apply;
        let count = self.arguments(machine, slot, count)?;
        let callee = self.constructor_of(machine.stack[slot]);
        let arguments = slot + 1..=slot + count;
        match self.target(callee, count)? {
            Target::Code(function) => {
                if let Some(profiler) = &mut self.profiler {
                    profiler.called(&function, &self.modules);
                }
                let binding = bind(&mut self.heap, &function, &machine.stack[arguments])?;
                machine.enter(function, callee, slot, result, binding)?;
            }
            Target::Native(_, _, true)
                if !spread && self.promised(&machine.stack[arguments.clone()]).is_some() =>
            {
                return Ok(self.await_argument(machine, slot, &arguments));
            }
            Target::Native(function, name, _) => {
                let started = self.profiler.is_some().then(Instant::now);
                machine.stack[result] = function(&mut self.heap, host, &machine.stack[arguments])?;
                self.timed(name, started, machine);
            }
            Target::Control(control) => {
                return self.control(machine, control, slot, count, Landing::Register(result));
            }
            Target::Data => {
                machine.stack[result] = call_data(&self.heap, callee, &machine.stack[arguments])?;
            }
        }
        Ok(Flow::Next)
    }

    fn prim(
        &mut self,
        machine: &mut Machine,
        prim: Prim,
        [to, a, b, base]: [Register; 4],
        cache: u16,
        host: &mut dyn Any,
    ) -> Step {
        let cached = machine
            .code
            .globals
            .get(usize::from(cache))
            .and_then(Cell::get);
        let callee = match cached {
            Some((epoch, value)) if epoch == self.epoch => value,
            _ => self.global(machine, Sym::intern(prim.name()), cache)?,
        };
        let (first, second) = (machine.get(a), machine.get(b));
        if callee == self.prims[prim as usize]
            && let Some(value) = prims::quick_prim(&self.heap, prim, first, second)
        {
            machine.set(to, value);
            return Ok(Flow::Next);
        }
        let slot = machine.base + usize::from(base);
        machine.stack[slot] = callee;
        machine.stack[slot + 1] = first;
        machine.stack[slot + 2] = second;
        let result = machine.base + usize::from(to);
        self.call_into(machine, slot, 2, result, host)
    }

    fn tail_call(
        &mut self,
        machine: &mut Machine,
        base: u16,
        count: u16,
        host: &mut dyn Any,
    ) -> Step {
        let slot = machine.base + usize::from(base);
        let spread = self.apply != Value::Nil && machine.stack[slot] == self.apply;
        let count = self.arguments(machine, slot, count)?;
        let callee = self.constructor_of(machine.stack[slot]);
        let arguments = slot + 1..=slot + count;
        let result = match self.target(callee, count)? {
            Target::Code(function) => {
                if let Some(profiler) = &mut self.profiler {
                    profiler.called(&function, &self.modules);
                }
                let binding = bind(&mut self.heap, &function, &machine.stack[arguments])?;
                machine.replace(function, callee, slot, count, binding);
                return Ok(Flow::Next);
            }
            Target::Native(_, _, true)
                if !spread && self.promised(&machine.stack[arguments.clone()]).is_some() =>
            {
                return Ok(self.await_argument(machine, slot, &arguments));
            }
            Target::Native(function, name, _) => {
                let started = self.profiler.is_some().then(Instant::now);
                let result = function(&mut self.heap, host, &machine.stack[arguments])?;
                self.timed(name, started, machine);
                result
            }
            Target::Control(control) => {
                return self.control(machine, control, slot, count, Landing::Leave);
            }
            Target::Data => call_data(&self.heap, callee, &machine.stack[arguments])?,
        };
        Ok(machine.leave(result).map_or(Flow::Next, Flow::Return))
    }

    fn control(
        &mut self,
        machine: &Machine,
        control: Control,
        slot: usize,
        count: usize,
        landing: Landing,
    ) -> Step {
        let arguments = &machine.stack[slot + 1..=slot + count];
        let argument = arguments.first().copied().unwrap_or(Value::Nil);
        Ok(match control {
            Control::Yield => Flow::Yield(argument, landing),
            Control::Next => Flow::Resume(argument, landing),
            Control::Await => Flow::Await(argument, landing),
            Control::Generator => Flow::Land(landing, self.make_generator(argument)?),
            Control::Spawn => Flow::Land(landing, self.spawn_promise(argument)?),
            Control::Send => Flow::Send(
                argument,
                arguments.get(1).copied().unwrap_or(Value::Nil),
                landing,
            ),
            Control::Receive => Flow::Receive(argument, landing),
            Control::Worker => Flow::Land(landing, self.start_worker(arguments)?),
            Control::Sleep => Flow::Sleep(argument, landing),
            Control::AtomicEnter => Flow::Atomic(true, landing),
            Control::AtomicLeave => Flow::Atomic(false, landing),
            Control::ProfileStart => Flow::Land(landing, Value::Bool(self.start_profiling())),
            Control::ProfileStop => {
                let profile = self.stop_profiling();
                Flow::Land(landing, self.profile_value(profile.as_ref())?)
            }
            Control::ProfileSnapshot => {
                let profile = self.profiler.as_ref().map(|profiler| profiler.profile());
                Flow::Land(landing, self.profile_value(profile.as_ref())?)
            }
        })
    }

    pub fn start_profiling(&mut self) -> bool {
        if self.profiler.is_some() {
            return false;
        }
        self.profiler = Some(Box::new(Profiler::new()));
        true
    }

    pub fn stop_profiling(&mut self) -> Option<Profile> {
        self.profiler.take().map(|profiler| profiler.profile())
    }

    fn profile_value(&mut self, profile: Option<&Profile>) -> Result<Value, Error> {
        profile.map_or(Ok(Value::Nil), |profile| profile.to_value(&mut self.heap))
    }

    fn timed(&mut self, name: Sym, started: Option<Instant>, machine: &Machine) {
        if let (Some(started), Some(profiler)) = (started, &mut self.profiler) {
            profiler.native(name, started.elapsed(), &machine.frames, &self.modules);
        }
    }

    fn check_every(&self) -> u32 {
        if self.profiler.is_some() {
            PROFILED_EVERY
        } else {
            CHECK_EVERY
        }
    }

    fn sample_profile(&mut self, fiber: &Fiber, pausing: bool) {
        let Some(profiler) = &mut self.profiler else {
            return;
        };
        let stack: Vec<&Rc<Code>> = fiber
            .callers
            .iter()
            .map(|caller| &caller.machine)
            .chain([&fiber.machine])
            .flat_map(|machine| machine.frames.iter().map(|frame| &frame.code))
            .collect();
        if pausing {
            profiler.pause(&stack, &self.modules);
        } else {
            profiler.sample(&stack, &self.modules);
        }
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

    #[inline(never)]
    fn global(&mut self, machine: &Machine, name: Sym, cache: u16) -> Result<Value, Error> {
        let site = machine.code.globals.get(usize::from(cache));
        self.global_lookups += 1;
        let value = self
            .lookup(machine.code.module, name)
            .ok_or_else(|| not_defined(name))?;
        if let Some(site) = site {
            site.set(Some((self.epoch, value)));
        }
        Ok(value)
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

    fn base_roots(&self, roots: &mut Vec<Value>, codes: &mut Vec<Rc<Code>>) {
        roots.extend(
            self.modules
                .iter()
                .flat_map(|module| module.globals.values().chain(module.macros.values()))
                .copied(),
        );
        roots.extend(self.error_types.values().copied());
        roots.extend(self.last_raised.as_ref().map(|(value, _)| *value));
        roots.push(self.apply);
        roots.extend(self.prims);
        roots.extend(self.running);
        self.task_roots(roots, codes);
    }

    fn reraised(&self, error: &Error) -> Option<Vec<Trace>> {
        let (value, frames) = self.last_raised.as_ref()?;
        error
            .raised
            .is_some_and(|raised| identical(raised, *value))
            .then(|| frames.clone())
    }

    fn frames_value(&mut self, frames: &[Trace]) -> Result<Value, Error> {
        let mut items = Vec::with_capacity(frames.len());
        for frame in frames {
            let (line, column) = frame.line_column();
            let function = match frame.function {
                Some(name) => self.heap.string(name.name()).map_err(full)?,
                None => Value::Nil,
            };
            let file = match &frame.source {
                Some(source) => self.heap.string(&source.name).map_err(full)?,
                None => Value::Nil,
            };
            let fields = [
                (Value::keyword("fn"), function),
                (Value::keyword("file"), file),
                (
                    Value::keyword("line"),
                    Value::Int(i64::try_from(line).unwrap_or(0)),
                ),
                (
                    Value::keyword("column"),
                    Value::Int(i64::try_from(column).unwrap_or(0)),
                ),
            ];
            let mut map = Map::EMPTY;
            for (key, value) in fields {
                map = map.assoc(&mut self.heap, key, value).map_err(full)?;
            }
            items.push(self.heap.map(map).map_err(full)?);
        }
        make_vector(&mut self.heap, &items)
    }

    pub(crate) fn error_value(&mut self, error: &Error) -> Result<Value, Error> {
        if error.kind == ErrorKind::Interrupt {
            return Ok(interrupt_marker());
        }
        let value = if let Some(value) = error.raised {
            value
        } else {
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
                .map_err(full)?
        };
        let slot = self
            .heap
            .record_of(value)
            .and_then(|record| record.slot(&self.heap, Sym::intern("traceback")))
            .filter(|slot| {
                self.heap
                    .record_of(value)
                    .and_then(|record| record.fields.get(*slot))
                    == Some(&Value::Nil)
            });
        if let Some(slot) = slot {
            let traceback = self.frames_value(&error.traceback)?;
            self.heap.record_set(value, slot, traceback);
        }
        Ok(value)
    }

    pub fn fresh_mark(&mut self, module: ModuleId) -> u32 {
        self.marks += 1;
        self.mark_modules.insert(self.marks, module);
        self.marks
    }

    pub fn invoke(
        &mut self,
        function: Value,
        arguments: &[Value],
        span: Span,
        host: &mut dyn Any,
    ) -> Result<Value, Error> {
        let code = Self::call_code(function, arguments, span)?;
        self.run(&Rc::new(code), host)
    }

    fn call_code(function: Value, arguments: &[Value], span: Span) -> Result<Code, Error> {
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
        Ok(Code {
            registers: count + 1,
            spans: vec![span; ops.len()],
            ops,
            constants,
            ..Code::default()
        })
    }

    pub fn collect(&mut self) -> usize {
        let mut roots = Vec::new();
        let mut codes = Vec::new();
        self.base_roots(&mut roots, &mut codes);
        self.heap.collect(&roots, &codes)
    }

    fn collect_with(&mut self, fiber: &Fiber) {
        let mut roots = Vec::new();
        let mut codes = Vec::new();
        self.base_roots(&mut roots, &mut codes);
        fiber.roots(&mut roots, &mut codes);
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
}
