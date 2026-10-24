use std::fmt;
use std::rc::Rc;

use crate::bytecode::{Code, Op};
use crate::error::Error;
use crate::heap::Heap;
use crate::native::Control;
use crate::symbol::Sym;
use crate::value::Value;
use crate::vm::Runtime;
use crate::vm::build::{make_vector, raised};
use crate::vm::machine::Generator;

pub(crate) const RAISED: u32 = 3;

#[derive(Debug)]
pub(crate) enum Next<'a> {
    Return(Value),
    Call(Value, &'a [Value]),
    Catch(Value, &'a [Value]),
}

#[derive(Debug)]
pub(crate) enum Started {
    Done(Value),
    Running(Box<dyn Routine>),
}

pub(crate) trait Routine: fmt::Debug {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error>;

    fn raised(&mut self, runtime: &mut Runtime, error: Value) -> Result<Next<'_>, Error> {
        Err(raised(&runtime.heap, error))
    }

    fn roots(&self, values: &mut Vec<Value>);
}

pub(crate) type Start = dyn Fn(&mut Runtime, Value, &[Value]) -> Result<Started, Error>;

pub(crate) struct Resumable {
    pub(crate) start: Box<Start>,
    pub(crate) code: Rc<Code>,
}

impl fmt::Debug for Resumable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Resumable")
            .field("name", &self.code.name.map(Sym::name))
            .finish_non_exhaustive()
    }
}

impl Resumable {
    pub(crate) fn new(name: Sym, start: Box<Start>) -> Self {
        Self {
            start,
            code: Rc::new(Code {
                name: Some(name),
                registers: 3,
                ops: vec![Op::Resume, Op::Invoke, Op::Jump { to: 0 }, Op::Raised],
                ..Code::default()
            }),
        }
    }

    pub(crate) fn drainer() -> Self {
        Self::new(
            Sym::intern("vec"),
            Box::new(|runtime, callee, arguments| {
                Ok(Started::Running(Box::new(Drain::new(
                    &runtime.heap,
                    callee,
                    arguments,
                ))))
            }),
        )
    }
}

pub(crate) fn is_generator(heap: &Heap, value: Value) -> bool {
    heap.generator_of(value).is_some()
}

pub(crate) fn finished(heap: &Heap, generator: Value) -> bool {
    matches!(heap.generator_of(generator), Some(Generator::Done))
}

#[derive(Debug)]
struct Drain {
    callee: Value,
    arguments: Vec<Value>,
    waiting: Vec<usize>,
    draining: Option<usize>,
    items: Vec<Value>,
    called: bool,
}

impl Drain {
    fn new(heap: &Heap, callee: Value, arguments: &[Value]) -> Self {
        let range = heap
            .native_of(callee)
            .and_then(|native| native.drains)
            .map_or(0..0, |drains| drains.range(arguments.len()));
        let mut waiting: Vec<usize> = range
            .filter(|at| is_generator(heap, arguments[*at]))
            .collect();
        waiting.reverse();
        Self {
            callee,
            arguments: arguments.to_vec(),
            waiting,
            draining: None,
            items: Vec::new(),
            called: false,
        }
    }
}

impl Routine for Drain {
    fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
        if self.called {
            return Ok(Next::Return(value));
        }
        if let Some(at) = self.draining {
            if finished(&runtime.heap, self.arguments[at]) {
                self.arguments[at] = make_vector(&mut runtime.heap, &self.items)?;
                self.items.clear();
                self.draining = None;
            } else {
                self.items.push(value);
            }
        }
        if self.draining.is_none() {
            self.draining = self.waiting.pop();
        }
        if let Some(at) = self.draining {
            let next = runtime.control_value(Control::Next);
            return Ok(Next::Call(next, std::slice::from_ref(&self.arguments[at])));
        }
        self.called = true;
        Ok(Next::Call(self.callee, &self.arguments))
    }

    fn roots(&self, values: &mut Vec<Value>) {
        values.push(self.callee);
        values.extend(&self.arguments);
        values.extend(&self.items);
    }
}

