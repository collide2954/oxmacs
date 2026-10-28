#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

const DIMMED: f32 = 0.3;

impl Color {
    pub const fn hex(rgb: u32) -> Self {
        let [_, r, g, b] = rgb.to_be_bytes();
        Self { r, g, b }
    }

    pub fn relative_luminance(self) -> f64 {
        let linear = |channel: u8| {
            let c = f64::from(channel) / 255.0;
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }

    pub fn dimmed(self, background: Self) -> Self {
        self.mix(background, DIMMED)
    }

    pub fn mix(self, other: Self, amount: f32) -> Self {
        let amount = if amount.is_nan() {
            0.0
        } else {
            amount.clamp(0.0, 1.0)
        };
        Self {
            r: blend(self.r, other.r, amount),
            g: blend(self.g, other.g, amount),
            b: blend(self.b, other.b, amount),
        }
    }

    pub fn contrast_ratio(self, other: Self) -> f64 {
        let (a, b) = (self.relative_luminance(), other.relative_luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the blend of two channels stays within 0..=255"
)]
fn blend(from: u8, to: u8, amount: f32) -> u8 {
    let from = f32::from(from);
    (from + (f32::from(to) - from) * amount).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_splits_into_channels() {
        assert_eq!(
            Color::hex(0x2E_34_36),
            Color {
                r: 0x2E,
                g: 0x34,
                b: 0x36
            }
        );
    }

    #[test]
    fn relative_luminance_spans_black_to_white() {
        assert!(Color::hex(0x00_00_00).relative_luminance().abs() < 1e-9);
        assert!((Color::hex(0xFF_FF_FF).relative_luminance() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn mixing_blends_channels_linearly() {
        let black = Color::hex(0x00_00_00);
        let white = Color::hex(0xFF_FF_FF);

        assert_eq!(black.mix(white, 0.0), black);
        assert_eq!(black.mix(white, 1.0), white);
        assert_eq!(black.mix(white, 0.5), Color::hex(0x80_80_80));
    }

    #[test]
    fn mixing_clamps_the_amount() {
        let black = Color::hex(0x00_00_00);
        let white = Color::hex(0xFF_FF_FF);

        assert_eq!(black.mix(white, -1.0), black);
        assert_eq!(black.mix(white, 2.0), white);
    }

    #[test]
    fn contrast_ratio_is_symmetric_and_maximal_for_black_on_white() {
        let black = Color::hex(0x00_00_00);
        let white = Color::hex(0xFF_FF_FF);

        assert!((black.contrast_ratio(white) - 21.0).abs() < 1e-9);
        assert!((white.contrast_ratio(black) - 21.0).abs() < 1e-9);
    }
}
