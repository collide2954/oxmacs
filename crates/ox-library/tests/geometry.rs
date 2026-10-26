use std::f64::consts::{FRAC_PI_2, PI};

use ox_library::geometry::Rect;
use ox_library::geometry::Vec2;

use crate::support::evaluated;

fn near(a: Vec2, b: Vec2) -> bool {
    (a - b).length() < 1e-9
}

#[test]
fn vectors_add_subtract_and_scale() {
    let a = Vec2::new(3.0, 4.0);
    assert_eq!(a + Vec2::new(1.0, -2.0), Vec2::new(4.0, 2.0));
    assert_eq!(a - Vec2::new(1.0, 1.0), Vec2::new(2.0, 3.0));
    assert_eq!(a * 2.0, Vec2::new(6.0, 8.0));
    assert_eq!(a / 2.0, Vec2::new(1.5, 2.0));
    assert_eq!(-a, Vec2::new(-3.0, -4.0));
    let mut b = a;
    b += Vec2::new(1.0, 1.0);
    b -= Vec2::new(2.0, 2.0);
    b *= 3.0;
    assert_eq!(b, Vec2::new(6.0, 9.0));
}

#[test]
fn lengths_distances_and_products() {
    let a = Vec2::new(3.0, 4.0);
    assert_eq!(a.length(), 5.0);
    assert_eq!(a.length_squared(), 25.0);
    assert_eq!(a.normalized(), Vec2::new(0.6, 0.8));
    assert_eq!(Vec2::ZERO.normalized(), Vec2::ZERO);
    assert_eq!(a.distance(Vec2::ZERO), 5.0);
    assert_eq!(a.dot(Vec2::new(2.0, 1.0)), 10.0);
    assert_eq!(a.cross(Vec2::new(2.0, 1.0)), -5.0);
}

#[test]
fn angles_turn_clockwise_because_y_points_down() {
    assert!(near(Vec2::from_angle(FRAC_PI_2), Vec2::new(0.0, 1.0)));
    assert_eq!(Vec2::new(0.0, 1.0).angle(), FRAC_PI_2);
    assert!(near(
        Vec2::new(1.0, 0.0).rotated(FRAC_PI_2),
        Vec2::new(0.0, 1.0)
    ));
    assert!(near(Vec2::new(1.0, 0.0).rotated(PI), Vec2::new(-1.0, 0.0)));
    assert_eq!(Vec2::new(1.0, 0.0).perpendicular(), Vec2::new(0.0, 1.0));
}

#[test]
fn lerp_clamp_reflect_and_move_toward() {
    let a = Vec2::ZERO;
    assert_eq!(a.lerp(Vec2::new(10.0, 20.0), 0.25), Vec2::new(2.5, 5.0));
    assert_eq!(Vec2::new(30.0, 40.0).clamp_length(5.0), Vec2::new(3.0, 4.0));
    assert_eq!(Vec2::new(3.0, 4.0).clamp_length(10.0), Vec2::new(3.0, 4.0));
    assert_eq!(
        Vec2::new(2.0, 3.0).reflect(Vec2::new(0.0, -1.0)),
        Vec2::new(2.0, -3.0)
    );
    assert_eq!(
        a.move_toward(Vec2::new(10.0, 0.0), 4.0),
        Vec2::new(4.0, 0.0)
    );
    assert_eq!(a.move_toward(Vec2::new(3.0, 0.0), 4.0), Vec2::new(3.0, 0.0));
}

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

fn geometry(source: &str) -> String {
    evaluated("(ox geometry)", source)
}

#[test]
fn points_add_subtract_and_scale_from_ox() {
    assert_eq!(
        geometry("(geometry/add [1 2] [3 4] [0.5 0.5])"),
        "[4.5 6.5]"
    );
    assert_eq!(geometry("(geometry/add [1 2])"), "[1.0 2.0]");
    assert_eq!(geometry("(geometry/sub [5 5] [1 2])"), "[4.0 3.0]");
    assert_eq!(geometry("(geometry/scale [1 2] 3)"), "[3.0 6.0]");
    assert_eq!(geometry("(geometry/add {x: 1 y: 2} [1 1])"), "[2.0 3.0]");
}

