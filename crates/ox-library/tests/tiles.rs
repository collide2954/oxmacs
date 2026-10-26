use ox_library::collide::move_in_tiles;
use ox_library::geometry::Rect;
use ox_library::geometry::Vec2;

const TILE: Vec2 = Vec2::new(16.0, 16.0);

fn level(column: i64, row: i64) -> bool {
    row == 4 || column == 6 || (row == 0 && column < 6)
}

#[test]
fn falling_boxes_land_on_solid_tiles() {
    let moved = move_in_tiles(
        Rect::new(20.0, 30.0, 10.0, 12.0),
        Vec2::new(0.0, 40.0),
        TILE,
        level,
    );
    assert_eq!(moved.rect, Rect::new(20.0, 52.0, 10.0, 12.0));
    assert!(moved.on_floor());
    assert_eq!(moved.contacts[0].index, (1, 4));
}

#[test]
fn running_boxes_cross_seams_and_stop_at_walls() {
    let moved = move_in_tiles(
        Rect::new(20.0, 52.0, 10.0, 12.0),
        Vec2::new(200.0, 1.0),
        TILE,
        level,
    );
    assert_eq!(moved.rect, Rect::new(86.0, 52.0, 10.0, 12.0));
    assert!(moved.on_floor());
    assert!(moved.on_wall());
}

#[test]
fn jumping_boxes_bump_their_heads() {
    let moved = move_in_tiles(
        Rect::new(20.0, 30.0, 10.0, 12.0),
        Vec2::new(8.0, -56.0),
        TILE,
        level,
    );
    assert_eq!(moved.rect, Rect::new(28.0, 16.0, 10.0, 12.0));
    assert!(moved.on_ceiling());
}
