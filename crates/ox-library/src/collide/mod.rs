mod grid;
mod natives;
mod sweep;
mod tiles;

pub use grid::Grid;
pub(crate) use natives::install;
pub use sweep::{Contact, Hit, Moved, slide, sweep};
pub use tiles::move_in_tiles;

use crate::geometry::Rect;
use crate::geometry::Vec2;

fn cell(value: f64, size: f64) -> i64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "positions beyond the i64 range saturate"
    )]
    let index = (value / size).floor() as i64;
    index
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Circle {
    pub center: Vec2,
    pub radius: f64,
}

impl Circle {
    pub const fn new(center: Vec2, radius: f64) -> Self {
        Self { center, radius }
    }

    pub fn contains(self, point: Vec2) -> bool {
        (point - self.center).length_squared() < self.radius * self.radius
    }

    pub fn overlaps(self, other: Self) -> bool {
        let reach = self.radius + other.radius;
        (other.center - self.center).length_squared() < reach * reach
    }

    pub fn overlaps_rect(self, rect: Rect) -> bool {
        self.contains(rect.clamp(self.center))
    }

    pub fn bounds(self) -> Rect {
        Rect::centered(self.center, Vec2::new(self.radius, self.radius) * 2.0)
    }
}

pub fn penetration(moving: Rect, solid: Rect) -> Option<Vec2> {
    if !moving.overlaps(solid) {
        return None;
    }
    let shallower = |back: f64, ahead: f64| {
        if back.abs() < ahead.abs() {
            back
        } else {
            ahead
        }
    };
    let x = shallower(solid.left() - moving.right(), solid.right() - moving.left());
    let y = shallower(solid.top() - moving.bottom(), solid.bottom() - moving.top());
    Some(if x.abs() < y.abs() {
        Vec2::new(x, 0.0)
    } else {
        Vec2::new(0.0, y)
    })
}
