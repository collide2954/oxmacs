use ox_lang::Isolate;
use ox_lang::build::make_vector;
use ox_lang::error::Error;
use ox_lang::heap::Heap;
use ox_lang::native::{expected, number};
use ox_lang::value::Value;

use super::{name_of, point, point_value};
use crate::ease::Ease;

const MODULE: &str = "rodeo/ease";

fn ease(heap: &Heap, name: &str, value: Value) -> Result<Ease, Error> {
    name_of(heap, value)
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| expected(heap, name, "an ease such as :quad-in or :bounce-out", value))
}

fn between(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let name = "ease/between";
    let curve = ease(heap, name, arguments[0])?;
    let t = number(heap, name, arguments[3])?;
    if heap.vector_of(arguments[1]).is_some() || heap.map_of(arguments[1]).is_some() {
        let from = point(heap, name, arguments[1])?;
        let to = point(heap, name, arguments[2])?;
        let eased = curve.apply(t);
        return point_value(heap, from.lerp(to, eased));
    }
    let from = number(heap, name, arguments[1])?;
    let to = number(heap, name, arguments[2])?;
    Ok(Value::Float(curve.between(from, to, t)))
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "apply")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let curve = ease(heap, "ease/apply", arguments[0])?;
            Ok(Value::Float(curve.apply(number(
                heap,
                "ease/apply",
                arguments[1],
            )?)))
        });
    isolate
        .native(MODULE, "between")
        .arity(4, Some(4))
        .function(between);
    isolate
        .native(MODULE, "names")
        .arity(0, Some(0))
        .function(|heap, _| {
            let names: Vec<Value> = Ease::all()
                .map(|curve| Value::keyword(curve.name()))
                .collect();
            make_vector(heap, &names)
        });
}
