use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::value::Value;

fn define(isolate: &mut Isolate, name: &str, test: fn(&Heap, Value) -> bool) {
    isolate.define_native(name, 1, Some(1), move |heap, arguments| {
        Ok(Value::Bool(test(heap, arguments[0])))
    });
}

pub fn install(isolate: &mut Isolate) {
    define(isolate, "int?", |heap, value| {
        matches!(value, Value::Int(_)) || heap.bigint_of(value).is_some()
    });
    define(isolate, "float?", |_, value| {
        matches!(value, Value::Float(_))
    });
    define(isolate, "number?", |heap, value| {
        matches!(value, Value::Int(_) | Value::Float(_)) || heap.bigint_of(value).is_some()
    });
    define(isolate, "string?", |heap, value| heap.text(value).is_some());
    define(isolate, "char?", |_, value| matches!(value, Value::Char(_)));
    define(isolate, "keyword?", |_, value| {
        matches!(value, Value::Keyword(_))
    });
    define(isolate, "symbol?", |heap, value| {
        matches!(value, Value::Symbol(_)) || heap.identifier_of(value).is_some()
    });
    define(isolate, "bool?", |_, value| matches!(value, Value::Bool(_)));
    define(isolate, "list?", |heap, value| {
        value == Value::Nil || heap.pair_of(value).is_some()
    });
    define(isolate, "fn?", |heap, value| {
        heap.closure_of(value).is_some() || heap.native_of(value).is_some()
    });
}
