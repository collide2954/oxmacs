use std::time::Duration;

use rodeo::clock::Clock;

fn sixtieths(count: f64) -> Duration {
    Duration::from_secs_f64(count / 60.0)
}

#[test]
fn a_clock_ticks_sixty_times_a_second_by_default() {
    let mut clock = Clock::default();
    assert_eq!(clock.step(), Duration::from_nanos(16_666_666));
    assert_eq!(clock.advance(sixtieths(1.0)), 1);
    assert_eq!(clock.advance(sixtieths(3.0)), 3);
    assert_eq!(clock.ticks(), 4);
    assert!(clock.alpha() < 0.001);
}

#[test]
fn leftover_time_carries_over_and_sets_alpha() {
    let mut clock = Clock::default();
    assert_eq!(clock.advance(Duration::from_nanos(8_333_333)), 0);
    assert!((clock.alpha() - 0.5).abs() < 0.001);
    assert_eq!(clock.advance(Duration::from_nanos(8_333_334)), 1);
    assert!(clock.alpha() < 0.001);
}

#[test]
fn long_frames_are_capped_so_the_game_never_spirals() {
    let mut clock = Clock::default();
    assert_eq!(clock.advance(Duration::from_secs(5)), 15);
}

#[test]
fn a_paused_clock_only_runs_requested_steps() {
    let mut clock = Clock::default();
    clock.pause();
    assert!(clock.paused());
    assert_eq!(clock.advance(Duration::from_secs(1)), 0);
    clock.step_once();
    clock.step_once();
    assert_eq!(clock.advance(Duration::ZERO), 2);
    assert_eq!(clock.advance(Duration::ZERO), 0);
    clock.resume();
    assert_eq!(clock.advance(sixtieths(1.0)), 1);
}

#[test]
fn slow_motion_and_other_rates_scale_game_time() {
    let mut clock = Clock::default();
    clock.set_scale(0.5);
    assert_eq!(clock.advance(sixtieths(2.0)), 1);
    let mut thirty = Clock::new(30);
    assert_eq!(thirty.advance(sixtieths(2.0)), 1);
    assert_eq!(thirty.ticks(), 1);
    assert!((thirty.seconds() - 1.0 / 30.0).abs() < 1e-6);
}
