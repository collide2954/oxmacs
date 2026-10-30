use std::rc::Rc;

use crate::error::Error;
use crate::isolate::Isolate;
use crate::symbol::Sym;
use crate::value::record::{Family, Record, RecordType};
use crate::value::{Value, print};
use crate::vm::Runtime;
use crate::vm::modules::PRELUDE;
use crate::vm::routine::{Resumable, Started, captured};

const ERRORS: [&str; 8] = [
    "error",
    "type-error",
    "arity-error",
    "name-error",
    "range-error",
    "zero-division-error",
    "no-match-error",
    "expectation-failed",
];

fn define(runtime: &mut Runtime, name: &'static str, family: Family) -> Result<(), Error> {
    let text = runtime.heap.string(name)?;
    let shown = print::write(&runtime.heap, text);
    let kind = runtime.heap.record_type(RecordType {
        name: Sym::intern(name),
        fields: Box::new([Sym::intern("message"), Sym::intern("traceback")]),
        constructor: Value::Nil,
        family,
        module: Sym::intern("ox"),
    })?;
    let build = Rc::new(Resumable::closure(
        &format!("(message {shown}) (traceback nil)"),
        move |runtime, callee, arguments| {
            let message = match arguments.first() {
                Some(message) => *message,
                None => runtime.heap.string(name)?,
            };
            let traceback = arguments.get(1).copied().unwrap_or(Value::Nil);
            runtime
                .heap
                .record(Record {
                    kind: captured(&runtime.heap, callee, 0),
                    fields: Box::new([message, traceback]),
                })
                .map(Started::Done)
                .map_err(Error::from)
        },
    ));
    let constructor = runtime.make_closure(&build, Sym::intern(name), 0, Some(2), &[kind])?;
    runtime.heap.set_constructor(kind, constructor);
    runtime.define(PRELUDE, Sym::intern(name), kind);
    Ok(())
}

pub(super) fn install(isolate: &mut Isolate) {
    let kinds = ERRORS
        .iter()
        .map(|name| (*name, Family::Error))
        .chain([("quit", Family::Quit)]);
    for (name, family) in kinds {
        if let Err(error) = define(isolate.runtime_mut(), name, family) {
            panic!("The Ox error type {name} failed to load: {error}");
        }
    }
}
