use ox_lang::Isolate;
use ox_lang::error::Error;
use ox_lang::heap::Heap;
use ox_lang::native::{expected, field, number, whole};
use ox_lang::value::Value;

use super::Camera;
use crate::geometry::Vec2;
use crate::geometry::{point, point_value, rect, rect_value};
use crate::map_value;
use crate::random::{bits, signed};

const MODULE: &str = "ox/camera";

fn camera(heap: &Heap, name: &str, value: Value) -> Result<Camera, Error> {
    if heap.map_of(value).is_none() {
        return Err(expected(heap, name, "a camera from camera/new", value));
    }
    let get = |key: &str| field(heap, value, key).filter(|found| *found != Value::Nil);
    let view = match get("view") {
        Some(found) => point(heap, name, found)?,
        None => Vec2::new(320.0, 180.0),
    };
    let mut made = Camera::new(view);
    if let Some(found) = get("position") {
        made.position = point(heap, name, found)?;
    }
    if let Some(found) = get("zoom") {
        made.zoom = number(heap, name, found)?;
    }
    if let Some(found) = get("bounds") {
        made.bounds = Some(rect(heap, name, found)?);
    }
    if let Some(found) = get("deadzone") {
        made.deadzone = point(heap, name, found)?;
    }
    if let Some(found) = get("trauma") {
        made.trauma = number(heap, name, found)?;
    }
    if let Some(found) = get("shake-strength") {
        made.shake_strength = number(heap, name, found)?;
    }
    if let Some(found) = get("shake-decay") {
        made.shake_decay = number(heap, name, found)?;
    }
    if let Some(found) = get("shake-seed") {
        made.shake_seed = bits(whole(heap, name, found)?);
    }
    if let Some(found) = get("shake-ticks") {
        made.shake_ticks = bits(whole(heap, name, found)?);
    }
    Ok(made)
}

fn stored(heap: &mut Heap, camera: &Camera) -> Result<Value, Error> {
    let position = point_value(heap, camera.position)?;
    let view = point_value(heap, camera.view)?;
    let deadzone = point_value(heap, camera.deadzone)?;
    let bounds = match camera.bounds {
        Some(bounds) => rect_value(heap, bounds)?,
        None => Value::Nil,
    };
    map_value(
        heap,
        &[
            ("position", position),
            ("view", view),
            ("zoom", Value::Float(camera.zoom)),
            ("bounds", bounds),
            ("deadzone", deadzone),
            ("trauma", Value::Float(camera.trauma)),
            ("shake-strength", Value::Float(camera.shake_strength)),
            ("shake-decay", Value::Float(camera.shake_decay)),
            ("shake-seed", Value::Int(signed(camera.shake_seed))),
            ("shake-ticks", Value::Int(signed(camera.shake_ticks))),
        ],
    )
}

fn changed(
    isolate: &mut Isolate,
    name: &'static str,
    count: usize,
    change: fn(&Heap, &str, &mut Camera, &[Value]) -> Result<(), Error>,
) {
    let shown = format!("camera/{name}");
    isolate
        .native(MODULE, name)
        .arity(count, Some(count))
        .function(move |heap, arguments| {
            let mut made = camera(heap, &shown, arguments[0])?;
            change(heap, &shown, &mut made, &arguments[1..])?;
            stored(heap, &made)
        });
}

fn placed(isolate: &mut Isolate, name: &'static str, place: fn(&Camera, Vec2) -> Vec2) {
    let shown = format!("camera/{name}");
    isolate
        .native(MODULE, name)
        .arity(2, Some(2))
        .function(move |heap, arguments| {
            let made = camera(heap, &shown, arguments[0])?;
            let spot = point(heap, &shown, arguments[1])?;
            point_value(heap, place(&made, spot))
        });
}

pub(crate) fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "new")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let view = point(heap, "camera/new", arguments[0])?;
            stored(heap, &Camera::new(view))
        });
    changed(isolate, "follow", 3, |heap, name, made, rest| {
        let target = point(heap, name, rest[0])?;
        let amount = number(heap, name, rest[1])?;
        made.follow(target, amount);
        Ok(())
    });
    changed(isolate, "clamp", 1, |_, _, made, _| {
        made.clamp_to_bounds();
        Ok(())
    });
    changed(isolate, "shake", 2, |heap, name, made, rest| {
        made.shake(number(heap, name, rest[0])?);
        Ok(())
    });
    changed(isolate, "tick", 2, |heap, name, made, rest| {
        made.tick(number(heap, name, rest[0])?);
        Ok(())
    });
    placed(isolate, "to-screen", Camera::world_to_screen);
    placed(isolate, "to-world", Camera::screen_to_world);
    isolate
        .native(MODULE, "view")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let made = camera(heap, "camera/view", arguments[0])?;
            rect_value(heap, made.view_rect())
        });
    isolate
        .native(MODULE, "offset")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let made = camera(heap, "camera/offset", arguments[0])?;
            point_value(heap, made.shake_offset())
        });
}