#[cfg(test)]
mod tests {
    use super::{Next, Routine, Started};
    use crate::error::Error;
    use crate::isolate::Isolate;
    use crate::library::collections;
    use crate::span::Span;
    use crate::value::Value;
    use crate::vm::Runtime;
    use crate::vm::build::make_vector;

    #[derive(Debug)]
    struct EachCall {
        function: Value,
        items: Vec<Value>,
        at: usize,
        results: Vec<Value>,
    }

    impl Routine for EachCall {
        fn resume(&mut self, runtime: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
            if self.at > 0 {
                if value == Value::keyword("bad") {
                    return Err(Error::new("each-call got :bad"));
                }
                self.results.push(value);
            }
            let Some(item) = self.items.get(self.at) else {
                return make_vector(&mut runtime.heap, &self.results).map(Next::Return);
            };
            self.at += 1;
            Ok(Next::Call(self.function, std::slice::from_ref(item)))
        }

        fn roots(&self, values: &mut Vec<Value>) {
            values.push(self.function);
            values.extend(&self.items);
            values.extend(&self.results);
        }
    }

    #[derive(Debug)]
    struct Guarded {
        first: Value,
        then: Value,
        step: usize,
        held: [Value; 1],
    }

    impl Routine for Guarded {
        fn resume(&mut self, _: &mut Runtime, value: Value) -> Result<Next<'_>, Error> {
            self.step += 1;
            match self.step {
                1 => Ok(Next::Catch(self.first, &[])),
                2 => {
                    self.held = [value];
                    Ok(Next::Call(self.then, &self.held))
                }
                _ => Ok(Next::Return(value)),
            }
        }

