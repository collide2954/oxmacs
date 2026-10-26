use super::Sprite;
use crate::color::Color;
use crate::vec::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Instance {
    pub corners: [[f32; 2]; 4],
    pub uvs: [[f32; 2]; 4],
    pub tint: [f32; 4],
    pub flash: f32,
    pub outline: [f32; 4],
    pub outline_width: f32,
    pub palette: i32,
}

pub(crate) fn narrow(value: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "screen coordinates fit in an f32"
    )]
    let narrowed = value as f32;
    narrowed
}

fn channels(color: Color) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}

impl Sprite {
    pub fn corners(&self) -> [Vec2; 4] {
        let (width, height) = (self.region.w, self.region.h);
        [
            Vec2::ZERO,
            Vec2::new(width, 0.0),
            Vec2::new(width, height),
            Vec2::new(0.0, height),
        ]
        .map(|corner| {
            let local = corner - self.origin;
            let scaled = Vec2::new(local.x * self.scale.x, local.y * self.scale.y);
            scaled.rotated(self.rotation) + self.position
        })
    }

    pub fn uvs(&self, texture_width: u32, texture_height: u32) -> [[f32; 2]; 4] {
        let across = f64::from(texture_width.max(1));
        let down = f64::from(texture_height.max(1));
        let (mut left, mut right) = (self.region.left() / across, self.region.right() / across);
        let (mut top, mut bottom) = (self.region.top() / down, self.region.bottom() / down);
        if self.flip_x {
            std::mem::swap(&mut left, &mut right);
        }
        if self.flip_y {
            std::mem::swap(&mut top, &mut bottom);
        }
        let [left, right, top, bottom] = [left, right, top, bottom].map(narrow);
        [[left, top], [right, top], [right, bottom], [left, bottom]]
    }

    pub fn instance(&self, texture_width: u32, texture_height: u32) -> Instance {
        let outline = self.outline.unwrap_or(super::Outline {
            color: Color::TRANSPARENT,
            width: 0.0,
        });
        Instance {
            corners: self
                .corners()
                .map(|corner| [narrow(corner.x), narrow(corner.y)]),
            uvs: self.uvs(texture_width, texture_height),
            tint: channels(self.tint),
            flash: self.flash,
            outline: channels(outline.color),
            outline_width: outline.width,
            palette: self
                .palette
                .and_then(|palette| i32::try_from(palette).ok())
                .unwrap_or(-1),
        }
    }
}
