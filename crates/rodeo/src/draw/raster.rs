use super::effects::{faded, outlined, shade};
use super::{Command, DrawList, Shape, Sprite};
use crate::texture::Textures;
use ox_library::color::Color;
use ox_library::geometry::Rect;
use ox_library::geometry::Vec2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

struct Canvas {
    width: i64,
    height: i64,
    colors: Vec<Color>,
}

fn whole(value: f64) -> i64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "pixel positions are clamped to the frame"
    )]
    let rounded = value.clamp(-1e9, 1e9) as i64;
    rounded
}

fn real(value: i64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "pixel positions are far below 2^52"
    )]
    let real = value as f64;
    real
}

fn center(x: i64, y: i64) -> Vec2 {
    Vec2::new(real(x) + 0.5, real(y) + 0.5)
}

impl Canvas {
    fn new(width: u32, height: u32, background: Color) -> Self {
        let count = usize::try_from(u64::from(width) * u64::from(height)).unwrap_or(0);
        Self {
            width: i64::from(width),
            height: i64::from(height),
            colors: vec![
                Color {
                    a: 1.0,
                    ..background
                };
                count
            ],
        }
    }

    fn blend(&mut self, x: i64, y: i64, color: Color) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height || color.a <= 0.0 {
            return;
        }
        let Ok(index) = usize::try_from(y * self.width + x) else {
            return;
        };
        let under = self.colors[index];
        self.colors[index] = Color {
            a: 1.0,
            ..under.lerp(color, color.a.min(1.0))
        };
    }

    fn area(
        &self,
        left: f64,
        top: f64,
        right: f64,
        bottom: f64,
    ) -> impl Iterator<Item = (i64, i64)> + use<> {
        let (x0, x1) = (
            whole(left.floor()).max(0),
            whole(right.ceil()).min(self.width),
        );
        let (y0, y1) = (
            whole(top.floor()).max(0),
            whole(bottom.ceil()).min(self.height),
        );
        (y0..y1).flat_map(move |y| (x0..x1).map(move |x| (x, y)))
    }

    fn fill(&mut self, bounds: Rect, color: Color, inside: impl Fn(Vec2) -> bool) {
        let pixels: Vec<(i64, i64)> = self
            .area(bounds.left(), bounds.top(), bounds.right(), bounds.bottom())
            .filter(|(x, y)| inside(center(*x, *y)))
            .collect();
        for (x, y) in pixels {
            self.blend(x, y, color);
        }
    }

    fn frame(self) -> Frame {
        Frame {
            width: u32::try_from(self.width).unwrap_or(0),
            height: u32::try_from(self.height).unwrap_or(0),
            pixels: self
                .colors
                .iter()
                .flat_map(|color| color.to_rgba8())
                .collect(),
        }
    }
}

fn texel(textures: &Textures, sprite: &Sprite, x: i64, y: i64) -> Color {
    let Some(texture) = textures.get(sprite.texture) else {
        return Color::TRANSPARENT;
    };
    let (column, row) = (whole(sprite.region.x) + x, whole(sprite.region.y) + y);
    if column < 0
        || row < 0
        || column >= i64::from(texture.width)
        || row >= i64::from(texture.height)
    {
        return Color::TRANSPARENT;
    }
    let Ok(at) = usize::try_from((row * i64::from(texture.width) + column) * 4) else {
        return Color::TRANSPARENT;
    };
    match texture.pixels.get(at..at + 4) {
        Some(&[red, green, blue, alpha]) => Color::rgba8(red, green, blue, alpha),
        _ => Color::TRANSPARENT,
    }
}

fn draw_sprite(canvas: &mut Canvas, list: &DrawList, textures: &Textures, sprite: &Sprite) {
    if sprite.scale.x == 0.0 || sprite.scale.y == 0.0 {
        return;
    }
    let (width, height) = (whole(sprite.region.w), whole(sprite.region.h));
    let reach = sprite
        .outline
        .map_or(0.0, |outline| outline.width.max(0.0).ceil());
    let pad = f64::from(reach) * sprite.scale.x.abs().max(sprite.scale.y.abs()) + 1.0;
    let corners = sprite.corners();
    let edge = |pick: fn(&Vec2) -> f64, keep: fn(f64, f64) -> f64, start: f64| {
        corners.iter().map(pick).fold(start, keep)
    };
    let left = edge(|corner| corner.x, f64::min, f64::INFINITY) - pad;
    let right = edge(|corner| corner.x, f64::max, f64::NEG_INFINITY) + pad;
    let top = edge(|corner| corner.y, f64::min, f64::INFINITY) - pad;
    let bottom = edge(|corner| corner.y, f64::max, f64::NEG_INFINITY) + pad;
    let alpha = |x: i64, y: i64| {
        if x < 0 || y < 0 || x >= width || y >= height {
            0.0
        } else {
            texel(textures, sprite, x, y).a
        }
    };
    let pixels: Vec<(i64, i64)> = canvas.area(left, top, right, bottom).collect();
    for (x, y) in pixels {
        let turned = (center(x, y) - sprite.position).rotated(-sprite.rotation);
        let local = Vec2::new(turned.x / sprite.scale.x, turned.y / sprite.scale.y) + sprite.origin;
        let (mut column, mut row) = (whole(local.x.floor()), whole(local.y.floor()));
        if sprite.flip_x {
            column = width - 1 - column;
        }
        if sprite.flip_y {
            row = height - 1 - row;
        }
        let base = if alpha(column, row) > 0.0 {
            texel(textures, sprite, column, row)
        } else {
            Color::TRANSPARENT
        };
        if base.a > 0.0 {
            canvas.blend(x, y, shade(base, sprite, list.palettes()));
        } else if let Some(outline) = sprite.outline {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "outline widths are a few pixels"
            )]
            let thickness = reach as u32;
            if outlined(alpha, column, row, thickness) {
                canvas.blend(x, y, outline.color);
            }
        }
    }
}

