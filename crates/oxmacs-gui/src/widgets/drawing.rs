use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Color, Font, Pixels, Point, Rectangle, Renderer, Size, Theme, mouse};
use oxmacs_core::theme::Theme as Faces;
use oxmacs_script::shapes::{Ink, Paint, Shape};

use crate::style;

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
    }
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
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use iced::Color;
    use oxmacs_core::theme::{ADWAITA, Highlight};
    use oxmacs_script::shapes::{Ink, Paint, Shape};

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
