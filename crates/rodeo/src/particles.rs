use std::f64::consts::{FRAC_PI_2, TAU};

use crate::color::Color;
use crate::ease::Ease;
use crate::random::Random;
use crate::vec::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub position: Vec2,
    pub velocity: Vec2,
    pub rotation: f64,
    pub spin: f64,
    pub age: f64,
    pub lifetime: f64,
}

impl Particle {
    pub fn progress(&self) -> f64 {
        if self.lifetime > 0.0 {
            (self.age / self.lifetime).min(1.0)
        } else {
            1.0
        }
    }
}

#[derive(Clone, Debug)]
pub struct Emitter {
    pub position: Vec2,
    pub area: Vec2,
    pub rate: f64,
    pub lifetime: [f64; 2],
    pub speed: [f64; 2],
    pub angle: f64,
    pub spread: f64,
    pub gravity: Vec2,
    pub drag: f64,
    pub size: [f64; 2],
    pub colors: [Color; 2],
    pub spin: [f64; 2],
    pub ease: Ease,
    pub limit: usize,
    pub emitting: bool,
    random: Random,
    owed: f64,
    particles: Vec<Particle>,
}

impl Emitter {
    pub fn new(seed: u64) -> Self {
        Self {
            position: Vec2::ZERO,
            area: Vec2::ZERO,
            rate: 0.0,
            lifetime: [1.0, 1.0],
            speed: [20.0, 40.0],
            angle: -FRAC_PI_2,
            spread: TAU,
            gravity: Vec2::ZERO,
            drag: 0.0,
            size: [4.0, 4.0],
            colors: [Color::WHITE, Color::WHITE],
            spin: [0.0, 0.0],
            ease: Ease::Linear,
            limit: 1000,
            emitting: true,
            random: Random::new(seed),
            owed: 0.0,
            particles: Vec::new(),
        }
    }

    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    pub fn clear(&mut self) {
        self.particles.clear();
        self.owed = 0.0;
    }

    fn between(&mut self, [low, high]: [f64; 2]) -> f64 {
        self.random.range(low, high)
    }

    fn spawn(&mut self) {
        if self.particles.len() >= self.limit {
            return;
        }
        let half = self.area / 2.0;
        let offset = Vec2::new(
            self.between([-half.x, half.x]),
            self.between([-half.y, half.y]),
        );
        let heading = self.angle + self.between([-self.spread / 2.0, self.spread / 2.0]);
        let speed = self.between(self.speed);
        let spin = self.between(self.spin);
        let lifetime = self.between(self.lifetime);
        self.particles.push(Particle {
            position: self.position + offset,
            velocity: Vec2::from_angle(heading) * speed,
            rotation: 0.0,
            spin,
            age: 0.0,
            lifetime,
        });
    }

    pub fn burst(&mut self, count: usize) {
        for _ in 0..count {
            self.spawn();
        }
    }

    pub fn update(&mut self, seconds: f64) {
        let damping = (1.0 - self.drag * seconds).max(0.0);
        for particle in &mut self.particles {
            particle.age += seconds;
            particle.velocity += self.gravity * seconds;
            particle.velocity *= damping;
            particle.position += particle.velocity * seconds;
            particle.rotation += particle.spin * seconds;
        }
        self.particles
            .retain(|particle| particle.age < particle.lifetime);
        if self.emitting {
            self.owed += self.rate * seconds;
            while self.owed >= 1.0 {
                self.owed -= 1.0;
                self.spawn();
            }
        }
    }

    fn eased(&self, particle: &Particle) -> f64 {
        self.ease.apply(particle.progress())
    }

    pub fn size_of(&self, particle: &Particle) -> f64 {
        let [start, end] = self.size;
        (end - start).mul_add(self.eased(particle), start)
    }

    pub fn color_of(&self, particle: &Particle) -> Color {
        let [start, end] = self.colors;
        #[expect(clippy::cast_possible_truncation, reason = "progress runs from 0 to 1")]
        let progress = self.eased(particle) as f32;
        start.lerp(end, progress)
    }
}
