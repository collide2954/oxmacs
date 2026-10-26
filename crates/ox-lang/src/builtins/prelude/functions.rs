use std::rc::Rc;

use crate::builtins::{collections, not_an_atom};
use crate::error::Error;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::native::Control;
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::equal::{equal, identical};
use crate::value::map::Map;
use crate::vm::Runtime;
use crate::vm::build::{full, make_vector};
use crate::vm::modules::PRELUDE;
use crate::vm::routine::{Forward, Next, Resumable, Routine, Started, captured};

fn all_captured(heap: &Heap, callee: Value) -> Vec<Value> {
    heap.native_of(callee)
        .map(|native| native.captured.to_vec())
        .unwrap_or_default()
}

fn deref(heap: &Heap, atom: Value) -> Result<Value, Error> {
    heap.atom_get(atom)
        .ok_or_else(|| not_an_atom(heap, "deref", atom))
}

#[derive(Debug)]
struct Negate {
    function: Value,
    arguments: Vec<Value>,
    called: bool,
}

impl Routine for Negate {
    fn resume(&mut self, _: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        if self.called {
            return Ok(Next::Return(Value::Bool(!value.truthy())));
        }
        self.called = true;
        Ok(Next::Call(self.function, &self.arguments))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.push(self.function);
        values.extend(&self.arguments);
    }
}

#[derive(Debug)]
struct Compose {
    functions: Vec<Value>,
    arguments: Vec<Value>,
    at: usize,
}

impl Routine for Compose {
    fn resume(&mut self, _: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        let at = self.at;
        self.at += 1;
        if at == 0 {
            return Ok(Next::Call(self.functions[0], &self.arguments));
        }
        let Some(function) = self.functions.get(at) else {
            return Ok(Next::Return(value));
        };
        self.arguments.clear();
        self.arguments.push(value);
        Ok(Next::Call(*function, &self.arguments))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.extend(&self.functions);
        values.extend(&self.arguments);
    }
}

#[derive(Debug)]
struct Juxt {
    functions: Vec<Value>,
    arguments: Vec<Value>,
    out: Vec<Value>,
    started: bool,
}

impl Routine for Juxt {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        if self.started {
            self.out.push(value);
        }
        self.started = true;
        match self.functions.get(self.out.len()) {
            Some(function) => Ok(Next::Call(*function, &self.arguments)),
            None => make_vector(&mut runtime.heap, &self.out).map(Next::Return),
        }
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.extend(&self.functions);
        values.extend(&self.arguments);
        values.extend(&self.out);
    }
}

#[derive(Debug)]
struct Memo {
    function: Value,
    seen: Value,
    key: Value,
    arguments: Vec<Value>,
    called: bool,
}

impl Routine for Memo {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        if !self.called {
            self.called = true;
            return Ok(Next::Call(self.function, &self.arguments));
        }
        let heap = &mut runtime.heap;
        let seen = deref(heap, self.seen)?;
        let grown = collections::assoc(heap, seen, &[self.key, value])?;
        heap.atom_set(self.seen, grown);
        Ok(Next::Return(value))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.extend([self.function, self.seen, self.key]);
        values.extend(&self.arguments);
    }
}

fn memoized(runtime: &mut Runtime, callee: Value, arguments: &[Value]) -> Result<Started, Error> {
    let (function, seen) = (
        captured(&runtime.heap, callee, 0),
        captured(&runtime.heap, callee, 1),
    );
    let heap = &mut runtime.heap;
    let key = make_vector(heap, arguments)?;
    let known = deref(heap, seen)?;
    if collections::contains(heap, known, key)?.truthy() {
        return Ok(Started::Done(collections::get(heap, &[known, key])));
    }
    Ok(Started::Running(Box::new(Memo {
        function,
        seen,
        key,
        arguments: arguments.to_vec(),
        called: false,
    })))
}

#[derive(Debug)]
struct Swap {
    atom: Value,
    function: Value,
    arguments: Vec<Value>,
    called: bool,
}