#[test]
fn vectors_measure_and_turn() {
    assert_eq!(geometry("(geometry/length [3 4])"), "5.0");
    assert_eq!(geometry("(geometry/normalize [3 4])"), "[0.6 0.8]");
    assert_eq!(geometry("(geometry/dot [3 4] [2 1])"), "10.0");
    assert_eq!(geometry("(geometry/cross [3 4] [2 1])"), "-5.0");
    assert_eq!(geometry("(geometry/distance [0 0] [3 4])"), "5.0");
    assert_eq!(geometry("(geometry/angle [0 1])"), "1.5707963267948966");
    assert_eq!(geometry("(geometry/from-angle 0)"), "[1.0 0.0]");
    assert_eq!(geometry("(geometry/from-angle 0 2)"), "[2.0 0.0]");
    assert_eq!(geometry("(geometry/rotate [2 0] 0)"), "[2.0 0.0]");
    assert_eq!(geometry("(geometry/perpendicular [1 0])"), "[0.0 1.0]");
}

#[test]
fn vectors_mix_clamp_reflect_and_approach() {
    assert_eq!(geometry("(geometry/lerp [0 0] [10 20] 0.25)"), "[2.5 5.0]");
    assert_eq!(geometry("(geometry/clamp-length [30 40] 5)"), "[3.0 4.0]");
    assert_eq!(geometry("(geometry/reflect [2 3] [0 -1])"), "[2.0 -3.0]");
    assert_eq!(
        geometry("(geometry/move-toward [0 0] [10 0] 4)"),
        "[4.0 0.0]"
    );
}

#[test]
fn bad_vectors_explain_themselves() {
    assert_eq!(
        geometry("(geometry/add [1] [2 3])"),
        "error: geometry/add expects a point as [x y] or a map of :x and :y, got [1]"
    );
    assert_eq!(
        geometry("(geometry/scale [1 2] :big)"),
        "error: geometry/scale expects a number, got :big"
    );
}

#[test]
fn rects_know_their_parts() {
    assert_eq!(geometry("(geometry/center [10 20 30 40])"), "[25.0 40.0]");
    assert_eq!(geometry("(geometry/position [10 20 30 40])"), "[10.0 20.0]");
    assert_eq!(
        geometry("(geometry/size {x: 10 y: 20 w: 30 h: 40})"),
        "[30.0 40.0]"
    );
    assert_eq!(
        geometry(
            "[(geometry/left [10 20 30 40]) (geometry/top [10 20 30 40]) (geometry/right [10 20 30 40]) (geometry/bottom [10 20 30 40])]"
        ),
        "[10.0 20.0 40.0 60.0]"
    );
    assert_eq!(
        geometry("(geometry/centered [25 40] [30 40])"),
        "[10.0 20.0 30.0 40.0]"
    );
}

#[test]
fn rects_overlap_contain_and_combine() {
    assert_eq!(
        geometry("(geometry/overlaps? [0 0 10 10] [5 5 10 10])"),
        "true"
    );
    assert_eq!(
        geometry("(geometry/overlaps? [0 0 10 10] [10 0 10 10])"),
        "false"
    );
    assert_eq!(geometry("(geometry/contains? [0 0 10 10] [0 0])"), "true");
    assert_eq!(geometry("(geometry/contains? [0 0 10 10] [10 5])"), "false");
    assert_eq!(
        geometry("(geometry/intersection [0 0 10 10] [5 5 10 10])"),
        "[5.0 5.0 5.0 5.0]"
    );
    assert_eq!(
        geometry("(geometry/intersection [0 0 1 1] [5 5 1 1])"),
        "nil"
    );
    assert_eq!(
        geometry("(geometry/union [0 0 10 10] [20 -5 5 5])"),
        "[0.0 -5.0 25.0 15.0]"
    );
}

#[test]
fn rects_move_grow_and_clamp() {
    assert_eq!(
        geometry("(geometry/translate [0 0 10 10] [2 3])"),
        "[2.0 3.0 10.0 10.0]"
    );
    assert_eq!(
        geometry("(geometry/expand [0 0 10 10] 2)"),
        "[-2.0 -2.0 14.0 14.0]"
    );
    assert_eq!(
        geometry("(geometry/clamp [0 0 10 10] [15 -4])"),
        "[10.0 0.0]"
    );
    assert_eq!(
        geometry("(geometry/center [1 2 3])"),
        "error: geometry/center expects a rect as [x y w h] or a map of :x, :y, :w and :h, got [1 2 3]"
    );
}
