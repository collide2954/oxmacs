use ox::Isolate;
use ox::build::make_vector;
use ox::error::Error;
use ox::heap::Heap;
use ox::value::Value;

use super::{name_of, number, point, point_value, wrong};
use crate::ease::Ease;

const MODULE: &str = "rodeo/ease";

fn ease(heap: &Heap, name: &str, value: Value) -> Result<Ease, Error> {
    name_of(heap, value)
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| wrong(heap, name, "an ease such as :quad-in or :bounce-out", value))
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
    isolate.define_native_in(MODULE, "apply", 2, Some(2), |heap, arguments| {
        let curve = ease(heap, "ease/apply", arguments[0])?;
        Ok(Value::Float(curve.apply(number(
            heap,
            "ease/apply",
            arguments[1],
        )?)))
    });
    isolate.define_native_in(MODULE, "between", 4, Some(4), between);
    isolate.define_native_in(MODULE, "names", 0, Some(0), |heap, _| {
        let names: Vec<Value> = Ease::all()
            .map(|curve| Value::keyword(curve.name()))
            .collect();
        make_vector(heap, &names)
    });
}