impl Routine for Swap {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        let heap = &mut runtime.heap;
        let current = deref(heap, self.atom)?;
        if self.called && identical(current, self.arguments[0]) {
            heap.atom_set(self.atom, value);
            return Ok(Next::Return(value));
        }
        self.called = true;
        self.arguments[0] = current;
        Ok(Next::Call(self.function, &self.arguments))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.extend([self.atom, self.function]);
        values.extend(&self.arguments);
    }
}

#[derive(Debug)]
struct Update {
    coll: Value,
    key: Value,
    function: Value,
    arguments: Vec<Value>,
    called: bool,
}

impl Routine for Update {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        if self.called {
            return collections::assoc(&mut runtime.heap, self.coll, &[self.key, value])
                .map(Next::Return);
        }
        self.called = true;
        Ok(Next::Call(self.function, &self.arguments))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.extend([self.coll, self.key, self.function]);
        values.extend(&self.arguments);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Beat {
    Sleep,
    Call,
    Check,
}

#[derive(Debug)]
struct Tick {
    pause: [Value; 1],
    function: Value,
    beat: Beat,
}

impl Routine for Tick {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        match self.beat {
            Beat::Call => {
                self.beat = Beat::Check;
                return Ok(Next::Call(self.function, &[]));
            }
            Beat::Check if equal(&runtime.heap, value, Value::keyword("stop")) => {
                return Ok(Next::Return(Value::Nil));
            }
            Beat::Sleep | Beat::Check => {}
        }
        self.beat = Beat::Call;
        let sleep = runtime.control_value(Control::Sleep);
        Ok(Next::Call(sleep, &self.pause))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.extend(self.pause);
        values.push(self.function);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Yield,
    Apply,
    Store,
}

#[derive(Debug)]
struct Iterate {
    function: Value,
    value: [Value; 1],
    stage: Stage,
}

impl Routine for Iterate {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        match self.stage {
            Stage::Apply => {
                self.stage = Stage::Store;
                return Ok(Next::Call(self.function, &self.value));
            }
            Stage::Store => self.value = [value],
            Stage::Yield => {}
        }
        self.stage = Stage::Apply;
        let yielding = runtime.control_value(Control::Yield);
        Ok(Next::Call(yielding, &self.value))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.push(self.function);
        values.extend(self.value);
    }
}

fn install_makers(isolate: &mut Isolate) {
    let negate = Rc::new(Resumable::closure(
        "& args",
        |runtime, callee, arguments| {
            Ok(Started::Running(Box::new(Negate {
                function: captured(&runtime.heap, callee, 0),
                arguments: arguments.to_vec(),
                called: false,
            })))
        },
    ));
    isolate.define_routine(
        "complement",
        1,
        Some(1),
        None,
        move |runtime, _, arguments| {
            runtime
                .native_closure(&negate, &arguments[..1])
                .map(Started::Done)
        },
    );
    let compose = Rc::new(Resumable::closure(
        "& args",
        |runtime, callee, arguments| {
            Ok(Started::Running(Box::new(Compose {
                functions: all_captured(&runtime.heap, callee),
                arguments: arguments.to_vec(),
                at: 0,
            })))
        },
    ));
    isolate.define_routine("comp", 0, None, None, move |runtime, _, arguments| {
        if arguments.is_empty() {
            let identity = runtime
                .lookup(PRELUDE, Sym::intern("identity"))
                .unwrap_or(Value::Nil);
            return Ok(Started::Done(identity));
        }
        let order: Vec<Value> = arguments.iter().rev().copied().collect();
        runtime.native_closure(&compose, &order).map(Started::Done)
    });
    let fill = Rc::new(Resumable::closure(
        "& more",
        |runtime, callee, arguments| {
            let mut given = all_captured(&runtime.heap, callee);
            let function = given.remove(0);
            given.extend_from_slice(arguments);
            Ok(Started::Running(Box::new(Forward::new(function, given))))
        },
    ));
    isolate.define_routine("partial", 1, None, None, move |runtime, _, arguments| {
        runtime.native_closure(&fill, arguments).map(Started::Done)
    });
    let juxt = Rc::new(Resumable::closure(
        "& args",
        |runtime, callee, arguments| {
            Ok(Started::Running(Box::new(Juxt {
                functions: all_captured(&runtime.heap, callee),
                arguments: arguments.to_vec(),
                out: Vec::new(),
                started: false,
            })))
        },
    ));
    isolate.define_routine("juxt", 0, None, None, move |runtime, _, arguments| {
        runtime.native_closure(&juxt, arguments).map(Started::Done)
    });
    let memo = Rc::new(Resumable::closure("& args", memoized));
    isolate.define_routine("memoize", 1, Some(1), None, move |runtime, _, arguments| {
        let empty = runtime.heap.map(Map::EMPTY).map_err(full)?;
        let seen = runtime.heap.atom(empty).map_err(full)?;
        runtime
            .native_closure(&memo, &[arguments[0], seen])
            .map(Started::Done)
    });
}

