use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::record::Family;
use crate::vm::build::{interrupt_marker, raised};

pub(crate) fn catches(heap: &Heap, value: Value, kind: Value) -> bool {
    let (Some(wanted), Some(record)) = (heap.record_type_of(kind), heap.record_of(value)) else {
        return false;
    };
    match wanted.family {
        Family::AnyError => heap
            .record_type_of(record.kind)
            .is_some_and(|found| matches!(found.family, Family::Error | Family::AnyError)),
        Family::Error | Family::Plain | Family::Quit => record.kind == kind,
    }
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "raise")
        .arity(1, Some(1))
        .function(|heap, arguments| Err(raised(heap, arguments[0])));
    isolate
        .native("ox", "#catchable?")
        .arity(1, Some(1))
        .function(|_, arguments| Ok(Value::Bool(arguments[0] != interrupt_marker())));
    isolate
        .native("ox", "#catches?")
        .arity(2, Some(2))
        .function(|heap, arguments| Ok(Value::Bool(catches(heap, arguments[0], arguments[1]))));
}
