use ox_library::color::Color;
use ox_library::geometry::Rect;
use ox_library::geometry::Vec2;
use rodeo::draw::{Batch, BatchKind, Command, DrawList, Shape, Sprite, Text};
use rodeo::texture::{TextureId, Textures};

fn two_textures() -> (TextureId, TextureId) {
    let mut textures = Textures::new();
    (
        textures.insert(1, 1, vec![0; 4]).unwrap(),
        textures.insert(1, 1, vec![0; 4]).unwrap(),
    )
}

fn sprite(texture: TextureId, x: f64, layer: i32) -> Sprite {
    Sprite {
        position: Vec2::new(x, 0.0),
        layer,
        ..Sprite::new(texture, Rect::new(0.0, 0.0, 1.0, 1.0))
    }
}

#[test]
fn layers_sort_while_keeping_the_order_things_were_drawn() {
    let (a, b) = two_textures();
    let mut list = DrawList::new();
    list.sprite(sprite(a, 1.0, 0));
    list.sprite(sprite(a, 2.0, 0));
    list.sprite(sprite(b, 3.0, 0));
    list.sprite(sprite(a, 4.0, 0));
    list.shape(
        Shape::Rect {
            rect: Rect::new(0.0, 0.0, 4.0, 4.0),
            stroke: None,
        },
        Color::BLACK,
        0,
    );
    list.sprite(sprite(a, 5.0, -1));
    list.text(Text::new("Score", Vec2::new(2.0, 2.0)), 2);
    list.sprite(sprite(b, 6.0, 2));
    assert_eq!(list.len(), 8);
    let batches = list.finish();
    assert_eq!(
        batches,
        [
            Batch {
                layer: -1,
                kind: BatchKind::Sprites(a),
                range: 0..1
            },
            Batch {
                layer: 0,
                kind: BatchKind::Sprites(a),
                range: 1..3
            },
            Batch {
                layer: 0,
                kind: BatchKind::Sprites(b),
                range: 3..4
            },
            Batch {
                layer: 0,
                kind: BatchKind::Sprites(a),
                range: 4..5
            },
            Batch {
                layer: 0,
                kind: BatchKind::Shapes,
                range: 5..6
            },
            Batch {
                layer: 2,
                kind: BatchKind::Text,
                range: 6..7
            },
            Batch {
                layer: 2,
                kind: BatchKind::Sprites(b),
                range: 7..8
            },
        ]
    );
    let order: Vec<f64> = list
        .commands()
        .iter()
        .filter_map(|command| match command {
            Command::Sprite(sprite) => Some(sprite.position.x),
            _ => None,
        })
        .collect();
    assert_eq!(order, [5.0, 1.0, 2.0, 3.0, 4.0, 6.0]);
}

#[test]
fn lists_clear_between_frames_and_hold_screen_effects() {
    let (a, _) = two_textures();
    let mut list = DrawList::new();
    list.sprite(sprite(a, 0.0, 0));
    list.fade = Some(Color::new(0.0, 0.0, 0.0, 0.5));
    list.background = Color::rgba8(10, 20, 30, 255);
    let palette = list.palette(vec![Color::WHITE], vec![Color::BLACK]);
    assert_eq!(palette, 0);
    assert_eq!(list.palettes()[0].to, [Color::BLACK]);
    list.clear();
    assert!(list.is_empty());
    assert_eq!(list.fade, None);
    assert_eq!(list.palettes(), []);
    assert_eq!(list.background, Color::rgba8(10, 20, 30, 255));
    assert_eq!(list.finish(), []);
}
