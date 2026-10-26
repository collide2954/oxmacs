use rodeo::collide::{Contact, slide, sweep};
use rodeo::rect::Rect;
use rodeo::vec::Vec2;

fn unit() -> Rect {
    Rect::new(0.0, 0.0, 10.0, 10.0)
}

#[test]
fn a_box_moving_into_a_wall_hits_it_partway() {
    let wall = Rect::new(15.0, 0.0, 10.0, 10.0);
    let hit = sweep(unit(), Vec2::new(20.0, 0.0), wall).unwrap();
    assert_eq!(hit.time, 0.25);
    assert_eq!(hit.normal, Vec2::new(-1.0, 0.0));
    assert_eq!(
        sweep(
            unit(),
            Vec2::new(20.0, 0.0),
            Rect::new(15.0, 20.0, 10.0, 10.0)
        ),
        None
    );
    assert_eq!(sweep(unit(), Vec2::new(4.0, 0.0), wall), None);
}

#[test]
fn boxes_touching_a_surface_only_hit_it_when_moving_into_it() {
    let floor = Rect::new(0.0, 10.0, 100.0, 10.0);
    assert_eq!(
        sweep(unit(), Vec2::new(0.0, 1.0), floor).map(|hit| (hit.time, hit.normal)),
        Some((0.0, Vec2::new(0.0, -1.0)))
    );
    assert_eq!(sweep(unit(), Vec2::new(0.0, -1.0), floor), None);
    assert_eq!(sweep(unit(), Vec2::new(5.0, 0.0), floor), None);
}

#[test]
fn sliding_moves_along_surfaces_instead_of_stopping() {
    let floor = Rect::new(0.0, 10.0, 100.0, 10.0);
    let moved = slide(unit(), Vec2::new(5.0, 3.0), &[floor]);
    assert_eq!(moved.rect, Rect::new(5.0, 0.0, 10.0, 10.0));
    assert_eq!(
        moved.contacts,
        [Contact {
            index: 0,
            normal: Vec2::new(0.0, -1.0)
        }]
    );
    assert!(moved.on_floor());
    assert!(!moved.on_wall());
    assert!(!moved.on_ceiling());
}

#[test]
fn sliding_crosses_tile_seams_and_stops_at_walls() {
    let mut solids: Vec<Rect> = (0..5)
        .map(|column| Rect::new(f64::from(column) * 16.0, 32.0, 16.0, 16.0))
        .collect();
    solids.push(Rect::new(64.0, 0.0, 16.0, 32.0));
    let moved = slide(
        Rect::new(0.0, 22.0, 10.0, 10.0),
        Vec2::new(100.0, 2.0),
        &solids,
    );
    assert_eq!(moved.rect, Rect::new(54.0, 22.0, 10.0, 10.0));
    assert!(moved.on_floor());
    assert!(moved.on_wall());
}

#[test]
fn boxes_starting_inside_a_solid_can_leave_it() {
    let solid = Rect::new(0.0, 0.0, 20.0, 20.0);
    let moved = slide(
        Rect::new(5.0, 5.0, 4.0, 4.0),
        Vec2::new(30.0, 0.0),
        &[solid],
    );
    assert_eq!(moved.rect, Rect::new(35.0, 5.0, 4.0, 4.0));
    assert_eq!(moved.contacts, []);
}
