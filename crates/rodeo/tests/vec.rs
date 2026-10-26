use std::f64::consts::{FRAC_PI_2, PI};

use rodeo::vec::Vec2;

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
