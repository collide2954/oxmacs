use ox_lang::Isolate;
use ox_lang::error::Error;
use ox_lang::heap::Heap;
use ox_lang::native::number;
use ox_lang::value::Value;

use super::{Rect, Vec2, point, point_value, rect, rect_value};

const MODULE: &str = "ox/geometry";

fn two(isolate: &mut Isolate, name: &'static str, apply: fn(Vec2, Vec2) -> Vec2) {
    let shown = format!("geometry/{name}");
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
    let shown = format!("geometry/{name}");
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
    let shown = format!("geometry/{name}");
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
    let shown = format!("geometry/{name}");
    isolate
        .native(MODULE, name)
        .arity(1, Some(1))
        .function(move |heap, arguments| {
            let vector = point(heap, &shown, arguments[0])?;
            point_value(heap, apply(vector))
        });
}

fn part(isolate: &mut Isolate, name: &'static str, read: fn(Rect) -> Vec2) {
    let shown = format!("geometry/{name}");
    isolate
        .native(MODULE, name)
        .arity(1, Some(1))
        .function(move |heap, arguments| {
            let found = rect(heap, &shown, arguments[0])?;
            point_value(heap, read(found))
        });
}

fn edge(isolate: &mut Isolate, name: &'static str, read: fn(Rect) -> f64) {
    let shown = format!("geometry/{name}");
    isolate
        .native(MODULE, name)
        .arity(1, Some(1))
        .function(move |heap, arguments| Ok(Value::Float(read(rect(heap, &shown, arguments[0])?))));
}

fn pair(isolate: &mut Isolate, name: &'static str, apply: fn(Rect, Rect) -> Rect) {
    let shown = format!("geometry/{name}");
    isolate
        .native(MODULE, name)
        .arity(2, Some(2))
        .function(move |heap, arguments| {
            let a = rect(heap, &shown, arguments[0])?;
            let b = rect(heap, &shown, arguments[1])?;
            rect_value(heap, apply(a, b))
        });
}

fn install_rects(isolate: &mut Isolate) {
    part(isolate, "center", Rect::center);
    part(isolate, "position", Rect::position);
    part(isolate, "size", Rect::size);
    edge(isolate, "left", Rect::left);
    edge(isolate, "top", Rect::top);
    edge(isolate, "right", Rect::right);
    edge(isolate, "bottom", Rect::bottom);
    pair(isolate, "union", Rect::union);
    isolate
        .native(MODULE, "centered")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let center = point(heap, "geometry/centered", arguments[0])?;
            let size = point(heap, "geometry/centered", arguments[1])?;
            rect_value(heap, Rect::centered(center, size))
        });
    isolate
        .native(MODULE, "overlaps?")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let a = rect(heap, "geometry/overlaps?", arguments[0])?;
            let b = rect(heap, "geometry/overlaps?", arguments[1])?;
            Ok(Value::Bool(a.overlaps(b)))
        });
    isolate
        .native(MODULE, "contains?")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let area = rect(heap, "geometry/contains?", arguments[0])?;
            let spot = point(heap, "geometry/contains?", arguments[1])?;
            Ok(Value::Bool(area.contains(spot)))
        });
    isolate
        .native(MODULE, "intersection")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let a = rect(heap, "geometry/intersection", arguments[0])?;
            let b = rect(heap, "geometry/intersection", arguments[1])?;
            match a.intersection(b) {
                Some(shared) => rect_value(heap, shared),
                None => Ok(Value::Nil),
            }
        });
    isolate
        .native(MODULE, "translate")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let area = rect(heap, "geometry/translate", arguments[0])?;
            let by = point(heap, "geometry/translate", arguments[1])?;
            rect_value(heap, area.translated(by))
        });
    isolate
        .native(MODULE, "expand")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let area = rect(heap, "geometry/expand", arguments[0])?;
            let by = number(heap, "geometry/expand", arguments[1])?;
            rect_value(heap, area.expanded(by))
        });
    isolate
        .native(MODULE, "clamp")
        .arity(2, Some(2))
        .function(|heap, arguments| {
            let area = rect(heap, "geometry/clamp", arguments[0])?;
            let spot = point(heap, "geometry/clamp", arguments[1])?;
            point_value(heap, area.clamp(spot))
        });
}

fn sum(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let mut total = Vec2::ZERO;
    for argument in arguments {
        total += point(heap, "geometry/add", *argument)?;
    }
    point_value(heap, total)
}

pub(crate) fn install(isolate: &mut Isolate) {
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
                point(heap, "geometry/length", arguments[0])?.length(),
            ))
        });
    isolate
        .native(MODULE, "angle")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            Ok(Value::Float(
                point(heap, "geometry/angle", arguments[0])?.angle(),
            ))
        });
    isolate
        .native(MODULE, "from-angle")
        .arity(1, Some(2))
        .function(|heap, arguments| {
            let angle = number(heap, "geometry/from-angle", arguments[0])?;
            let length = match arguments.get(1) {
                Some(length) => number(heap, "geometry/from-angle", *length)?,
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
            let a = point(heap, "geometry/lerp", arguments[0])?;
            let b = point(heap, "geometry/lerp", arguments[1])?;
            let t = number(heap, "geometry/lerp", arguments[2])?;
            point_value(heap, a.lerp(b, t))
        });
    isolate
        .native(MODULE, "move-toward")
        .arity(3, Some(3))
        .function(|heap, arguments| {
            let from = point(heap, "geometry/move-toward", arguments[0])?;
            let to = point(heap, "geometry/move-toward", arguments[1])?;
            let step = number(heap, "geometry/move-toward", arguments[2])?;
            point_value(heap, from.move_toward(to, step))
        });
    install_rects(isolate);
}
