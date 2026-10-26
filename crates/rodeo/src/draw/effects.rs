use super::{Palette, Sprite};
use crate::color::Color;

fn keep_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

pub fn shade(texel: Color, sprite: &Sprite, palettes: &[Palette]) -> Color {
    let mut color = texel;
    if let Some(palette) = sprite.palette.and_then(|index| palettes.get(index)) {
        let bytes = texel.to_rgba8();
        let swapped = palette
            .from
            .iter()
            .position(|from| from.to_rgba8() == bytes)
            .and_then(|at| palette.to.get(at));
        if let Some(to) = swapped {
            color = *to;
        }
    }
    color = color * sprite.tint;
    if sprite.flash > 0.0 {
        color = keep_alpha(color.lerp(Color::WHITE, sprite.flash.min(1.0)), color.a);
    }
    color
}

pub fn outlined(alpha: impl Fn(i64, i64) -> f32, x: i64, y: i64, width: u32) -> bool {
    if alpha(x, y) > 0.0 {
        return false;
    }
    let reach = i64::from(width);
    (-reach..=reach).any(|down| {
        let across = reach - down.abs();
        (-across..=across).any(|side| (side, down) != (0, 0) && alpha(x + side, y + down) > 0.0)
    })
}

pub fn faded(pixel: Color, fade: Color) -> Color {
    keep_alpha(pixel.lerp(keep_alpha(fade, pixel.a), fade.a), pixel.a)
}
