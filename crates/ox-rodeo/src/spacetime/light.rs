use std::ops::{Add, Mul, Sub};

use ox_library::color::Color;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl V3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    pub fn normalized(self) -> Self {
        let length = self.length();
        if length == 0.0 {
            self
        } else {
            self * (1.0 / length)
        }
    }
}

impl Add for V3 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl Sub for V3 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

impl Mul<f64> for V3 {
    type Output = Self;

    fn mul(self, factor: f64) -> Self {
        Self::new(self.x * factor, self.y * factor, self.z * factor)
    }
}

pub(crate) type Rgb = [f64; 3];

pub(crate) fn scaled(color: Rgb, factor: f64) -> Rgb {
    color.map(|channel| channel * factor)
}

pub(crate) fn added(a: Rgb, b: Rgb) -> Rgb {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn mixed(a: Rgb, b: Rgb, t: f64) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

pub(crate) fn smoothstep(from: f64, to: f64, value: f64) -> f64 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) fn floor_of(value: f64) -> i64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "noise and cell coordinates stay far inside i64"
    )]
    let floored = value.floor().clamp(-1e15, 1e15) as i64;
    floored
}

pub(crate) fn real(value: i64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "noise and cell coordinates stay below 2^52"
    )]
    let real = value as f64;
    real
}

pub(crate) fn hash(a: i64, b: i64, c: i64, seed: u64) -> u64 {
    let mut h = seed
        ^ a.cast_unsigned().wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ b.cast_unsigned().wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ c.cast_unsigned().wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^ (h >> 33)
}

pub(crate) fn unit(h: u64) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "53 bits fit an f64 exactly")]
    let fraction = (h >> 11) as f64 / (1_u64 << 53) as f64;
    fraction
}

fn fade(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

pub(crate) fn noise(point: V3, seed: u64) -> f64 {
    let (x, y, z) = (floor_of(point.x), floor_of(point.y), floor_of(point.z));
    let (fx, fy, fz) = (
        fade(point.x - real(x)),
        fade(point.y - real(y)),
        fade(point.z - real(z)),
    );
    let corner = |dx: i64, dy: i64, dz: i64| unit(hash(x + dx, y + dy, z + dz, seed));
    let along = |dy: i64, dz: i64| corner(0, dy, dz) + (corner(1, dy, dz) - corner(0, dy, dz)) * fx;
    let near = along(0, 0) + (along(1, 0) - along(0, 0)) * fy;
    let far = along(0, 1) + (along(1, 1) - along(0, 1)) * fy;
    near + (far - near) * fz
}

pub(crate) fn fbm(point: V3, octaves: u32, seed: u64) -> f64 {
    let (mut total, mut weight, mut scale, mut sum) = (0.0, 0.5, 1.0, 0.0);
    for octave in 0..octaves {
        total += noise(point * scale, seed.wrapping_add(u64::from(octave))) * weight;
        sum += weight;
        weight *= 0.5;
        scale *= 2.03;
    }
    total / sum
}

fn decoded(channel: f64) -> f64 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn encoded(channel: f64) -> f64 {
    if channel <= 0.003_130_8 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    }
}

pub(crate) fn blackbody(kelvin: f64) -> Rgb {
    let t = (kelvin / 100.0).clamp(5.0, 400.0);
    let red = if t <= 66.0 {
        255.0
    } else {
        329.698_727_446 * (t - 60.0).powf(-0.133_204_759_2)
    };
    let green = if t <= 66.0 {
        99.470_802_586_1 * t.ln() - 161.119_568_166_1
    } else {
        288.122_169_528_3 * (t - 60.0).powf(-0.075_514_849_2)
    };
    let blue = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7
    };
    [red, green, blue].map(|channel| decoded((channel / 255.0).clamp(0.0, 1.0)))
}

fn filmic(x: f64) -> f64 {
    let x = x.max(0.0);
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

fn narrow(value: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "display channels are between 0 and 1"
    )]
    let narrowed = value as f32;
    narrowed
}

pub(crate) fn displayed(light: Rgb, exposure: f64) -> Color {
    let [r, g, b] = light.map(|channel| narrow(encoded(filmic(channel * exposure))));
    Color::new(r, g, b, 1.0)
}
