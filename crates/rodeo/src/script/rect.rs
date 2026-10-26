use ox::Isolate;
use ox::value::Value;

use super::{number, point, point_value, rect, rect_value};
use crate::rect::Rect;
use crate::vec::Vec2;

const MODULE: &str = "rodeo/rect";

fn part(isolate: &mut Isolate, name: &'static str, read: fn(Rect) -> Vec2) {
    let shown = format!("rect/{name}");
    isolate.define_native_in(MODULE, name, 1, Some(1), move |heap, arguments| {
        let found = rect(heap, &shown, arguments[0])?;
        point_value(heap, read(found))
    });
}

fn edge(isolate: &mut Isolate, name: &'static str, read: fn(Rect) -> f64) {
    let shown = format!("rect/{name}");
    isolate.define_native_in(MODULE, name, 1, Some(1), move |heap, arguments| {
        Ok(Value::Float(read(rect(heap, &shown, arguments[0])?)))
    });
}

fn pair(isolate: &mut Isolate, name: &'static str, apply: fn(Rect, Rect) -> Rect) {
    let shown = format!("rect/{name}");
    isolate.define_native_in(MODULE, name, 2, Some(2), move |heap, arguments| {
        let a = rect(heap, &shown, arguments[0])?;
        let b = rect(heap, &shown, arguments[1])?;
        rect_value(heap, apply(a, b))
    });
}

pub fn install(isolate: &mut Isolate) {
    part(isolate, "center", Rect::center);
    part(isolate, "position", Rect::position);
    part(isolate, "size", Rect::size);
    edge(isolate, "left", Rect::left);
    edge(isolate, "top", Rect::top);
    edge(isolate, "right", Rect::right);
    edge(isolate, "bottom", Rect::bottom);
    pair(isolate, "union", Rect::union);
    isolate.define_native_in(MODULE, "centered", 2, Some(2), |heap, arguments| {
        let center = point(heap, "rect/centered", arguments[0])?;
        let size = point(heap, "rect/centered", arguments[1])?;
        rect_value(heap, Rect::centered(center, size))
    });
    isolate.define_native_in(MODULE, "overlaps?", 2, Some(2), |heap, arguments| {
        let a = rect(heap, "rect/overlaps?", arguments[0])?;
        let b = rect(heap, "rect/overlaps?", arguments[1])?;
        Ok(Value::Bool(a.overlaps(b)))
    });
    isolate.define_native_in(MODULE, "contains?", 2, Some(2), |heap, arguments| {
        let area = rect(heap, "rect/contains?", arguments[0])?;
        let spot = point(heap, "rect/contains?", arguments[1])?;
        Ok(Value::Bool(area.contains(spot)))
    });
    isolate.define_native_in(MODULE, "intersection", 2, Some(2), |heap, arguments| {
        let a = rect(heap, "rect/intersection", arguments[0])?;
        let b = rect(heap, "rect/intersection", arguments[1])?;
        match a.intersection(b) {
            Some(shared) => rect_value(heap, shared),
            None => Ok(Value::Nil),
        }
    });
    isolate.define_native_in(MODULE, "translate", 2, Some(2), |heap, arguments| {
        let area = rect(heap, "rect/translate", arguments[0])?;
        let by = point(heap, "rect/translate", arguments[1])?;
        rect_value(heap, area.translated(by))
    });
    isolate.define_native_in(MODULE, "expand", 2, Some(2), |heap, arguments| {
        let area = rect(heap, "rect/expand", arguments[0])?;
        let by = number(heap, "rect/expand", arguments[1])?;
        rect_value(heap, area.expanded(by))
    });
    isolate.define_native_in(MODULE, "clamp", 2, Some(2), |heap, arguments| {
        let area = rect(heap, "rect/clamp", arguments[0])?;
        let spot = point(heap, "rect/clamp", arguments[1])?;
        point_value(heap, area.clamp(spot))
    });
}
