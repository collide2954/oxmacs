use ox::Error;
use ox::heap::Heap;
use ox::value::{Value, print};
use oxmacs_core::theme::Highlight;

use crate::views::face_named;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ink {
    Foreground,
    Background,
    Face(Highlight),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    pub ink: Ink,
    pub alpha: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        paint: Paint,
    },
    Circle {
        x: f32,
        y: f32,
        radius: f32,
        paint: Paint,
    },
    Line {
        from: (f32, f32),
        to: (f32, f32),
        width: f32,
        paint: Paint,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        size: Option<f32>,
        paint: Paint,
    },
}

const SHAPES: &str = "[:rect x y width height color], [:circle x y radius color], [:line x1 y1 x2 y2 color width] or [:text x y text color size]";

fn wrong(heap: &Heap, wanted: &str, value: Value) -> Error {
    Error::new(format!(
        "Drawings take {wanted}, got {}",
        print::write(heap, value)
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
        _ => return Err(wrong(heap, SHAPES, value)),
    })
}

pub fn shapes(heap: &Heap, value: Value) -> Result<Vec<Shape>, Error> {
    heap.elements(value)
        .ok_or_else(|| wrong(heap, "a vector of shapes", value))?
        .into_iter()
        .map(|item| shape(heap, item))
        .collect()
}
