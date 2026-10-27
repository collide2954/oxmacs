use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

use ox_library::color::Color;
use ox_library::geometry::Vec2;
use ox_rodeo::particles::Emitter;

fn still() -> Emitter {
    let mut emitter = Emitter::new(1);
    emitter.speed = [10.0, 10.0];
    emitter.spread = 0.0;
    emitter.angle = 0.0;
    emitter.lifetime = [1.0, 1.0];
    emitter
}

#[test]
fn bursts_spawn_at_the_emitter_heading_its_way() {
    let mut emitter = still();
    emitter.position = Vec2::new(5.0, 6.0);
    emitter.burst(5);
    assert_eq!(emitter.particles().len(), 5);
    for particle in emitter.particles() {
        assert_eq!(particle.position, Vec2::new(5.0, 6.0));
        assert_eq!(particle.velocity, Vec2::new(10.0, 0.0));
    }
    emitter.clear();
    emitter.angle = -FRAC_PI_2;
    emitter.spread = FRAC_PI_2;
    emitter.burst(50);
    for particle in emitter.particles() {
        assert!(particle.velocity.y < 0.0);
        assert!((particle.velocity.angle() + FRAC_PI_2).abs() <= FRAC_PI_4 + 1e-9);
    }
}

#[test]
fn particles_move_age_and_die() {
    let mut emitter = still();
    emitter.burst(3);
    emitter.update(0.5);
    assert_eq!(emitter.particles()[0].position, Vec2::new(5.0, 0.0));
    assert_eq!(emitter.particles()[0].progress(), 0.5);
    emitter.update(0.6);
    assert_eq!(emitter.particles(), []);
}

#[test]
fn emitters_spawn_at_their_rate() {
    let mut emitter = still();
    emitter.lifetime = [10.0, 10.0];
    emitter.rate = 30.0;
    for _ in 0..60 {
        emitter.update(1.0 / 60.0);
    }
    assert_eq!(emitter.particles().len(), 30);
    emitter.emitting = false;
    emitter.update(1.0);
    assert_eq!(emitter.particles().len(), 30);
}

#[test]
fn gravity_pulls_and_drag_slows() {
    let mut falling = still();
    falling.speed = [0.0, 0.0];
    falling.gravity = Vec2::new(0.0, 100.0);
    falling.burst(1);
    falling.update(0.5);
    assert_eq!(falling.particles()[0].velocity, Vec2::new(0.0, 50.0));
    assert_eq!(falling.particles()[0].position, Vec2::new(0.0, 25.0));
    let mut slowed = still();
    slowed.drag = 1.0;
    slowed.burst(1);
    slowed.update(0.5);
    assert_eq!(slowed.particles()[0].velocity, Vec2::new(5.0, 0.0));
}

#[test]
fn size_and_color_change_over_a_lifetime() {
    let mut emitter = still();
    emitter.size = [4.0, 0.0];
    emitter.colors = [Color::WHITE, Color::TRANSPARENT];
    emitter.burst(1);
    emitter.update(0.5);
    let particle = emitter.particles()[0];
    assert_eq!(emitter.size_of(&particle), 2.0);
    assert_eq!(emitter.color_of(&particle).a, 0.5);
}

#[test]
fn limits_cap_and_seeds_repeat() {
    let mut emitter = still();
    emitter.limit = 4;
    emitter.burst(10);
    assert_eq!(emitter.particles().len(), 4);
    let spray = |seed| {
        let mut emitter = Emitter::new(seed);
        emitter.burst(5);
        emitter.particles().to_vec()
    };
    assert_eq!(spray(7), spray(7));
    assert_ne!(spray(7), spray(8));
}
