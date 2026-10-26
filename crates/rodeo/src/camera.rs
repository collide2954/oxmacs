use crate::random::noise;
use crate::rect::Rect;
use crate::vec::Vec2;

#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub position: Vec2,
    pub view: Vec2,
    pub zoom: f64,
    pub bounds: Option<Rect>,
    pub deadzone: Vec2,
    pub trauma: f64,
    pub shake_strength: f64,
    pub shake_decay: f64,
    pub shake_seed: u64,
    pub shake_ticks: u64,
}

impl Camera {
    pub fn new(view: Vec2) -> Self {
        Self {
            position: view / 2.0,
            view,
            zoom: 1.0,
            bounds: None,
            deadzone: Vec2::ZERO,
            trauma: 0.0,
            shake_strength: 8.0,
            shake_decay: 1.5,
            shake_seed: 0,
            shake_ticks: 0,
        }
    }

    fn scale(&self) -> f64 {
        self.zoom.max(f64::MIN_POSITIVE)
    }

    pub fn shake_offset(&self) -> Vec2 {
        if self.trauma <= 0.0 {
            return Vec2::ZERO;
        }
        let amount = self.shake_strength * self.trauma * self.trauma;
        let index = self.shake_ticks.wrapping_mul(2);
        Vec2::new(
            amount * noise(self.shake_seed, index),
            amount * noise(self.shake_seed, index + 1),
        )
    }

    pub fn world_to_screen(&self, point: Vec2) -> Vec2 {
        (point - self.position) * self.scale() + self.view / 2.0 + self.shake_offset()
    }

    pub fn screen_to_world(&self, point: Vec2) -> Vec2 {
        (point - self.view / 2.0 - self.shake_offset()) / self.scale() + self.position
    }

    pub fn view_rect(&self) -> Rect {
        Rect::centered(self.position, self.view / self.scale())
    }

    pub fn follow(&mut self, target: Vec2, amount: f64) {
        let half = self.deadzone / 2.0;
        let gap = target - self.position;
        let outside = |gap: f64, half: f64| {
            if gap.abs() <= half {
                0.0
            } else {
                gap - half.copysign(gap)
            }
        };
        self.position += Vec2::new(outside(gap.x, half.x), outside(gap.y, half.y)) * amount;
        self.clamp_to_bounds();
    }

    pub fn clamp_to_bounds(&mut self) {
        let Some(bounds) = self.bounds else {
            return;
        };
        let half = self.view / (2.0 * self.scale());
        let fit = |position: f64, low: f64, high: f64, half: f64| {
            if high - low <= half * 2.0 {
                f64::midpoint(low, high)
            } else {
                position.max(low + half).min(high - half)
            }
        };
        self.position = Vec2::new(
            fit(self.position.x, bounds.left(), bounds.right(), half.x),
            fit(self.position.y, bounds.top(), bounds.bottom(), half.y),
        );
    }

    pub fn shake(&mut self, amount: f64) {
        self.trauma = (self.trauma + amount).clamp(0.0, 1.0);
    }

    pub fn tick(&mut self, seconds: f64) {
        if self.trauma > 0.0 {
            self.shake_ticks = self.shake_ticks.wrapping_add(1);
            self.trauma = (self.trauma - self.shake_decay * seconds).max(0.0);
        }
    }
}
