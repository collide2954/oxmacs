use std::f64::consts::FRAC_PI_2;

use ox_library::color::Color;
use ox_library::geometry::Rect;
use ox_library::geometry::Vec2;
use rodeo::draw::{Outline, Sprite};
use rodeo::texture::Textures;

fn sprite() -> Sprite {
    let mut textures = Textures::new();
    let sheet = textures.insert(64, 32, vec![0; 64 * 32 * 4]).unwrap();
    Sprite {
        position: Vec2::new(100.0, 50.0),
        origin: Vec2::new(8.0, 4.0),
        scale: Vec2::new(2.0, 2.0),
        ..Sprite::new(sheet, Rect::new(16.0, 0.0, 16.0, 8.0))
    }
}

#[test]
fn corners_scale_around_the_origin() {
    assert_eq!(
        sprite().corners(),
        [
            Vec2::new(84.0, 42.0),
            Vec2::new(116.0, 42.0),
            Vec2::new(116.0, 58.0),
            Vec2::new(84.0, 58.0)
        ]
    );
}

#[test]
fn corners_turn_clockwise_with_rotation() {
    let turned = Sprite {
        rotation: FRAC_PI_2,
        ..sprite()
    };
    let corners = turned.corners();
    assert!((corners[0] - Vec2::new(108.0, 34.0)).length() < 1e-9);
    assert!((corners[2] - Vec2::new(92.0, 66.0)).length() < 1e-9);
}

#[test]
fn uvs_follow_the_region_and_flips() {
    assert_eq!(
        sprite().uvs(64, 32),
        [[0.25, 0.0], [0.5, 0.0], [0.5, 0.25], [0.25, 0.25]]
    );
    let flipped = Sprite {
        flip_x: true,
        flip_y: true,
        ..sprite()
    };
    assert_eq!(
        flipped.uvs(64, 32),
        [[0.5, 0.25], [0.25, 0.25], [0.25, 0.0], [0.5, 0.0]]
    );
}

#[test]
fn instances_pack_everything_a_shader_needs() {
    let glowing = Sprite {
        tint: Color::new(1.0, 0.5, 0.5, 1.0),
        flash: 0.25,
        outline: Some(Outline {
            color: Color::BLACK,
            width: 1.0,
        }),
        palette: Some(2),
        ..sprite()
    };
    let instance = glowing.instance(64, 32);
    assert_eq!(instance.corners[1], [116.0, 42.0]);
    assert_eq!(instance.uvs[2], [0.5, 0.25]);
    assert_eq!(instance.tint, [1.0, 0.5, 0.5, 1.0]);
    assert_eq!(instance.flash, 0.25);
    assert_eq!(instance.outline, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(instance.outline_width, 1.0);
    assert_eq!(instance.palette, 2);
    assert_eq!(sprite().instance(64, 32).palette, -1);
    assert_eq!(sprite().instance(64, 32).outline_width, 0.0);
}