        fn raised(&mut self, _: &mut Runtime, error: Value) -> Result<Next<'_>, Error> {
            self.held = [error];
            Ok(Next::Return(Value::keyword("caught")))
        }

        fn roots(&self, values: &mut Vec<Value>) {
            values.extend([self.first, self.then]);
            values.extend(self.held);
        }
    }

    fn isolate() -> Isolate {
        let mut isolate = Isolate::new();
        isolate.define_routine("each-call", 2, Some(2), None, |runtime, _, arguments| {
            let items = collections::sequence(&mut runtime.heap, "each-call", arguments[1])?;
            Ok(Started::Running(Box::new(EachCall {
                function: arguments[0],
                items,
                at: 0,
                results: Vec::new(),
            })))
        });
        isolate.define_routine("guarded", 2, Some(2), None, |_, _, arguments| {
            Ok(Started::Running(Box::new(Guarded {
                first: arguments[0],
                then: arguments[1],
                step: 0,
                held: [Value::Nil],
            })))
        });
        isolate.define_routine("at-once", 1, Some(1), None, |_, _, arguments| {
            Ok(Started::Done(arguments[0]))
        });
        isolate
    }

    fn shown(source: &str) -> String {
        let mut isolate = isolate();
        match isolate.eval(source) {
            Ok(value) => isolate.write(value),
            Err(error) => format!("error: {}", error.message),
        }
    }

    #[test]
    fn routines_call_functions_and_resume_with_their_results() {
        assert_eq!(shown("(each-call (fn [x] (* x 10)) [1 2 3])"), "[10 20 30]");
        assert_eq!(shown("(each-call inc [1 2])"), "[2 3]");
        assert_eq!(shown("(each-call :a [{a: 1} {a: 2}])"), "[1 2]");
        assert_eq!(shown("(each-call inc [])"), "[]");
        assert_eq!(shown("(at-once 5)"), "5");
        assert_eq!(shown("(apply each-call [inc [1]])"), "[2]");
        assert_eq!(shown("each-call"), "#<native each-call>");
    }

    #[test]
    fn routines_nest_and_can_be_called_last() {
        assert_eq!(
            shown("(def (bump xs) (each-call inc xs)) (bump [1 2])"),
            "[2 3]"
        );
        assert_eq!(
            shown("(each-call (fn [xs] (each-call inc xs)) [[1] [2 3]])"),
            "[[2] [3 4]]"
        );
        assert_eq!(
            shown("(def (down n) (if (< n 1) :done (each-call down [(- n 1)]))) (down 3)"),
            "[[[:done]]]"
        );
    }

    #[test]
    fn rust_can_call_routines() {
        let mut isolate = isolate();
        let routine = isolate.eval("each-call").unwrap();
        let function = isolate.eval("inc").unwrap();
        let items = isolate.eval("[1 2]").unwrap();

        let result = isolate.call(routine, &[function, items]).unwrap();

        assert_eq!(isolate.write(result), "[2 3]");
    }

    #[test]
    fn callbacks_can_yield_from_inside_a_routine() {
        assert_eq!(
            shown(
                "(def g (gen (each-call (fn [x] (yield x) (* x 2)) [1 2]))) [(next g) (next g) (next g) (done? g)]"
            ),
            "[1 2 nil true]"
        );
        assert_eq!(shown("(vec (gen (each-call yield [:a :b])))"), "[:a :b]");
    }

    #[test]
    fn callbacks_can_await_and_sleep() {
        assert_eq!(
            shown(
                "(let [p (promise)] (spawn (sleep 5) (deliver p 10)) (each-call (fn [x] (+ x (await p))) [1 2]))"
            ),
            "[11 12]"
        );
        assert_eq!(
            shown("(each-call (fn [x] (sleep 1) (* x 3)) [1 2])"),
            "[3 6]"
        );
        assert_eq!(
            shown(
                "(let [p (promise)] (def t (spawn (each-call (fn [x] (+ x (await p))) [1]))) (deliver p 4) (await t))"
            ),
            "[5]"
        );
    }

    #[test]
    fn errors_from_callbacks_unwind_through_routines() {
        assert_eq!(
            shown(
                "(try (each-call (fn [x] (raise (error \"boom\"))) [1]) (catch error e e.message))"
            ),
            "\"boom\""
        );
        let error = isolate()
            .eval_named("demo.ox", "(def (f x)\n  (/ x 0))\n(each-call f [1])")
            .unwrap_err();
        let frames: Vec<String> = error.traceback.iter().map(ToString::to_string).collect();
        assert_eq!(frames, ["f at demo.ox:2:3", "top level at demo.ox:3:1"]);
    }

    #[test]
    fn errors_inside_a_routine_point_at_its_call() {
        let source = "(+ 1\n (each-call identity [:bad]))";
        let error = isolate().eval_named("demo.ox", source).unwrap_err();
        assert_eq!(error.message, "each-call got :bad");
        assert_eq!(error.span, Some(Span::new(6, 33)));
        let frames: Vec<String> = error.traceback.iter().map(ToString::to_string).collect();
        assert_eq!(frames, ["top level at demo.ox:2:2"]);

        let error = isolate()
            .eval_named("demo.ox", "(+ 1\n (each-call inc [:a]))")
            .unwrap_err();
        assert_eq!(error.span, Some(Span::new(6, 26)));
    }

    #[test]
    fn routines_can_catch_errors_from_their_calls() {
        assert_eq!(
            shown("(guarded (fn [] (raise (error \"x\"))) identity)"),
            ":caught"
        );
        assert_eq!(shown("(guarded (fn [] 5) inc)"), "6");
        assert_eq!(
            shown(
                "(try (guarded (fn [] 1) (fn [x] (raise (error \"late\")))) (catch error e e.message))"
            ),
            "\"late\""
        );
    }

    #[test]
    fn routines_keep_what_they_hold_through_garbage_collection() {
        assert_eq!(
            shown(
                "(each-call (fn [s] (map str (range 120000)) s) [(str \"a\" 1) (str \"b\" 2) (str \"c\" 3)])"
            ),
            "[\"a1\" \"b2\" \"c3\"]"
        );
    }
}
