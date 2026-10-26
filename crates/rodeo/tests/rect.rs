use rodeo::rect::Rect;
use rodeo::vec::Vec2;

#[test]
fn rects_know_their_edges_and_center() {
    let rect = Rect::new(10.0, 20.0, 30.0, 40.0);
    assert_eq!(
        [rect.left(), rect.top(), rect.right(), rect.bottom()],
        [10.0, 20.0, 40.0, 60.0]
    );
    assert_eq!(rect.center(), Vec2::new(25.0, 40.0));
    assert_eq!(rect.position(), Vec2::new(10.0, 20.0));
    assert_eq!(rect.size(), Vec2::new(30.0, 40.0));
    assert_eq!(
        Rect::centered(Vec2::new(25.0, 40.0), Vec2::new(30.0, 40.0)),
        rect
    );
}

#[test]
fn points_inside_include_the_top_left_edge_only() {
    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    assert!(rect.contains(Vec2::new(0.0, 0.0)));
    assert!(rect.contains(Vec2::new(9.5, 9.5)));
    assert!(!rect.contains(Vec2::new(10.0, 5.0)));
    assert!(!rect.contains(Vec2::new(-0.1, 5.0)));
}

#[test]
fn touching_rects_do_not_overlap() {
    let a = Rect::new(0.0, 0.0, 10.0, 10.0);
    assert!(a.overlaps(Rect::new(5.0, 5.0, 10.0, 10.0)));
    assert!(!a.overlaps(Rect::new(10.0, 0.0, 10.0, 10.0)));
    assert!(!a.overlaps(Rect::new(0.0, 10.0, 10.0, 10.0)));
    assert_eq!(
        a.intersection(Rect::new(5.0, 5.0, 10.0, 10.0)),
        Some(Rect::new(5.0, 5.0, 5.0, 5.0))
    );
    assert_eq!(a.intersection(Rect::new(20.0, 0.0, 1.0, 1.0)), None);
    assert_eq!(
        a.union(Rect::new(20.0, -5.0, 5.0, 5.0)),
        Rect::new(0.0, -5.0, 25.0, 15.0)
    );
}

#[test]
fn rects_move_grow_and_clamp_points() {
    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    assert_eq!(
        rect.translated(Vec2::new(2.0, 3.0)),
        Rect::new(2.0, 3.0, 10.0, 10.0)
    );
    assert_eq!(rect.expanded(2.0), Rect::new(-2.0, -2.0, 14.0, 14.0));
    assert_eq!(rect.clamp(Vec2::new(15.0, -4.0)), Vec2::new(10.0, 0.0));
}
