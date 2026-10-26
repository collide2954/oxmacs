use rodeo::color::Color;
use rodeo::draw::raster::{Frame, render};
use rodeo::draw::{DrawList, Outline, Shape, Sprite, Text};
use rodeo::rect::Rect;
use rodeo::texture::{TextureId, Textures};
use rodeo::vec::Vec2;

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

fn checker(textures: &mut Textures) -> TextureId {
    let pixels = [RED, GREEN, BLUE, WHITE].concat();
    textures.insert(2, 2, pixels).unwrap()
}

fn at(frame: &Frame, x: u32, y: u32) -> [u8; 4] {
    let index = usize::try_from((y * frame.width + x) * 4).unwrap();
    frame.pixels[index..index + 4].try_into().unwrap()
}

fn list() -> DrawList {
    let mut list = DrawList::new();
    list.background = Color::BLACK;
    list
}

#[test]
fn empty_frames_show_the_background() {
    let textures = Textures::new();
    let mut list = list();
    list.background = Color::rgba8(10, 20, 30, 255);
    list.text(Text::new("ignored", Vec2::ZERO), 0);
    let frame = render(&mut list, &textures, 3, 2);
    assert_eq!([frame.width, frame.height], [3, 2]);
    assert_eq!(frame.pixels.len(), 24);
    assert!(
        frame
            .pixels
            .chunks(4)
            .all(|pixel| pixel == [10, 20, 30, 255])
    );
}

#[test]
fn sprites_copy_their_region_where_they_stand() {
    let mut textures = Textures::new();
    let texture = checker(&mut textures);
    let mut list = list();
    list.sprite(Sprite {
        position: Vec2::new(1.0, 1.0),
        ..Sprite::new(texture, Rect::new(0.0, 0.0, 2.0, 2.0))
    });
    let frame = render(&mut list, &textures, 4, 4);
    assert_eq!(
        [
            at(&frame, 1, 1),
            at(&frame, 2, 1),
            at(&frame, 1, 2),
            at(&frame, 2, 2)
        ],
        [RED, GREEN, BLUE, WHITE]
    );
    assert_eq!(at(&frame, 0, 0), BLACK);
    assert_eq!(at(&frame, 3, 3), BLACK);
}

#[test]
fn sprites_flip_scale_and_blend() {
    let mut textures = Textures::new();
    let texture = checker(&mut textures);
    let mut list = list();
    list.sprite(Sprite {
        flip_x: true,
        ..Sprite::new(texture, Rect::new(0.0, 0.0, 2.0, 1.0))
    });
    list.sprite(Sprite {
        position: Vec2::new(0.0, 2.0),
        scale: Vec2::new(2.0, 2.0),
        ..Sprite::new(texture, Rect::new(0.0, 0.0, 1.0, 1.0))
    });
    list.sprite(Sprite {
        position: Vec2::new(3.0, 0.0),
        tint: Color::new(1.0, 1.0, 1.0, 0.5),
        ..Sprite::new(texture, Rect::new(1.0, 1.0, 1.0, 1.0))
    });
    let frame = render(&mut list, &textures, 4, 4);
    assert_eq!([at(&frame, 0, 0), at(&frame, 1, 0)], [GREEN, RED]);
    assert_eq!(
        [
            at(&frame, 0, 2),
            at(&frame, 1, 2),
            at(&frame, 0, 3),
            at(&frame, 1, 3)
        ],
        [RED; 4]
    );
    assert_eq!(at(&frame, 3, 0), [128, 128, 128, 255]);
}

#[test]
fn effects_flash_and_outline() {
    let mut textures = Textures::new();
    let texture = checker(&mut textures);
    let mut list = list();
    list.sprite(Sprite {
        position: Vec2::new(1.0, 1.0),
        flash: 1.0,
        outline: Some(Outline {
            color: Color::rgba8(255, 255, 0, 255),
            width: 1.0,
        }),
        ..Sprite::new(texture, Rect::new(0.0, 0.0, 1.0, 1.0))
    });
    let frame = render(&mut list, &textures, 3, 3);
    assert_eq!(at(&frame, 1, 1), WHITE);
    let yellow = [255, 255, 0, 255];
    assert_eq!(
        [
            at(&frame, 0, 1),
            at(&frame, 2, 1),
            at(&frame, 1, 0),
            at(&frame, 1, 2)
        ],
        [yellow; 4]
    );
    assert_eq!(at(&frame, 0, 0), BLACK);
}

#[test]
fn shapes_fill_and_stroke() {
    let textures = Textures::new();
    let mut list = list();
    list.shape(
        Shape::Rect {
            rect: Rect::new(0.0, 0.0, 4.0, 4.0),
            stroke: Some(1.0),
        },
        Color::WHITE,
        0,
    );
    list.shape(
        Shape::Circle {
            center: Vec2::new(8.0, 2.0),
            radius: 1.5,
            stroke: None,
        },
        Color::rgba8(255, 0, 0, 255),
        0,
    );
    list.shape(
        Shape::Line {
            from: Vec2::new(0.0, 6.5),
            to: Vec2::new(10.0, 6.5),
            width: 1.0,
        },
        Color::rgba8(0, 0, 255, 255),
        0,
    );
    list.shape(
        Shape::Polygon {
            points: vec![
                Vec2::new(0.0, 8.0),
                Vec2::new(4.0, 8.0),
                Vec2::new(4.0, 10.0),
                Vec2::new(0.0, 10.0),
            ],
            stroke: None,
        },
        Color::rgba8(0, 255, 0, 255),
        0,
    );
    let frame = render(&mut list, &textures, 10, 10);
    assert_eq!([at(&frame, 0, 0), at(&frame, 3, 3)], [WHITE; 2]);
    assert_eq!(at(&frame, 1, 1), BLACK);
    assert_eq!(at(&frame, 8, 2), RED);
    assert_eq!(at(&frame, 8, 0), BLACK);
    assert_eq!([at(&frame, 0, 6), at(&frame, 9, 6)], [BLUE; 2]);
    assert_eq!(at(&frame, 5, 5), BLACK);
    assert_eq!([at(&frame, 0, 8), at(&frame, 3, 9)], [GREEN; 2]);
    assert_eq!(at(&frame, 5, 9), BLACK);
}

#[test]
fn layers_stack_and_fades_cover() {
    let textures = Textures::new();
    let mut list = list();
    let square = |rect| Shape::Rect { rect, stroke: None };
    list.shape(square(Rect::new(0.0, 0.0, 2.0, 1.0)), Color::WHITE, 1);
    list.shape(
        square(Rect::new(0.0, 0.0, 1.0, 1.0)),
        Color::rgba8(255, 0, 0, 255),
        0,
    );
    let frame = render(&mut list, &textures, 2, 1);
    assert_eq!([at(&frame, 0, 0), at(&frame, 1, 0)], [WHITE; 2]);
    let mut faded = self::list();
    faded.background = Color::WHITE;
    faded.fade = Some(Color::new(0.0, 0.0, 0.0, 0.5));
    let frame = render(&mut faded, &textures, 1, 1);
    assert_eq!(at(&frame, 0, 0), [128, 128, 128, 255]);
}
