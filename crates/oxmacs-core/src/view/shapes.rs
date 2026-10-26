use crate::theme::Highlight;

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
