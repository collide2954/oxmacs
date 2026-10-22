use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;
use crate::value::record::Family;
use crate::vm::build::{interrupt_marker, raised};

fn catches(heap: &Heap, value: Value, kind: Value) -> bool {
    let (Some(wanted), Some(record)) = (heap.record_type_of(kind), heap.record_of(value)) else {
        return false;
    };
    match wanted.family {
        Family::AnyError => heap
            .record_type_of(record.kind)
            .is_some_and(|found| found.family != Family::Plain),
        Family::Error | Family::Plain => record.kind == kind,
    }
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native("raise", 1, Some(1), |heap, arguments| {
        Err(raised(heap, arguments[0]))
    });
    isolate.define_native("#catchable?", 1, Some(1), |_, arguments| {
        Ok(Value::Bool(arguments[0] != interrupt_marker()))
    });
    isolate.define_native("#catches?", 2, Some(2), |heap, arguments| {
        Ok(Value::Bool(catches(heap, arguments[0], arguments[1])))
    });
}
