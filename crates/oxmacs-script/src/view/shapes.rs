use ox_lang::Error;
use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};
use oxmacs_core::view::shapes::{Ink, Paint, Shape};

use oxmacs_core::theme::face_named;

const SHAPES: &str = "[:rect x y width height color], [:circle x y radius color], [:line x1 y1 x2 y2 color width], [:text x y text color size], [:polygon points color], [:curve points color width fade] or [:glow x y radius color]";

fn wrong(heap: &Heap, wanted: &str, value: Value) -> Error {
    Error::new(format!(
        "Drawings take {wanted}, got {}",
        print::brief(heap, value)
    ))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "drawings use small pixel coordinates"
)]
fn number(heap: &Heap, value: Value) -> Result<f32, Error> {
    match value {
        Value::Int(number) => Ok(number as f32),
        Value::Float(number) => Ok(number as f32),
        _ => Err(wrong(heap, "numbers for positions and sizes", value)),
    }
}

fn hex(text: &str) -> Option<(u8, u8, u8)> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.is_ascii() {
        return None;
    }
    let part = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    Some((part(0)?, part(2)?, part(4)?))
}

fn ink(heap: &Heap, value: Value) -> Result<Ink, Error> {
    let found = match value {
        Value::Keyword(name) => match name.name() {
            "foreground" => Some(Ink::Foreground),
            "background" => Some(Ink::Background),
            other => face_named(other).map(Ink::Face),
        },
        _ => heap
            .text(value)
            .and_then(hex)
            .map(|(red, green, blue)| Ink::Rgb(red, green, blue)),
    };
    found.ok_or_else(|| {
        wrong(
            heap,
            "a face such as :success, :foreground or a \"#rrggbb\" color",
            value,
        )
    })
}

pub fn paint(heap: &Heap, value: Value) -> Result<Paint, Error> {
    if heap.text(value).is_none()
        && let Some(parts) = heap.elements(value)
    {
        let [color, alpha] = parts.as_slice() else {
            return Err(wrong(heap, "a color or [color alpha]", value));
        };
        return Ok(Paint {
            ink: ink(heap, *color)?,
            alpha: number(heap, *alpha)?.clamp(0.0, 1.0),
        });
    }
    Ok(Paint {
        ink: ink(heap, value)?,
        alpha: 1.0,
    })
}

fn shape(heap: &Heap, value: Value) -> Result<Shape, Error> {
    let parts = heap
        .text(value)
        .is_none()
        .then(|| heap.elements(value))
        .flatten()
        .ok_or_else(|| wrong(heap, SHAPES, value))?;
    let kind = match parts.first() {
        Some(Value::Keyword(kind)) => kind.name(),
        _ => return Err(wrong(heap, SHAPES, value)),
    };
    let at = |index: usize| number(heap, parts[index]);
    Ok(match (kind, parts.len()) {
        ("rect", 6) => Shape::Rect {
            x: at(1)?,
            y: at(2)?,
            width: at(3)?,
            height: at(4)?,
            paint: paint(heap, parts[5])?,
        },
        ("circle", 5) => Shape::Circle {
            x: at(1)?,
            y: at(2)?,
            radius: at(3)?,
            paint: paint(heap, parts[4])?,
        },
        ("line", 6 | 7) => Shape::Line {
            from: (at(1)?, at(2)?),
            to: (at(3)?, at(4)?),
            width: if parts.len() == 7 { at(6)? } else { 1.0 },
            paint: paint(heap, parts[5])?,
        },
        ("text", 5 | 6) => Shape::Text {
            x: at(1)?,
            y: at(2)?,
            text: print::display(heap, parts[3]),
            size: if parts.len() == 6 { Some(at(5)?) } else { None },
            paint: paint(heap, parts[4])?,
        },
        ("polygon", 3) => Shape::Polygon {
            points: points(heap, parts[1])?,
            paint: paint(heap, parts[2])?,
        },
        ("curve", 4 | 5) => Shape::Curve {
            points: points(heap, parts[1])?,
            width: at(3)?,
            fade: if parts.len() == 5 {
                at(4)?.clamp(0.0, 1.0)
            } else {
                0.0
            },
            paint: paint(heap, parts[2])?,
        },
        ("glow", 5) => Shape::Glow {
            x: at(1)?,
            y: at(2)?,
            radius: at(3)?,
            paint: paint(heap, parts[4])?,
        },
        _ => return Err(wrong(heap, SHAPES, value)),
    })
}

fn points(heap: &Heap, value: Value) -> Result<Vec<(f32, f32)>, Error> {
    heap.elements(value)
        .ok_or_else(|| wrong(heap, "a vector of [x y] points", value))?
        .into_iter()
        .map(|point| match heap.elements(point).as_deref() {
            Some([x, y]) => Ok((number(heap, *x)?, number(heap, *y)?)),
            _ => Err(wrong(heap, "points as [x y]", point)),
        })
        .collect()
}

pub fn shapes(heap: &Heap, value: Value) -> Result<Vec<Shape>, Error> {
    heap.elements(value)
        .ok_or_else(|| wrong(heap, "a vector of shapes", value))?
        .into_iter()
        .map(|item| shape(heap, item))
        .collect()
}
