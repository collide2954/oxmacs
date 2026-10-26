use ox_library::camera::Camera;
use ox_library::geometry::Rect;
use ox_library::geometry::Vec2;

use crate::support::evaluated;

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

fn camera(source: &str) -> String {
    evaluated("(ox camera)", source)
}

#[test]
fn cameras_are_plain_maps_that_show_the_world() {
    assert_eq!(
        camera(
            "(let [cam (camera/new [320 180])] [cam.position cam.zoom (camera/to-screen cam [0 0]) (camera/view cam)])"
        ),
        "[[160.0 90.0] 1.0 [0.0 0.0] [0.0 0.0 320.0 180.0]]"
    );
    assert_eq!(
        camera("(camera/to-screen (assoc (camera/new [320 180]) :zoom 2) [170 90])"),
        "[180.0 90.0]"
    );
    assert_eq!(
        camera(
            "(let [cam (assoc (camera/new [320 180]) :zoom 2)] (camera/to-world cam (camera/to-screen cam [12.5 300])))"
        ),
        "[12.5 300.0]"
    );
}

#[test]
fn cameras_follow_within_their_bounds() {
    assert_eq!(
        camera("(get (camera/follow (camera/new [320 180]) [260 90] 0.5) :position)"),
        "[210.0 90.0]"
    );
    assert_eq!(
        camera(
            "(get (camera/follow (assoc (camera/new [320 180]) :bounds [0 0 640 360]) [-50 1000] 1) :position)"
        ),
        "[160.0 270.0]"
    );
    assert_eq!(
        camera(
            "(get (camera/clamp (assoc (camera/new [320 180]) :bounds [0 0 640 360] :position [0 0])) :position)"
        ),
        "[160.0 90.0]"
    );
}

#[test]
fn cameras_shake_and_settle() {
    assert_eq!(
        camera(
            "(let [cam (camera/tick (camera/shake (camera/new [320 180]) 1) (/ 1 60))] (not= (camera/offset cam) [0.0 0.0]))"
        ),
        "true"
    );
    assert_eq!(
        camera(
            "(let [cam (reduce (fn [cam _] (camera/tick cam (/ 1 60))) (camera/shake (camera/new [320 180]) 1) (range 120))] [cam.trauma (camera/offset cam)])"
        ),
        "[0.0 [0.0 0.0]]"
    );
}
