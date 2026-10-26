use crate::builtins::collections;
use crate::builtins::output;
use crate::error::Error;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::native::Drains;
use crate::value::{Value, print};
use crate::vm::build::{make_set, make_vector};

use super::sequences::zipped;

pub(super) const PLAIN_HEADS: [&str; 12] = [
    "and", "or", "not", "if", "when", "unless", "cond", "let", "do", "match", "try", ".",
];

pub(super) fn shown_plainly(heap: &Heap, form: Value) -> bool {
    matches!(
        form,
        Value::Int(_)
            | Value::Float(_)
            | Value::Keyword(_)
            | Value::Nil
            | Value::Bool(_)
            | Value::Char(_)
    ) || heap.bigint_of(form).is_some()
        || heap.text(form).is_some()
}

fn explain(heap: &mut Heap, form: Value, values: Value) -> Result<Value, Error> {
    let operands = collections::rest(heap, form)?;
    let mut text = print::write(heap, form);
    for pair in zipped(heap, "explain", &[operands, values])? {
        if !shown_plainly(heap, pair[0]) {
            text.push_str("\n  ");
            text.push_str(&print::write(heap, pair[0]));
            text.push_str(" => ");
            text.push_str(&print::write(heap, pair[1]));
        }
    }
    heap.string(&text).map_err(Error::from)
}

pub(super) fn install(isolate: &mut Isolate) {
    let heap = isolate.heap_mut();
    let tests = make_vector(heap, &[])
        .and_then(|tests| heap.atom(tests).map_err(Error::from))
        .unwrap_or(Value::Nil);
    let heads = PLAIN_HEADS
        .iter()
        .map(|head| heap.string(head).map_err(Error::from))
        .collect::<Result<Vec<_>, Error>>()
        .and_then(|heads| make_set(heap, &heads))
        .unwrap_or(Value::Nil);
    isolate.define("registered-tests", tests);
    isolate.define("plain-heads", heads);
    isolate
        .native("ox", "shown-plainly?")
        .arity(1, Some(1))
        .function(|heap, arguments| Ok(Value::Bool(shown_plainly(heap, arguments[0]))));
    isolate
        .native("ox", "explain")
        .arity(2, Some(2))
        .drains(Drains::At(1))
        .function(|heap, arguments| explain(heap, arguments[0], arguments[1]));
    isolate
        .native("ox", "profile-report")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let report = collections::get(heap, &[arguments[0], Value::keyword("report")]);
            output(heap, &[report], false)
        });
}
