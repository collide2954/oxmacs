use std::rc::Rc;

use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::equal::identical;
use crate::vm::routine::{Resumable, Started, captured};

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
}
