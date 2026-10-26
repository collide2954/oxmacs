use rodeo::collide::{Circle, penetration};
use rodeo::rect::Rect;
use rodeo::vec::Vec2;

#[test]
fn circles_overlap_circles_rects_and_points() {
    let circle = Circle::new(Vec2::new(0.0, 0.0), 5.0);
    assert!(circle.overlaps(Circle::new(Vec2::new(8.0, 0.0), 4.0)));
    assert!(!circle.overlaps(Circle::new(Vec2::new(9.0, 0.0), 4.0)));
    assert!(circle.contains(Vec2::new(3.0, 3.0)));
    assert!(!circle.contains(Vec2::new(4.0, 4.0)));
    assert!(circle.overlaps_rect(Rect::new(4.0, -1.0, 10.0, 2.0)));
    assert!(!circle.overlaps_rect(Rect::new(4.0, 4.0, 10.0, 10.0)));
    assert!(circle.overlaps_rect(Rect::new(-1.0, -1.0, 2.0, 2.0)));
    assert_eq!(circle.bounds(), Rect::new(-5.0, -5.0, 10.0, 10.0));
}

#[test]
fn penetration_pushes_out_along_the_shallowest_axis() {
    let wall = Rect::new(10.0, 0.0, 10.0, 100.0);
    assert_eq!(
        penetration(Rect::new(8.0, 20.0, 4.0, 4.0), wall),
        Some(Vec2::new(-2.0, 0.0))
    );
    assert_eq!(
        penetration(Rect::new(19.0, 20.0, 4.0, 4.0), wall),
        Some(Vec2::new(1.0, 0.0))
    );
    let floor = Rect::new(0.0, 50.0, 100.0, 10.0);
    assert_eq!(
        penetration(Rect::new(20.0, 47.0, 4.0, 4.0), floor),
        Some(Vec2::new(0.0, -1.0))
    );
    assert_eq!(penetration(Rect::new(0.0, 0.0, 4.0, 4.0), floor), None);
}
