use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn from_angle(radians: f64) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(cos, sin)
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    pub fn normalized(self) -> Self {
        let length = self.length();
        if length == 0.0 {
            Self::ZERO
        } else {
            self / length
        }
    }

    pub fn distance(self, other: Self) -> f64 {
        (other - self).length()
    }

    pub fn dot(self, other: Self) -> f64 {
        self.x.mul_add(other.x, self.y * other.y)
    }

    pub fn cross(self, other: Self) -> f64 {
        self.x.mul_add(other.y, -(self.y * other.x))
    }

    pub fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }

    pub fn rotated(self, radians: f64) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(
            self.x.mul_add(cos, -(self.y * sin)),
            self.x.mul_add(sin, self.y * cos),
        )
    }

    pub fn perpendicular(self) -> Self {
        Self::new(-self.y, self.x)
    }

    pub fn lerp(self, other: Self, t: f64) -> Self {
        self + (other - self) * t
    }

    pub fn clamp_length(self, most: f64) -> Self {
        let length = self.length();
        if length > most {
            self / length * most
        } else {
            self
        }
    }

    pub fn reflect(self, normal: Self) -> Self {
        self - normal * (2.0 * self.dot(normal))
    }

    pub fn move_toward(self, target: Self, step: f64) -> Self {
        let gap = target - self;
        let distance = gap.length();
        if distance <= step || distance == 0.0 {
            target
        } else {
            self + gap / distance * step
        }
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Self;

    fn mul(self, factor: f64) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }
}

impl Div<f64> for Vec2 {
    type Output = Self;

    fn div(self, divisor: f64) -> Self {
        Self::new(self.x / divisor, self.y / divisor)
    }
}

impl Neg for Vec2 {
    type Output = Self;

    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl MulAssign<f64> for Vec2 {
    fn mul_assign(&mut self, factor: f64) {
        *self = *self * factor;
    }
}
