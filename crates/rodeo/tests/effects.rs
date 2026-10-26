use rodeo::color::Color;
use rodeo::draw::effects::{faded, outlined, shade};
use rodeo::draw::{Palette, Sprite};
use rodeo::rect::Rect;
use rodeo::texture::Textures;

fn plain() -> Sprite {
    let mut textures = Textures::new();
    let texture = textures.insert(1, 1, vec![0; 4]).unwrap();
    Sprite::new(texture, Rect::new(0.0, 0.0, 1.0, 1.0))
}

#[test]
fn tint_multiplies_and_flash_whitens() {
    let texel = Color::rgba8(200, 100, 50, 255);
    assert_eq!(shade(texel, &plain(), &[]), texel);
    let tinted = Sprite {
        tint: Color::new(0.5, 1.0, 1.0, 0.5),
        ..plain()
    };
    assert_eq!(shade(texel, &tinted, &[]).to_rgba8(), [100, 100, 50, 128]);
    let flashing = Sprite {
        flash: 1.0,
        ..plain()
    };
    assert_eq!(
        shade(Color::rgba8(0, 0, 0, 128), &flashing, &[]).to_rgba8(),
        [255, 255, 255, 128]
    );
}

#[test]
fn palettes_swap_exact_colors() {
    let palettes = [Palette {
        from: vec![Color::rgba8(255, 0, 0, 255), Color::rgba8(0, 0, 255, 255)],
        to: vec![Color::rgba8(0, 255, 0, 255), Color::rgba8(9, 9, 9, 255)],
    }];
    let swapped = Sprite {
        palette: Some(0),
        ..plain()
    };
    assert_eq!(
        shade(Color::rgba8(0, 0, 255, 255), &swapped, &palettes).to_rgba8(),
        [9, 9, 9, 255]
    );
    assert_eq!(
        shade(Color::rgba8(1, 2, 3, 255), &swapped, &palettes).to_rgba8(),
        [1, 2, 3, 255]
    );
}

#[test]
fn outlines_wrap_opaque_pixels() {
    let opaque = |x: i64, y: i64| if (x, y) == (2, 2) { 1.0 } else { 0.0 };
    assert!(outlined(opaque, 1, 2, 1));
    assert!(outlined(opaque, 2, 3, 1));
    assert!(!outlined(opaque, 1, 1, 1));
    assert!(outlined(opaque, 1, 1, 2));
    assert!(!outlined(opaque, 2, 2, 1));
    assert!(!outlined(opaque, 5, 2, 2));
}

#[test]
fn fades_cover_the_screen_by_their_alpha() {
    let pixel = Color::rgba8(200, 200, 200, 255);
    assert_eq!(
        faded(pixel, Color::new(0.0, 0.0, 0.0, 0.5)).to_rgba8(),
        [100, 100, 100, 255]
    );
    assert_eq!(faded(pixel, Color::TRANSPARENT), pixel);
}
