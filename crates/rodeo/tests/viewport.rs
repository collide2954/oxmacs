use ox_library::geometry::Vec2;
use rodeo::draw::Viewport;

const GAME: Vec2 = Vec2::new(320.0, 180.0);

#[test]
fn games_scale_by_whole_numbers_and_letterbox() {
    let exact = Viewport::fit(GAME, Vec2::new(1280.0, 720.0), false);
    assert_eq!(exact.scale, 4.0);
    assert_eq!(exact.offset, Vec2::ZERO);
    let wide = Viewport::fit(GAME, Vec2::new(1366.0, 768.0), false);
    assert_eq!(wide.scale, 4.0);
    assert_eq!(wide.offset, Vec2::new(43.0, 24.0));
    assert_eq!(wide.size, Vec2::new(1280.0, 720.0));
}

#[test]
fn smooth_games_and_small_windows_scale_by_fractions() {
    let smooth = Viewport::fit(GAME, Vec2::new(1366.0, 768.0), true);
    assert!((smooth.scale - 768.0 / 180.0).abs() < 1e-9);
    assert!((smooth.offset.x - (1366.0 - 320.0 * smooth.scale) / 2.0).abs() < 1e-9);
    assert_eq!(smooth.offset.y, 0.0);
    let small = Viewport::fit(GAME, Vec2::new(200.0, 100.0), false);
    assert!((small.scale - 100.0 / 180.0).abs() < 1e-9);
}

#[test]
fn window_points_map_into_the_game() {
    let wide = Viewport::fit(GAME, Vec2::new(1366.0, 768.0), false);
    assert_eq!(
        wide.to_game(Vec2::new(43.0 + 40.0, 24.0 + 20.0)),
        Some(Vec2::new(10.0, 5.0))
    );
    assert_eq!(wide.to_game(Vec2::new(10.0, 300.0)), None);
    assert_eq!(wide.to_window(Vec2::new(10.0, 5.0)), Vec2::new(83.0, 44.0));
}
