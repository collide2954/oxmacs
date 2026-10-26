use ox_lang::Isolate;
use ox_lang::error::Error;
use ox_lang::heap::Heap;
use ox_lang::value::Value;

use super::{number, point, point_value};
use crate::vec::Vec2;

const MODULE: &str = "rodeo/vec";

fn two(isolate: &mut Isolate, name: &'static str, apply: fn(Vec2, Vec2) -> Vec2) {
    let shown = format!("vec/{name}");
    isolate
        .native(MODULE, name)
        .arity(2, Some(2))
        .function(move |heap, arguments| {
            let a = point(heap, &shown, arguments[0])?;
            let b = point(heap, &shown, arguments[1])?;
            point_value(heap, apply(a, b))
        });
}

fn measure(isolate: &mut Isolate, name: &'static str, apply: fn(Vec2, Vec2) -> f64) {
    let shown = format!("vec/{name}");
    isolate
        .native(MODULE, name)
        .arity(2, Some(2))
        .function(move |heap, arguments| {
            let a = point(heap, &shown, arguments[0])?;
            let b = point(heap, &shown, arguments[1])?;
            Ok(Value::Float(apply(a, b)))
        });
}

fn scaled(isolate: &mut Isolate, name: &'static str, apply: fn(Vec2, f64) -> Vec2) {
    let shown = format!("vec/{name}");
    isolate
        .native(MODULE, name)
        .arity(2, Some(2))
        .function(move |heap, arguments| {
            let vector = point(heap, &shown, arguments[0])?;
            let amount = number(heap, &shown, arguments[1])?;
            point_value(heap, apply(vector, amount))
        });
}

fn one(isolate: &mut Isolate, name: &'static str, apply: fn(Vec2) -> Vec2) {
    let shown = format!("vec/{name}");
    isolate
        .native(MODULE, name)
        .arity(1, Some(1))
        .function(move |heap, arguments| {
            let vector = point(heap, &shown, arguments[0])?;
            point_value(heap, apply(vector))
        });
}

fn plain(isolate: &mut Isolate, name: &'static str, apply: fn(f64) -> f64) {
    let shown = format!("vec/{name}");
    isolate
        .native(MODULE, name)
        .arity(1, Some(1))
        .function(move |heap, arguments| {
            Ok(Value::Float(apply(number(heap, &shown, arguments[0])?)))
        });
}

fn sum(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let mut total = Vec2::ZERO;
    for argument in arguments {
        total += point(heap, "vec/add", *argument)?;
    }
    point_value(heap, total)
}

pub fn install(isolate: &mut Isolate) {
    isolate.native(MODULE, "add").arity(1, None).function(sum);
    two(isolate, "sub", |a, b| a - b);
    scaled(isolate, "scale", |vector, factor| vector * factor);
    one(isolate, "normalize", Vec2::normalized);
    one(isolate, "perpendicular", Vec2::perpendicular);
    measure(isolate, "dot", Vec2::dot);
    measure(isolate, "cross", Vec2::cross);
    measure(isolate, "distance", Vec2::distance);
    isolate
        .native(MODULE, "length")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Ok(Value::Float(
                point(heap, "vec/length", arguments[0])?.length(),
            ))
        });
    isolate
        .native(MODULE, "angle")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Ok(Value::Float(
                point(heap, "vec/angle", arguments[0])?.angle(),
            ))
        });
    isolate
        .native(MODULE, "from-angle")
        .arity(1, Some(2))
        .function(|heap, arguments| {
            let angle = number(heap, "vec/from-angle", arguments[0])?;
            let length = match arguments.get(1) {
                Some(length) => number(heap, "vec/from-angle", *length)?,
                None => 1.0,
            };
            point_value(heap, Vec2::from_angle(angle) * length)
        });
    scaled(isolate, "rotate", Vec2::rotated);
    scaled(isolate, "clamp-length", Vec2::clamp_length);
    two(isolate, "reflect", Vec2::reflect);
    isolate
        .native(MODULE, "lerp")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let a = point(heap, "vec/lerp", arguments[0])?;
            let b = point(heap, "vec/lerp", arguments[1])?;
            let t = number(heap, "vec/lerp", arguments[2])?;
            point_value(heap, a.lerp(b, t))
        });
    isolate
        .native(MODULE, "move-toward")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let from = point(heap, "vec/move-toward", arguments[0])?;
            let to = point(heap, "vec/move-toward", arguments[1])?;
            let step = number(heap, "vec/move-toward", arguments[2])?;
            point_value(heap, from.move_toward(to, step))
        });
    plain(isolate, "degrees", f64::to_degrees);
    plain(isolate, "radians", f64::to_radians);
}