fn segment_distance(point: Vec2, from: Vec2, to: Vec2) -> f64 {
    let along = to - from;
    let length = along.length_squared();
    let t = if length == 0.0 {
        0.0
    } else {
        ((point - from).dot(along) / length).clamp(0.0, 1.0)
    };
    point.distance(from + along * t)
}

fn line(canvas: &mut Canvas, from: Vec2, to: Vec2, width: f64, color: Color) {
    let half = width.max(1.0) / 2.0;
    let bounds = Rect::new(
        from.x.min(to.x),
        from.y.min(to.y),
        (to.x - from.x).abs(),
        (to.y - from.y).abs(),
    )
    .expanded(half + 1.0);
    canvas.fill(bounds, color, |point| {
        segment_distance(point, from, to) <= half
    });
}

fn inside_polygon(point: Vec2, points: &[Vec2]) -> bool {
    let mut inside = false;
    let mut previous = points.len().saturating_sub(1);
    for (index, current) in points.iter().enumerate() {
        let before = points[previous];
        if (current.y > point.y) != (before.y > point.y)
            && point.x
                < (before.x - current.x) * (point.y - current.y) / (before.y - current.y)
                    + current.x
        {
            inside = !inside;
        }
        previous = index;
    }
    inside
}

fn draw_shape(canvas: &mut Canvas, shape: &Shape, color: Color) {
    match shape {
        Shape::Rect { rect, stroke: None } => {
            canvas.fill(*rect, color, |point| rect.contains(point));
        }
        Shape::Rect {
            rect,
            stroke: Some(width),
        } => {
            let inner = rect.expanded(-width);
            canvas.fill(*rect, color, |point| {
                rect.contains(point) && !inner.contains(point)
            });
        }
        Shape::Circle {
            center,
            radius,
            stroke,
        } => {
            let reach = radius + stroke.unwrap_or(0.0);
            let bounds = Rect::centered(*center, Vec2::new(reach, reach) * 2.0).expanded(1.0);
            canvas.fill(bounds, color, |point| {
                let distance = point.distance(*center);
                match stroke {
                    None => distance < *radius,
                    Some(width) => (distance - radius).abs() <= width / 2.0,
                }
            });
        }
        Shape::Line { from, to, width } => line(canvas, *from, *to, *width, color),
        Shape::Polygon {
            points,
            stroke: None,
        } => {
            let Some(first) = points.first() else {
                return;
            };
            let bounds = points
                .iter()
                .fold(Rect::at(*first, Vec2::ZERO), |bounds, point| {
                    bounds.union(Rect::at(*point, Vec2::ZERO))
                });
            canvas.fill(bounds, color, |point| inside_polygon(point, points));
        }
        Shape::Polygon {
            points,
            stroke: Some(width),
        } => {
            for (index, from) in points.iter().enumerate() {
                let to = points[(index + 1) % points.len()];
                line(canvas, *from, to, *width, color);
            }
        }
    }
}

pub fn render(list: &mut DrawList, textures: &Textures, width: u32, height: u32) -> Frame {
    let batches = list.finish();
    let mut canvas = Canvas::new(width, height, list.background);
    for batch in batches {
        for command in &list.commands()[batch.range] {
            match command {
                Command::Sprite(sprite) => draw_sprite(&mut canvas, list, textures, sprite),
                Command::Shape { shape, color, .. } => draw_shape(&mut canvas, shape, *color),
                Command::Text { .. } => {}
            }
        }
    }
    if let Some(fade) = list.fade {
        for color in &mut canvas.colors {
            *color = faded(*color, fade);
        }
    }
    canvas.frame()
}
