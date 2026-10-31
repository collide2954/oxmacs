use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use iced::advanced::image::{Handle, Image};
use iced::widget::canvas::{self, Frame, Geometry, LineCap, LineJoin, Path, Stroke, Text};
use iced::{Color, Font, Pixels, Point, Rectangle, Renderer, Size, Theme, mouse};
use oxmacs_core::theme::Theme as Faces;
use oxmacs_core::view::shapes::{Ink, Paint, Shape};

use crate::{style, units};

#[derive(Debug, Clone, Copy)]
pub struct Drawing<'a> {
    pub shapes: &'a [Shape],
    pub relative: bool,
    pub background: Option<Paint>,
    pub faces: Faces,
    pub font: Font,
    pub size: f32,
}

pub fn color(paint: Paint, faces: &Faces) -> Color {
    let base = match paint.ink {
        Ink::Foreground => style::color(faces.default.foreground),
        Ink::Background => style::color(faces.default.background),
        Ink::Face(face) => style::color(faces.highlight(face)),
        Ink::Rgb(red, green, blue) => Color::from_rgb8(red, green, blue),
    };
    Color {
        a: paint.alpha.clamp(0.0, 1.0),
        ..base
    }
}

pub fn placed(shape: &Shape, width: f32) -> Shape {
    match shape.clone() {
        Shape::Rect {
            x,
            y,
            width: across,
            height,
            paint,
        } => Shape::Rect {
            x: x * width,
            y,
            width: across * width,
            height,
            paint,
        },
        Shape::Circle {
            x,
            y,
            radius,
            paint,
        } => Shape::Circle {
            x: x * width,
            y,
            radius,
            paint,
        },
        Shape::Line {
            from,
            to,
            width: thickness,
            paint,
        } => Shape::Line {
            from: (from.0 * width, from.1),
            to: (to.0 * width, to.1),
            width: thickness,
            paint,
        },
        Shape::Text {
            x,
            y,
            text,
            size,
            paint,
        } => Shape::Text {
            x: x * width,
            y,
            text,
            size,
            paint,
        },
        Shape::Polygon { points, paint } => Shape::Polygon {
            points: across(points, width),
            paint,
        },
        Shape::Curve {
            points,
            width: thickness,
            fade,
            paint,
        } => Shape::Curve {
            points: across(points, width),
            width: thickness,
            fade,
            paint,
        },
        Shape::Glow {
            x,
            y,
            radius,
            paint,
        } => Shape::Glow {
            x: x * width,
            y,
            radius,
            paint,
        },
    }
}

fn across(points: Vec<(f32, f32)>, width: f32) -> Vec<(f32, f32)> {
    points.into_iter().map(|(x, y)| (x * width, y)).collect()
}

fn traced(points: &[(f32, f32)]) -> Path {
    Path::new(|builder| {
        let mut points = points.iter();
        if let Some((x, y)) = points.next() {
            builder.move_to(Point::new(*x, *y));
        }
        for (x, y) in points {
            builder.line_to(Point::new(*x, *y));
        }
    })
}

const FADE_STEPS: usize = 24;

fn curve(frame: &mut Frame, points: &[(f32, f32)], width: f32, fade: f32, color: Color) {
    let stroke = |color: Color| {
        Stroke::default()
            .with_width(width)
            .with_color(color)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round)
    };
    if fade <= 0.0 || points.len() < 3 {
        frame.stroke(&traced(points), stroke(color));
        return;
    }
    let chunk = points.len().div_ceil(FADE_STEPS).max(1);
    let last = points.len() - 1;
    let mut start = 0;
    while start < last {
        let end = (start + chunk).min(last);
        let along = units::to_f32(end) / units::to_f32(last);
        let alpha = color.a * (1.0 - fade * (1.0 - along));
        frame.stroke(
            &traced(&points[start..=end]),
            stroke(Color { a: alpha, ..color }),
        );
        start = end;
    }
}

const GLOW_SIZE: usize = 64;
const GLOW_PIXELS: u32 = 64;
const GLOW_COLORS: usize = 256;

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to a byte's range first"
)]
fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn glow_image(color: Color) -> Handle {
    static GLOWS: OnceLock<Mutex<HashMap<[u8; 3], Handle>>> = OnceLock::new();
    let [red, green, blue, _] = color.into_rgba8();
    let key = [red, green, blue];
    let mut glows = GLOWS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if glows.len() >= GLOW_COLORS && !glows.contains_key(&key) {
        glows.clear();
    }
    glows
        .entry(key)
        .or_insert_with(|| {
            let middle = units::to_f32(GLOW_SIZE) / 2.0;
            let pixels: Vec<u8> = (0..GLOW_SIZE * GLOW_SIZE)
                .flat_map(|index| {
                    let x = (units::to_f32(index % GLOW_SIZE) + 0.5 - middle) / middle;
                    let y = (units::to_f32(index / GLOW_SIZE) + 0.5 - middle) / middle;
                    let falloff = (-(x * x + y * y) * 4.5).exp() * (1.0 - (x * x + y * y)).max(0.0);
                    [red, green, blue, byte(falloff)]
                })
                .collect();
            Handle::from_rgba(GLOW_PIXELS, GLOW_PIXELS, pixels)
        })
        .clone()
}

