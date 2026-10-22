use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::native::Control;
use crate::value::Value;
use crate::value::print;
use crate::value::promise::Promise;
use crate::vm::build::full;
use crate::vm::machine::Generator;

fn expected(heap: &Heap, name: &str, wanted: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} expects {wanted}, got {}",
        print::write(heap, value)
    ))
    .of(ErrorKind::Type)
}

fn settle(heap: &mut Heap, name: &str, promise: Value, outcome: Promise) -> Result<Value, Error> {
    match heap.promise_of(promise) {
        Some(Promise::Pending { .. }) => {
            heap.set_promise(promise, outcome);
            Ok(promise)
        }
        Some(_) => Err(Error::new("This promise was already delivered")),
        None => Err(expected(heap, name, "a promise", promise)),
    }
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_control("yield", 0, Some(1), Control::Yield);
    isolate.define_control("next", 1, Some(1), Control::Next);
    isolate.define_control("await", 1, Some(1), Control::Await);
    isolate.define_control("generator", 1, Some(1), Control::Generator);
    isolate.define_control("spawn-call", 1, Some(1), Control::Spawn);
    isolate.define_native("promise", 0, Some(0), |heap, _| {
        heap.promise(Promise::Pending { awaited: false })
            .map_err(full)
    });
    isolate.define_native("deliver", 2, Some(2), |heap, arguments| {
        settle(
            heap,
            "deliver",
            arguments[0],
            Promise::Delivered(arguments[1]),
        )
    });
    isolate.define_native("fail", 2, Some(2), |heap, arguments| {
        settle(heap, "fail", arguments[0], Promise::Failed(arguments[1]))
    });
    isolate.define_native("done?", 1, Some(1), |heap, arguments| {
        if let Some(generator) = heap.generator_of(arguments[0]) {
            return Ok(Value::Bool(matches!(generator, Generator::Done)));
        }
        match heap.promise_of(arguments[0]) {
            Some(promise) => Ok(Value::Bool(!matches!(promise, Promise::Pending { .. }))),
            None => Err(expected(
                heap,
                "done?",
                "a generator or a promise",
                arguments[0],
            )),
        }
    });
    isolate.define_native("generator?", 1, Some(1), |heap, arguments| {
        Ok(Value::Bool(heap.generator_of(arguments[0]).is_some()))
    });
    isolate.define_native("promise?", 1, Some(1), |heap, arguments| {
        Ok(Value::Bool(heap.promise_of(arguments[0]).is_some()))
    });
}
