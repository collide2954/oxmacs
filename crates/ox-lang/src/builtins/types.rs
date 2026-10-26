use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::symbol::Sym;
use crate::value::{Kind, Value, print};
use crate::vm::build::full;

fn named(heap: &Heap, function: &str, value: Value) -> Result<String, Error> {
    match value {
        Value::Keyword(name) | Value::Symbol(name) => Ok(name.name().to_owned()),
        _ => heap
            .text(value)
            .map(str::to_owned)
            .or_else(|| {
                heap.identifier_of(value)
                    .map(|found| found.name.name().to_owned())
            })
            .ok_or_else(|| {
                Error::new(format!(
                    "{function} expects a string, symbol or keyword, got {}",
                    print::brief(heap, value)
                ))
                .of(ErrorKind::Type)
            }),
    }
}

fn type_of(heap: &Heap, value: Value) -> Value {
    let name = match value {
        Value::Nil => "nil",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Char(_) => "char",
        Value::Symbol(_) => "symbol",
        Value::Keyword(_) => "keyword",
        Value::Ref(reference) => match reference.kind {
            Kind::String => "string",
            Kind::Pair => "list",
            Kind::Native | Kind::Closure => "fn",
            Kind::BigInt => "int",
            Kind::Vector => "vector",
            Kind::Map => "map",
            Kind::Set => "set",
            Kind::Atom => "atom",
            Kind::Identifier => "symbol",
            Kind::Generator => "generator",
            Kind::Promise => "promise",
            Kind::Channel => "channel",
            Kind::Foreign => "foreign",
            Kind::RecordType => "record-type",
            Kind::Record => {
                return heap
                    .record_of(value)
                    .and_then(|record| heap.record_type_of(record.kind))
                    .map_or(Value::keyword("record"), |found| Value::Keyword(found.name));
            }
            Kind::Cell | Kind::VecNode | Kind::MapNode => "internal",
        },
    };
    Value::keyword(name)
}

fn define(isolate: &mut Isolate, name: &str, test: fn(&Heap, Value) -> bool) {
    isolate
        .native("ox", name)
        .arity(1, Some(1))
        .function(move |heap, arguments| Ok(Value::Bool(test(heap, arguments[0]))));
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "keyword")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Ok(Value::Keyword(Sym::intern(&named(
                heap,
                "keyword",
                arguments[0],
            )?)))
        });
    isolate
        .native("ox", "symbol")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Ok(Value::Symbol(Sym::intern(&named(
                heap,
                "symbol",
                arguments[0],
            )?)))
        });
    isolate
        .native("ox", "name")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let found = named(heap, "name", arguments[0])?;
            heap.string(&found).map_err(full)
        });
    isolate
        .native("ox", "type")
        .arity(1, Some(1))
        .function(|heap, arguments| Ok(type_of(heap, arguments[0])));
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