impl<Message> canvas::Program<Message> for Drawing<'_> {
    type State = ();

    fn draw(
        &self,
        (): &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        if let Some(background) = self.background {
            frame.fill_rectangle(Point::ORIGIN, bounds.size(), color(background, &self.faces));
        }
        for shape in self.shapes {
            let scaled = self.relative.then(|| placed(shape, bounds.width));
            match scaled.as_ref().unwrap_or(shape) {
                Shape::Rect {
                    x,
                    y,
                    width,
                    height,
                    paint,
                } => frame.fill_rectangle(
                    Point::new(*x, *y),
                    Size::new(*width, *height),
                    color(*paint, &self.faces),
                ),
                Shape::Circle {
                    x,
                    y,
                    radius,
                    paint,
                } => frame.fill(
                    &Path::circle(Point::new(*x, *y), *radius),
                    color(*paint, &self.faces),
                ),
                Shape::Line {
                    from,
                    to,
                    width,
                    paint,
                } => frame.stroke(
                    &Path::line(Point::new(from.0, from.1), Point::new(to.0, to.1)),
                    Stroke::default()
                        .with_width(*width)
                        .with_color(color(*paint, &self.faces)),
                ),
                Shape::Text {
                    x,
                    y,
                    text,
                    size,
                    paint,
                } => frame.fill_text(Text {
                    content: text.clone(),
                    position: Point::new(*x, *y),
                    color: color(*paint, &self.faces),
                    size: Pixels(size.unwrap_or(self.size)),
                    font: self.font,
                    ..Text::default()
                }),
                Shape::Polygon { points, paint } => {
                    frame.fill(&traced(points), color(*paint, &self.faces));
                }
                Shape::Curve {
                    points,
                    width,
                    fade,
                    paint,
                } => curve(
                    &mut frame,
                    points,
                    *width,
                    *fade,
                    color(*paint, &self.faces),
                ),
                Shape::Glow {
                    x,
                    y,
                    radius,
                    paint,
                } => {
                    let shade = color(*paint, &self.faces);
                    frame.draw_image(
                        Rectangle::new(
                            Point::new(x - radius, y - radius),
                            Size::new(radius * 2.0, radius * 2.0),
                        ),
                        Image::new(glow_image(shade)).opacity(shade.a),
                    );
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use iced::Color;
    use oxmacs_core::theme::{ADWAITA, Highlight};
    use oxmacs_core::view::shapes::{Ink, Paint, Shape};

    use super::color;
    use crate::style;

    #[test]
    fn relative_shapes_take_their_across_positions_from_the_width() {
        let paint = Paint {
            ink: Ink::Foreground,
            alpha: 1.0,
        };
        let rect = Shape::Rect {
            x: 0.5,
            y: 3.0,
            width: 0.25,
            height: 10.0,
            paint,
        };
        let line = Shape::Line {
            from: (0.0, 1.0),
            to: (1.0, 2.0),
            width: 1.0,
            paint,
        };

        assert_eq!(
            super::placed(&rect, 200.0),
            Shape::Rect {
                x: 100.0,
                y: 3.0,
                width: 50.0,
                height: 10.0,
                paint
            }
        );
        assert_eq!(
            super::placed(&line, 200.0),
            Shape::Line {
                from: (0.0, 1.0),
                to: (200.0, 2.0),
                width: 1.0,
                paint
            }
        );
    }

    #[test]
    fn paints_take_their_color_from_the_theme_or_hex_and_keep_their_alpha() {
        let faces = ADWAITA;
        let paint = |ink, alpha| Paint { ink, alpha };

        assert_eq!(
            color(paint(Ink::Foreground, 1.0), &faces),
            style::color(faces.default.foreground)
        );
        assert_eq!(
            color(paint(Ink::Background, 1.0), &faces),
            style::color(faces.default.background)
        );
        assert_eq!(
            color(paint(Ink::Face(Highlight::Link), 0.5), &faces),
            Color {
                a: 0.5,
                ..style::color(faces.highlight(Highlight::Link))
            }
        );
        assert_eq!(
            color(paint(Ink::Rgb(255, 0, 0), 2.0), &faces),
            Color::from_rgb8(255, 0, 0)
        );
    }
}
