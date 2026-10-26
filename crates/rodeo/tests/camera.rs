use rodeo::camera::Camera;
use rodeo::rect::Rect;
use rodeo::vec::Vec2;

fn near(a: Vec2, b: Vec2) -> bool {
    (a - b).length() < 1e-9
}

fn view() -> Camera {
    Camera::new(Vec2::new(320.0, 180.0))
}

#[test]
fn a_new_camera_shows_the_world_from_its_top_left_corner() {
    let camera = view();
    assert_eq!(camera.position, Vec2::new(160.0, 90.0));
    assert_eq!(camera.world_to_screen(Vec2::ZERO), Vec2::ZERO);
    assert_eq!(camera.view_rect(), Rect::new(0.0, 0.0, 320.0, 180.0));
}

#[test]
fn zoom_scales_around_the_camera_position() {
    let mut camera = view();
    camera.zoom = 2.0;
    assert_eq!(
        camera.world_to_screen(Vec2::new(170.0, 90.0)),
        Vec2::new(180.0, 90.0)
    );
    assert_eq!(camera.view_rect(), Rect::new(80.0, 45.0, 160.0, 90.0));
    let point = Vec2::new(12.5, 300.0);
    assert!(near(
        camera.screen_to_world(camera.world_to_screen(point)),
        point
    ));
}

#[test]
fn following_moves_part_of_the_way_outside_the_deadzone() {
    let mut camera = view();
    camera.follow(Vec2::new(260.0, 90.0), 0.5);
    assert_eq!(camera.position, Vec2::new(210.0, 90.0));
    camera.follow(Vec2::new(260.0, 90.0), 1.0);
    assert_eq!(camera.position, Vec2::new(260.0, 90.0));
    camera.deadzone = Vec2::new(40.0, 20.0);
    camera.follow(Vec2::new(275.0, 95.0), 1.0);
    assert_eq!(camera.position, Vec2::new(260.0, 90.0));
    camera.follow(Vec2::new(300.0, 90.0), 1.0);
    assert_eq!(camera.position, Vec2::new(280.0, 90.0));
}

#[test]
fn bounds_keep_the_view_inside_the_level() {
    let mut camera = view();
    camera.bounds = Some(Rect::new(0.0, 0.0, 640.0, 360.0));
    camera.follow(Vec2::new(-50.0, 1000.0), 1.0);
    assert_eq!(camera.position, Vec2::new(160.0, 270.0));
    camera.bounds = Some(Rect::new(0.0, 0.0, 100.0, 360.0));
    camera.follow(Vec2::new(500.0, 0.0), 1.0);
    assert_eq!(camera.position, Vec2::new(50.0, 90.0));
}

#[test]
fn shaking_jitters_the_view_and_settles() {
    let mut camera = view();
    let still = camera.world_to_screen(Vec2::ZERO);
    camera.shake(1.0);
    camera.tick(1.0 / 60.0);
    let shaken = camera.world_to_screen(Vec2::ZERO);
    assert_ne!(shaken, still);
    assert!((shaken - still).length() <= camera.shake_strength * 2f64.sqrt());
    let mut twin = view();
    twin.shake(1.0);
    twin.tick(1.0 / 60.0);
    assert_eq!(twin.world_to_screen(Vec2::ZERO), shaken);
    for _ in 0..120 {
        camera.tick(1.0 / 60.0);
    }
    assert_eq!(camera.trauma, 0.0);
    assert_eq!(camera.world_to_screen(Vec2::ZERO), still);
}
