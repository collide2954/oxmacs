use crate::rect::Rect;
use crate::vec::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub game: Vec2,
    pub scale: f64,
    pub offset: Vec2,
    pub size: Vec2,
}

impl Viewport {
    pub fn fit(game: Vec2, window: Vec2, smooth: bool) -> Self {
        let fits = (window.x / game.x.max(1.0)).min(window.y / game.y.max(1.0));
        let scale = if smooth || fits < 1.0 {
            fits
        } else {
            fits.floor()
        };
        let size = game * scale;
        let centered = (window - size) / 2.0;
        let offset = if smooth {
            centered
        } else {
            Vec2::new(centered.x.floor(), centered.y.floor())
        };
        Self {
            game,
            scale,
            offset,
            size,
        }
    }

    pub fn to_game(&self, point: Vec2) -> Option<Vec2> {
        let inside = (point - self.offset) / self.scale;
        Rect::at(Vec2::ZERO, self.game)
            .contains(inside)
            .then_some(inside)
    }

    pub fn to_window(&self, point: Vec2) -> Vec2 {
        point * self.scale + self.offset
    }
}
