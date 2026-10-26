use std::ops::Mul;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

impl Color {
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        let unit = |byte: u8| f32::from(byte) / 255.0;
        Self::new(unit(r), unit(g), unit(b), unit(a))
    }

    pub fn to_rgba8(self) -> [u8; 4] {
        let byte = |channel: f32| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped to 0 through 255 first"
            )]
            let byte = (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
            byte
        };
        [byte(self.r), byte(self.g), byte(self.b), byte(self.a)]
    }

    pub fn hex(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        if !digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
            return None;
        }
        let pair = |at: usize| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok();
        let single = |at: usize| {
            let value = u8::from_str_radix(digits.get(at..=at)?, 16).ok()?;
            Some(value * 17)
        };
        let channels = match digits.len() {
            3 => [single(0)?, single(1)?, single(2)?, 255],
            4 => [single(0)?, single(1)?, single(2)?, single(3)?],
            6 => [pair(0)?, pair(2)?, pair(4)?, 255],
            8 => [pair(0)?, pair(2)?, pair(4)?, pair(6)?],
            _ => return None,
        };
        let [r, g, b, a] = channels;
        Some(Self::rgba8(r, g, b, a))
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let mix = |from: f32, to: f32| (to - from).mul_add(t, from);
        Self::new(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
            mix(self.a, other.a),
        )
    }
}

impl Mul for Color {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Self::new(
            self.r * other.r,
            self.g * other.g,
            self.b * other.b,
            self.a * other.a,
        )
    }
}
