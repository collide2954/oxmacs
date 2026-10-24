use crate::error::Error;
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::library::collections;
use crate::library::output;
use crate::native::Drains;
use crate::value::{Value, print};
use crate::vm::build::{full, make_set, make_vector};

use super::sequences::zipped;

pub(super) const PLAIN_HEADS: [&str; 11] = [
    "and", "or", "not", "if", "when", "unless", "cond", "let", "do", "match", "try",
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
    for pair in zipped(heap, &[operands, values])? {
        if !shown_plainly(heap, pair[0]) {
            text.push_str("\n  ");
            text.push_str(&print::write(heap, pair[0]));
            text.push_str(" => ");
            text.push_str(&print::write(heap, pair[1]));
        }
    }
    heap.string(&text).map_err(full)
}

pub(super) fn install(isolate: &mut Isolate) {
    let heap = isolate.heap_mut();
    let tests = make_vector(heap, &[])
        .and_then(|tests| heap.atom(tests).map_err(full))
        .unwrap_or(Value::Nil);
    let heads = PLAIN_HEADS
        .iter()
        .map(|head| heap.string(head).map_err(full))
        .collect::<Result<Vec<_>, Error>>()
        .and_then(|heads| make_set(heap, &heads))
        .unwrap_or(Value::Nil);
    isolate.define("registered-tests", tests);
    isolate.define("plain-heads", heads);
    isolate.define_native("shown-plainly?", 1, Some(1), |heap, arguments| {
        Ok(Value::Bool(shown_plainly(heap, arguments[0])))
    });
    isolate.define_draining("explain", 2, Some(2), Drains::At(1), |heap, arguments| {
        explain(heap, arguments[0], arguments[1])
    });
    isolate.define_native("profile-report", 1, Some(1), |heap, arguments| {
        let report = collections::get(heap, &[arguments[0], Value::keyword("report")]);
        output(heap, &[report], false)
    });
}