fn install_tasks(isolate: &mut Isolate) {
    let tick = Rc::new(Resumable::closure("", |runtime, callee, _| {
        Ok(Started::Running(Box::new(Tick {
            pause: [captured(&runtime.heap, callee, 0)],
            function: captured(&runtime.heap, callee, 1),
            beat: Beat::Sleep,
        })))
    }));
    isolate.define_routine("every", 2, Some(2), None, move |runtime, _, arguments| {
        let ticking = runtime.native_closure(&tick, arguments)?;
        runtime.spawn_promise(ticking).map(Started::Done)
    });
    let step = Rc::new(Resumable::closure("", |runtime, callee, _| {
        Ok(Started::Running(Box::new(Iterate {
            function: captured(&runtime.heap, callee, 0),
            value: [captured(&runtime.heap, callee, 1)],
            stage: Stage::Yield,
        })))
    }));
    isolate.define_routine("iterate", 2, Some(2), None, move |runtime, _, arguments| {
        let stepping = runtime.native_closure(&step, arguments)?;
        runtime.make_generator(stepping).map(Started::Done)
    });
}

pub(super) fn install(isolate: &mut Isolate) {
    isolate.define_native("identity", 1, Some(1), |_, arguments| Ok(arguments[0]));
    isolate.define_native("not", 1, Some(1), |_, arguments| {
        Ok(Value::Bool(!arguments[0].truthy()))
    });
    isolate.define_native("nil?", 1, Some(1), |_, arguments| {
        Ok(Value::Bool(identical(arguments[0], Value::Nil)))
    });
    isolate.define_native("some?", 1, Some(1), |_, arguments| {
        Ok(Value::Bool(!identical(arguments[0], Value::Nil)))
    });
    let always = Rc::new(Resumable::closure("& _", |runtime, callee, _| {
        Ok(Started::Done(captured(&runtime.heap, callee, 0)))
    }));
    isolate.define_routine(
        "constantly",
        1,
        Some(1),
        None,
        move |runtime, _, arguments| {
            runtime
                .native_closure(&always, &arguments[..1])
                .map(Started::Done)
        },
    );
    isolate.define_routine("swap!", 2, None, None, |runtime, _, arguments| {
        let current = deref(&runtime.heap, arguments[0])?;
        let mut given = vec![current];
        given.extend_from_slice(&arguments[2..]);
        Ok(Started::Running(Box::new(Swap {
            atom: arguments[0],
            function: arguments[1],
            arguments: given,
            called: false,
        })))
    });
    isolate.define_routine("update", 3, None, None, |runtime, _, arguments| {
        let current = collections::get(&runtime.heap, &arguments[..2]);
        let mut given = vec![current];
        given.extend_from_slice(&arguments[3..]);
        Ok(Started::Running(Box::new(Update {
            coll: arguments[0],
            key: arguments[1],
            function: arguments[2],
            arguments: given,
            called: false,
        })))
    });
    install_makers(isolate);
    install_tasks(isolate);
}
